use api::TimeInfo;
use gloo::events::EventListener;
use musli_web::web03::prelude::*;
use web_sys::HtmlImageElement;
use yew::prelude::*;

use crate::SetupChannel;
use crate::background::Background;
use crate::error::{CustomContext, Error, Message};
use crate::router::{
    MediaQuery, MediaSelection, Route, Router, ShowDetailQuery, SortField, TrackedFilter,
};
use crate::ui::{Button, Image, MarkTimeMenu, MediaKindToggle, PaginationButtons, TimePreset};

const PAGE_SIZE: usize = 20;

/// Route to the detail view for a list item of the given kind.
fn detail_route(kind: api::MediaKind, id: u64) -> Route {
    match kind {
        api::MediaKind::Shows => {
            Route::ShowDetail(api::ShowId::new(id), ShowDetailQuery::default())
        }
        api::MediaKind::Movies => Route::MovieDetail(api::MovieId::new(id)),
    }
}

pub(crate) struct MediaList {
    channel: ws::Channel,
    items: Vec<api::MediaItem>,
    filter: String,
    page: usize,
    sort: SortField,
    desc: bool,
    tracked: TrackedFilter,
    selection: MediaSelection,
    time: TimeInfo,
    _time_handle: ContextHandle<TimeInfo>,
    background: Background,
    router: Router,
    _setup: SetupChannel,
    _broadcast: ws::Listener,
    list_req: ws::Request,
    _mark_req: ws::Request,
    _track_req: ws::Request,
    /// Backdrop URL last requested as the page background, to avoid re-emitting.
    applied_backdrop: Option<String>,
    /// The image element preloading the next backdrop, kept alive until it loads.
    _preload_img: Option<HtmlImageElement>,
    _preload_load: Option<EventListener>,
}

pub(crate) enum Msg {
    Channel(Result<ws::Channel, ws::Error>),
    AppBroadcast(Result<ws::Packet<api::AppBroadcast>, ws::Error>),
    Loaded(Result<ws::Packet<api::ListMedia>, ws::Error>),
    MarkWatched(u64, api::MarkTime),
    MarkWatchedDone(Result<ws::Packet<api::MarkWatched>, ws::Error>),
    SetTracked(api::MediaKind, u64, bool),
    SetTrackedDone(Result<(), ws::Error>),
    Filter(String),
    SetSort(SortField),
    ToggleDir,
    CycleTracked,
    SetSelection(MediaSelection),
    SetPage(usize),
    Navigate(Route),
    SetTime(TimeInfo),
}

#[derive(Properties, PartialEq)]
pub(crate) struct Props {
    pub(crate) page: usize,
    pub(crate) filter: String,
    pub(crate) sort: SortField,
    pub(crate) desc: bool,
    pub(crate) tracked: TrackedFilter,
    pub(crate) selection: MediaSelection,
}

impl Component for MediaList {
    type Message = Msg;
    type Properties = Props;

    fn create(ctx: &Context<Self>) -> Self {
        let (ws, _) = ctx
            .link()
            .context::<ws::Handle>(Callback::noop())
            .expect("Expected ws::Handle in context");

        let _setup = SetupChannel::new(ws.clone(), ctx.link().callback(Msg::Channel));
        let _broadcast = ws.on_broadcast(ctx.link().callback(Msg::AppBroadcast));

        let (time, _time_handle) = ctx
            .link()
            .context::<TimeInfo>(ctx.link().callback(Msg::SetTime))
            .expect("Expected a configured time zone");

        let (background, _) = ctx
            .link()
            .context::<Background>(Callback::noop())
            .expect("Expected background handle in context");

        let (router, _) = ctx
            .link()
            .context::<Router>(Callback::noop())
            .expect("Expected router in context");

        Self {
            channel: ws::Channel::default(),
            items: Vec::new(),
            filter: ctx.props().filter.clone(),
            page: ctx.props().page,
            sort: ctx.props().sort,
            desc: ctx.props().desc,
            tracked: ctx.props().tracked,
            selection: ctx.props().selection,
            time,
            _time_handle,
            background,
            router,
            _setup,
            _broadcast,
            list_req: ws::Request::default(),
            _mark_req: ws::Request::default(),
            _track_req: ws::Request::default(),
            applied_backdrop: None,
            _preload_img: None,
            _preload_load: None,
        }
    }

    fn update(&mut self, ctx: &Context<Self>, msg: Self::Message) -> bool {
        match self.try_update(ctx, msg) {
            Ok(render) => render,
            Err(e) => {
                self.background.error(e);
                false
            }
        }
    }

    fn changed(&mut self, ctx: &Context<Self>, _old_props: &Self::Properties) -> bool {
        let props = ctx.props();

        self.page = props.page;
        self.filter = props.filter.clone();
        self.sort = props.sort;
        self.desc = props.desc;
        self.tracked = props.tracked;
        self.selection = props.selection;

        true
    }

    fn rendered(&mut self, _ctx: &Context<Self>, first_render: bool) {
        if first_render {
            self.background.title(Some("Media".to_string()));
        }

        // Drive the page background from the first backdrop on the current page.
        // Track the requested URL so we only react to changes (and never re-emit
        // the same value, since `SetBackground` always triggers a re-render).
        if let Some(url) = self.current_backdrop()
            && self.applied_backdrop.as_ref() != Some(&url)
        {
            self.applied_backdrop = Some(url.clone());
            self.preload_background(url);
        }
    }

    fn destroy(&mut self, _ctx: &Context<Self>) {
        self.background.title(None);
    }

    fn view(&self, ctx: &Context<Self>) -> Html {
        let link = ctx.link();

        let filtered = self.filtered_sorted();

        let total = filtered.len();
        let total_pages = total.div_ceil(PAGE_SIZE).max(1);
        let page = self.page.min(total_pages - 1);

        let items = filtered.into_iter().skip(page * PAGE_SIZE).take(PAGE_SIZE);

        let on_filter = link.callback(|e: InputEvent| {
            let input: web_sys::HtmlInputElement = e.target_unchecked_into();
            Msg::Filter(input.value())
        });

        let on_sort = link.callback(|e: Event| {
            let select: web_sys::HtmlSelectElement = e.target_unchecked_into();
            Msg::SetSort(match select.value().as_str() {
                "release" => SortField::Release,
                "watched" => SortField::Watched,
                _ => SortField::Title,
            })
        });

        let sort_value = match self.sort {
            SortField::Title => "title",
            SortField::Release => "release",
            SortField::Watched => "watched",
        };

        let dir_icon = if self.desc {
            "bars-arrow-down"
        } else {
            "bars-arrow-up"
        };
        let dir_title = if self.desc { "Descending" } else { "Ascending" };

        let (tracked_icon, tracked_label) = match self.tracked {
            TrackedFilter::All => ("funnel", "All"),
            TrackedFilter::Tracked => ("eye", "Tracked"),
            TrackedFilter::Untracked => ("eye-slash", "Untracked"),
        };

        html! {
            <>
                <div class="row-split">
                    <h1>{"Media"}</h1>
                    <h4 class="text-muted">{total}</h4>
                </div>

                <input-controls>
                    <div class="input-group">
                        <input type="text" placeholder="Filter" value={self.filter.clone()} oninput={on_filter} class="input-text fill" />

                        if !self.filter.is_empty() {
                            <Button icon="backspace" title="Clear filter" onclick={link.callback(|_| Msg::Filter(String::new()))} />
                        }
                    </div>

                    <controls>
                        <div class="input-group fill">
                            <div class="input-label has-text">
                                {"Sort by:"}
                            </div>

                            <select class="input-select fill" onchange={on_sort} value={sort_value}>
                                <option value="title" selected={matches!(self.sort, SortField::Title)}>
                                    {"Title"}
                                </option>
                                <option value="release" selected={matches!(self.sort, SortField::Release)}>
                                    {"Release date"}
                                </option>
                                <option value="watched" selected={matches!(self.sort, SortField::Watched)}>
                                    {"Last watched"}
                                </option>
                            </select>

                            <Button icon={dir_icon} title={dir_title} onclick={link.callback(|_| Msg::ToggleDir)} />
                        </div>

                        <div class="input-group">
                            <Button icon={tracked_icon} title={format!("Showing: {tracked_label}")} text={tracked_label} onclick={link.callback(|_| Msg::CycleTracked)} />

                            <MediaKindToggle
                                selection={self.selection}
                                on_change={link.callback(Msg::SetSelection)}
                            />
                        </div>

                        <PaginationButtons {page} {total_pages} on_page={link.callback(Msg::SetPage)} />
                    </controls>
                </input-controls>

                if self.list_req.is_pending() {
                    <div class="row center">
                        <span class="item-inline-more"><span class="icon arrow-path spin" /></span>
                    </div>
                } else if items.len() == 0 {
                    <div class="row center">
                        <span class="item-inline-more">{"Nothing to show."}</span>
                    </div>
                } else {
                    <div class="table">
                        { for items.into_iter().map(|m| self.view_row(ctx, m)) }
                    </div>

                    <div class="row desktop-align-end">
                        <PaginationButtons {page} {total_pages} on_page={link.callback(Msg::SetPage)} />
                    </div>
                }
            </>
        }
    }
}

impl MediaList {
    fn try_update(&mut self, ctx: &Context<Self>, msg: Msg) -> Result<bool, Error> {
        match msg {
            Msg::Channel(result) => {
                self.channel = result?;

                if self.channel.id() != ws::ChannelId::NONE {
                    self.load(ctx);
                } else {
                    self.items.clear();
                }

                Ok(true)
            }
            Msg::AppBroadcast(packet) => {
                let event = packet?.decode_event()?;

                if event.channel == self.channel.id() {
                    return Ok(false);
                }

                let relevant = matches!(
                    event.kind,
                    api::AppEventKind::MovieCreated { .. }
                        | api::AppEventKind::MovieChanged { .. }
                        | api::AppEventKind::MovieDeleted { .. }
                        | api::AppEventKind::ShowCreated { .. }
                        | api::AppEventKind::ShowChanged { .. }
                        | api::AppEventKind::ShowDeleted { .. }
                        | api::AppEventKind::WatchedChanged { .. }
                );

                if relevant && self.channel.id() != ws::ChannelId::NONE {
                    self.load(ctx);
                }

                Ok(false)
            }
            Msg::Loaded(result) => {
                self.items = result
                    .context(Message::LoadingMovies)?
                    .decode()
                    .context(Message::LoadingMovies)?
                    .items;
                Ok(true)
            }
            Msg::MarkWatched(id, mark_time) => {
                self._mark_req = self
                    .channel
                    .request()
                    .body(api::MarkWatchedRequest {
                        kind: api::WatchedKind::Movie {
                            movie: api::MovieId::new(id),
                        },
                        mark_time,
                    })
                    .on_packet(ctx.link().callback(Msg::MarkWatchedDone))
                    .send();
                Ok(true)
            }
            Msg::MarkWatchedDone(result) => {
                result.context(Message::MarkingWatched)?;

                if self.channel.id() != ws::ChannelId::NONE {
                    self.load(ctx);
                }

                Ok(false)
            }
            Msg::SetTracked(kind, id, tracked) => {
                self._track_req = match kind {
                    api::MediaKind::Shows => self
                        .channel
                        .request()
                        .body(api::UntrackShowRequest {
                            id: api::ShowId::new(id),
                            tracked,
                        })
                        .on_packet(ctx.link().callback(
                            |r: Result<ws::Packet<api::UntrackShow>, ws::Error>| {
                                Msg::SetTrackedDone(r.map(drop))
                            },
                        ))
                        .send(),
                    api::MediaKind::Movies => self
                        .channel
                        .request()
                        .body(api::UntrackMovieRequest {
                            id: api::MovieId::new(id),
                            tracked,
                        })
                        .on_packet(ctx.link().callback(
                            |r: Result<ws::Packet<api::UntrackMovie>, ws::Error>| {
                                Msg::SetTrackedDone(r.map(drop))
                            },
                        ))
                        .send(),
                };

                Ok(false)
            }
            Msg::SetTrackedDone(result) => {
                result.context(Message::TrackingShow)?;

                if self.channel.id() != ws::ChannelId::NONE {
                    self.load(ctx);
                }

                Ok(false)
            }
            Msg::Filter(s) => {
                self.filter = s;
                self.page = 0;
                self.emit_navigate();
                Ok(true)
            }
            Msg::SetSort(sort) => {
                self.sort = sort;
                self.page = 0;
                self.emit_navigate();
                Ok(true)
            }
            Msg::ToggleDir => {
                self.desc = !self.desc;
                self.page = 0;
                self.emit_navigate();
                Ok(true)
            }
            Msg::CycleTracked => {
                self.tracked = self.tracked.next();
                self.page = 0;
                self.emit_navigate();
                Ok(true)
            }
            Msg::SetSelection(selection) => {
                self.selection = selection;
                self.page = 0;
                self.emit_navigate();
                Ok(true)
            }
            Msg::SetPage(p) => {
                self.page = p;
                self.emit_navigate();
                Ok(true)
            }
            Msg::Navigate(route) => {
                self.router.push(route);
                Ok(false)
            }
            Msg::SetTime(time) => {
                self.time = time;
                Ok(true)
            }
        }
    }

    /// Items matching the current filter/selection, ordered by the active sort.
    fn filtered_sorted(&self) -> Vec<&api::MediaItem> {
        let filter = self.filter.to_lowercase();

        let mut filtered: Vec<&api::MediaItem> = self
            .items
            .iter()
            .filter(|m| self.selection.contains(m.kind))
            .filter(|m| match self.tracked {
                TrackedFilter::All => true,
                TrackedFilter::Tracked => m.tracked,
                TrackedFilter::Untracked => !m.tracked,
            })
            .filter(|m| {
                filter.is_empty()
                    || m.strings
                        .texts(api::StringKind::Title)
                        .any(|t| t.to_lowercase().contains(&filter))
            })
            .collect();

        match self.sort {
            SortField::Title => filtered.sort_by_key(|m| m.strings.title().map(str::to_lowercase)),
            SortField::Release => filtered.sort_by_key(|m| m.date),
            SortField::Watched => filtered.sort_by_key(|m| m.last_watched_at),
        }

        if self.desc {
            filtered.reverse();
        }

        filtered
    }

    /// First backdrop set on the current page, used as the page background.
    fn current_backdrop(&self) -> Option<String> {
        let filtered = self.filtered_sorted();
        let total_pages = filtered.len().div_ceil(PAGE_SIZE).max(1);
        let page = self.page.min(total_pages - 1);

        filtered
            .into_iter()
            .skip(page * PAGE_SIZE)
            .take(PAGE_SIZE)
            .find_map(|m| m.backdrop.as_ref().map(|i| i.proxy_url()))
    }

    /// Preload `url` into an off-screen image, only switching the page
    /// background to it once the browser has the image ready. This avoids a
    /// flash of a half-loaded backdrop and lets the CSS cross-fade run smoothly.
    fn preload_background(&mut self, url: String) {
        let Ok(img) = HtmlImageElement::new() else {
            // Fall back to switching immediately if we can't preload.
            self.background.background(Some(url));
            return;
        };

        let background = self.background.clone();
        let load = EventListener::once(&img, "load", {
            let url = url.clone();
            move |_| background.background(Some(url))
        });

        img.set_src(&url);

        self._preload_img = Some(img);
        self._preload_load = Some(load);
    }

    fn emit_navigate(&self) {
        self.router.push(Route::Media(MediaQuery {
            page: self.page,
            filter: self.filter.clone(),
            sort: self.sort,
            desc: self.desc,
            tracked: self.tracked,
            selection: self.selection,
        }));
    }

    fn load(&mut self, ctx: &Context<Self>) {
        self.list_req = self
            .channel
            .request()
            .body(api::ListMediaRequest)
            .on_packet(ctx.link().callback(Msg::Loaded))
            .send();
    }

    fn view_row(&self, ctx: &Context<Self>, m: &api::MediaItem) -> Html {
        let id = m.id;
        let kind = m.kind;
        let onclick = ctx
            .link()
            .callback(move |_| Msg::Navigate(detail_route(kind, id)));

        let is_movie = matches!(m.kind, api::MediaKind::Movies);

        let kind_icon = match m.kind {
            api::MediaKind::Shows => "tv",
            api::MediaKind::Movies => "film",
        };

        let kind_title = match m.kind {
            api::MediaKind::Shows => "Show",
            api::MediaKind::Movies => "Movie",
        };

        // When the filter matched an alternate-language title rather than the
        // primary one, surface that alt title so it's clear why the row matched.
        let primary_title = m.strings.title();
        let matched_alt = {
            let filter = self.filter.to_lowercase();
            let primary_matches = primary_title.is_some_and(|t| t.to_lowercase().contains(&filter));

            (!filter.is_empty() && !primary_matches)
                .then(|| {
                    m.strings
                        .texts(api::StringKind::Title)
                        .find(|t| t.to_lowercase().contains(&filter) && Some(*t) != primary_title)
                })
                .flatten()
        };

        let preset = m
            .date
            .map(|timestamp| TimePreset::at("clock", "Released", timestamp));

        html! {
            <div class="desktop-row mobile-column align-top">
                <Image class="banner clickable mobile-only" onclick={&onclick} src={m.banner.clone()} />
                <Image class="poster poster-side clickable desktop-only" onclick={&onclick} src={m.poster.clone()} />

                <div class="column fill">
                    <div class="row-split fill align-top">
                        <div class="column fill">
                            <div class="row clickable" onclick={&onclick}>
                                <div class="item-inline" title={kind_title}>
                                    <div class={classes!("icon", kind_icon)} />
                                </div>

                                <span class="item-title">{primary_title.unwrap_or("Untitled Media")}</span>
                            </div>

                            if let Some(alt) = matched_alt {
                                <div class="row text-gap text-muted">
                                    <span>{"Alt:"}</span>
                                    <span>{alt}</span>
                                </div>
                            }

                            if !m.remotes.is_empty() {
                                <div class="row">
                                    { for m.remotes.iter().filter_map(|r| {
                                        let url = match m.kind {
                                            api::MediaKind::Shows => r.remote.show_url(r.slug.as_deref()),
                                            api::MediaKind::Movies => r.remote.movie_url(),
                                        }?;
                                        let label = r.remote.source().as_label();
                                        let id = r.remote.source().as_id();

                                        Some(html! {
                                            <a class="item-inline-source" href={url} target="_blank" rel="noopener noreferrer" title={format!("Open on {label}")}>
                                                <span class={classes!("logo", id)} />
                                            </a>
                                        })
                                    }) }
                                </div>
                            }

                            <div class="row">
                                if let Some(ts) = m.date {
                                    <div class="row">
                                        <span class="text-muted item-inline" title="Release date">
                                            <span class="icon calendar" />
                                        </span>

                                        <span>{ts.human_date(self.time.clone())}</span>
                                    </div>
                                } else {
                                    <div class="row">
                                        <span class="text-muted item-inline" title="Unknown release date">
                                            <span class="icon calendar" />
                                        </span>

                                        {"No release date"}
                                    </div>
                                }

                                if let Some(ts) = m.last_watched_at {
                                    <div class="row">
                                        <span class="text-muted item-inline" title="Last watched">
                                            <span class="icon eye" />
                                        </span>

                                        <span>{ts.human_date(self.time.clone())}</span>
                                    </div>
                                } else {
                                    <div class="row">
                                        <span class="text-muted item-inline" title="Not watched">
                                            <span class="icon eye-slash" />
                                        </span>

                                        {"Never watched"}
                                    </div>
                                }
                            </div>
                        </div>

                        <div class="row">
                            <div class="row">
                                if is_movie {
                                    <MarkTimeMenu class="success" title="Mark watched" prompt={match primary_title { Some(title) => format!("When did you watch {}?", title), None => "When did you watch this movie?".to_string() }} {preset} on_confirm={ctx.link().callback(move |mark_time| Msg::MarkWatched(id, mark_time))}>
                                        <span class="icon check" />
                                    </MarkTimeMenu>
                                }
                            </div>

                            if !m.tracked {
                                <button class="desktop-has-text" title="Track"
                                    onclick={ctx.link().callback(move |_| Msg::SetTracked(kind, id, true))}>
                                    <span class="icon eye-slash" />
                                    <span class="desktop-only">{"Track"}</span>
                                </button>
                            }
                        </div>
                    </div>

                    if let Some(overview) = m.strings.overview() {
                        <div class="overview">
                            {overview}
                        </div>
                    }
                </div>

                <span class="item-inline align-end clickable"><span onclick={&onclick} class="icon chevron-right" /></span>
            </div>
        }
    }
}
