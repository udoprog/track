use api::TimeZone;
use musli_web::web03::prelude::*;
use yew::prelude::*;

use crate::background::Background;
use crate::error::{CustomContext, Error, Message};
use crate::router::{MediaQuery, MediaSelection, Route, ShowDetailQuery, SortField, TrackedFilter};
use crate::ui::{Loading, MarkWatchedPicker, PaginationButtons};
use crate::{Image, SetupChannel};

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

pub(super) struct MediaList {
    channel: ws::Channel,
    items: Vec<api::MediaItem>,
    filter: String,
    page: usize,
    sort: SortField,
    desc: bool,
    tracked: TrackedFilter,
    selection: MediaSelection,
    tz: TimeZone,
    background: Background,
    _tz_handle: ContextHandle<TimeZone>,
    _setup: SetupChannel,
    _broadcast: ws::Listener,
    list_req: ws::Request,
    _mark_req: ws::Request,
    _track_req: ws::Request,
    /// Movie id currently awaiting watch confirmation (movies only).
    confirming_watch: Option<u64>,
    /// Backdrop URL last pushed as the page background, to avoid re-emitting.
    applied_backdrop: Option<String>,
}

pub(super) enum Msg {
    Channel(Result<ws::Channel, ws::Error>),
    AppBroadcast(Result<ws::Packet<api::AppBroadcast>, ws::Error>),
    Loaded(Result<ws::Packet<api::ListMedia>, ws::Error>),
    AskMarkWatched(u64),
    CancelMarkWatch,
    MarkWatched(u64, api::MarkTime),
    MarkWatchedDone(Result<ws::Packet<api::MarkWatched>, ws::Error>),
    SetTracked(api::MediaKind, u64, bool),
    SetTrackedDone(Result<(), ws::Error>),
    Filter(String),
    SetSort(SortField),
    ToggleDir,
    CycleTracked,
    ToggleKind(api::MediaKind),
    SetPage(usize),
    Navigate(Route),
    SetTz(TimeZone),
}

#[derive(Properties, PartialEq)]
pub(super) struct Props {
    pub(super) onerror: Callback<Option<Error>>,
    pub(super) page: usize,
    pub(super) filter: String,
    pub(super) sort: SortField,
    pub(super) desc: bool,
    pub(super) tracked: TrackedFilter,
    pub(super) selection: MediaSelection,
    pub(super) on_navigate: Callback<Route>,
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

        let (tz, _tz_handle) = ctx
            .link()
            .context::<TimeZone>(ctx.link().callback(Msg::SetTz))
            .expect("Expected a configured time zone");

        let (background, _) = ctx
            .link()
            .context::<Background>(Callback::noop())
            .expect("Expected background handle in context");

        Self {
            channel: ws::Channel::default(),
            items: Vec::new(),
            filter: ctx.props().filter.clone(),
            page: ctx.props().page,
            sort: ctx.props().sort,
            desc: ctx.props().desc,
            tracked: ctx.props().tracked,
            selection: ctx.props().selection,
            tz,
            background,
            _tz_handle,
            _setup,
            _broadcast,
            list_req: ws::Request::default(),
            _mark_req: ws::Request::default(),
            _track_req: ws::Request::default(),
            confirming_watch: None,
            applied_backdrop: None,
        }
    }

    fn update(&mut self, ctx: &Context<Self>, msg: Self::Message) -> bool {
        match self.try_update(ctx, msg) {
            Ok(render) => render,
            Err(e) => {
                ctx.props().onerror.emit(Some(e));
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
        // Only emit on change, since `SetBackground` always triggers a re-render.
        if let Some(url) = self.current_backdrop()
            && self.applied_backdrop.as_ref() != Some(&url)
        {
            self.applied_backdrop = Some(url.clone());
            self.background.background(Some(url));
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
                <div class="row-fill">
                    <h1>{"Media"}</h1>
                    <h4 class="text-muted end">{total}</h4>
                </div>

                <div class="input-controls">
                    <div class="input-group">
                        <input
                            type="text"
                            placeholder="Filter"
                            value={self.filter.clone()}
                            oninput={on_filter}
                            class="input-text fill"
                        />

                        if !self.filter.is_empty() {
                            <button class="btn" title="Clear filter"
                                onclick={link.callback(|_| Msg::Filter(String::new()))}>
                                <span class="icon backspace" />
                            </button>
                        }
                    </div>

                    <div class="row">
                        <div class="input-group">
                            <div class="input-text">
                                {"Sort by:"}
                            </div>

                            <select class="input-select" onchange={on_sort} value={sort_value}>
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

                            <button class="btn" title={dir_title}
                                onclick={link.callback(|_| Msg::ToggleDir)}>
                                <span class={classes!("icon", dir_icon)} />
                            </button>
                        </div>

                        <div class="input-group">
                            <button class="btn" title={format!("Showing: {tracked_label}")}
                                onclick={link.callback(|_| Msg::CycleTracked)}>
                                <span class={classes!("icon", tracked_icon)} />
                                <span class="hide-mobile">{tracked_label}</span>
                            </button>

                            <span
                                class={classes!("input-checkbox", self.selection.shows.then_some("checked"))}
                                title="Show series"
                                onclick={link.callback(|_| Msg::ToggleKind(api::MediaKind::Shows))}>
                                <span class="icon tv" />
                                <span class="mark" />
                            </span>

                            <span
                                class={classes!("input-checkbox", self.selection.movies.then_some("checked"))}
                                title="Show movies"
                                onclick={link.callback(|_| Msg::ToggleKind(api::MediaKind::Movies))}>
                                <span class="icon film" />
                                <span class="mark" />
                            </span>
                        </div>

                        <div class="input-group">
                            <PaginationButtons {page} {total_pages} on_page={link.callback(Msg::SetPage)} />
                        </div>
                    </div>
                </div>

                if self.list_req.is_pending() {
                    <Loading />
                } else if items.len() == 0 {
                    <div class="text-muted">{"Nothing to show."}</div>
                } else {
                    <div class="table">
                        { for items.into_iter().map(|m| self.view_row(ctx, m)) }
                    </div>

                    <div class="row center">
                        <div class="input-group">
                            <PaginationButtons {page} {total_pages} on_page={link.callback(Msg::SetPage)} />
                        </div>
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
            Msg::AskMarkWatched(id) => {
                self.confirming_watch = Some(id);
                Ok(true)
            }
            Msg::CancelMarkWatch => {
                self.confirming_watch = None;
                Ok(true)
            }
            Msg::MarkWatched(id, mark_time) => {
                self.confirming_watch = None;
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
                self.emit_navigate(ctx);
                Ok(true)
            }
            Msg::SetSort(sort) => {
                self.sort = sort;
                self.page = 0;
                self.emit_navigate(ctx);
                Ok(true)
            }
            Msg::ToggleDir => {
                self.desc = !self.desc;
                self.page = 0;
                self.emit_navigate(ctx);
                Ok(true)
            }
            Msg::CycleTracked => {
                self.tracked = self.tracked.next();
                self.page = 0;
                self.emit_navigate(ctx);
                Ok(true)
            }
            Msg::ToggleKind(kind) => {
                match kind {
                    api::MediaKind::Shows => self.selection.shows = !self.selection.shows,
                    api::MediaKind::Movies => self.selection.movies = !self.selection.movies,
                }
                self.page = 0;
                self.emit_navigate(ctx);
                Ok(true)
            }
            Msg::SetPage(p) => {
                self.page = p;
                self.emit_navigate(ctx);
                Ok(true)
            }
            Msg::Navigate(route) => {
                ctx.props().on_navigate.emit(route);
                Ok(false)
            }
            Msg::SetTz(tz) => {
                self.tz = tz;
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
                    || m.title
                        .as_ref()
                        .is_some_and(|t| t.to_lowercase().contains(&filter))
            })
            .collect();

        match self.sort {
            SortField::Title => filtered.sort_by_key(|m| m.title.as_deref().map(str::to_lowercase)),
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

    fn emit_navigate(&self, ctx: &Context<Self>) {
        ctx.props().on_navigate.emit(Route::Media(MediaQuery {
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

        html! {
            <div class="table-entry">
                <div class="desktop-row mobile-column">
                    <Image class="banner clickable hide-desktop" onclick={&onclick} src={m.banner.clone()} />
                    <Image class="poster poster-side clickable hide-mobile top" onclick={&onclick} src={m.poster.clone()} />

                    <div class="column fill top">
                        if is_movie && self.confirming_watch == Some(id) {
                            <MarkWatchedPicker
                                aired_label="Released"
                                on_confirm={ctx.link().callback(move |mark_time| Msg::MarkWatched(id, mark_time))}
                                on_cancel={ctx.link().callback(|_| Msg::CancelMarkWatch)}
                            />
                        } else {
                            <div class="row-fill fill">
                                <div class="column fill">
                                    <div class="row clickable" onclick={&onclick}>
                                        <div class="item-inline" title={kind_title}>
                                            <div class={classes!("icon", kind_icon)} />
                                        </div>

                                        <span class="item-title">{m.title.as_deref().unwrap_or("Untitled Media")}</span>
                                    </div>

                                    if !m.remotes.is_empty() {
                                        <div class="row">
                                            { for m.remotes.iter().filter_map(|r| {
                                                let url = match m.kind {
                                                    api::MediaKind::Shows => r.remote.show_url(r.slug.as_deref()),
                                                    api::MediaKind::Movies => r.remote.movie_url(),
                                                }?;
                                                let label = r.remote.source().as_str();

                                                Some(html! {
                                                    <a class="item-inline-source" href={url} target="_blank" rel="noopener noreferrer" title={format!("Open on {label}")}>
                                                        <span class={classes!("logo", label.to_owned())} />
                                                    </a>
                                                })
                                            }) }
                                        </div>
                                    }

                                    <div class="row">
                                        if let Some(date) = m.date {
                                            <div class="row">
                                                <span class="text-muted item-inline" title="Release date">
                                                    <span class="icon calendar" />
                                                </span>

                                                {date.date(self.tz.clone()).to_string()}
                                            </div>
                                        } else {
                                            <div class="row">
                                                <span class="text-muted item-inline" title="Unknown release date">
                                                    <span class="icon calendar" />
                                                </span>

                                                {"No release date"}
                                            </div>
                                        }

                                        if let Some(watched) = m.last_watched_at {
                                            <div class="row">
                                                <span class="text-muted item-inline" title="Last watched">
                                                    <span class="icon eye" />
                                                </span>
                                                {watched.date(self.tz.clone()).to_string()}
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

                                <div class="row end top">
                                    <div class="row">
                                        if is_movie {
                                            <button class="btn-success" title="Mark watched" onclick={ctx.link().callback(move |_| Msg::AskMarkWatched(id))}>
                                                <span class="icon check" />
                                            </button>
                                        }
                                    </div>

                                    if !m.tracked {
                                        <button class="btn" title="Track"
                                            onclick={ctx.link().callback(move |_| Msg::SetTracked(kind, id, true))}>
                                            <span class="icon eye-slash" />
                                            <span class="hide-mobile">{"Track"}</span>
                                        </button>
                                    }
                                </div>
                            </div>
                        }

                        if let Some(ref overview) = m.overview {
                            <div class="overview">
                                {overview}
                            </div>
                        }
                    </div>

                    <span class="item-inline align-end clickable"><span onclick={&onclick} class="icon chevron-right" /></span>
                </div>
            </div>
        }
    }
}
