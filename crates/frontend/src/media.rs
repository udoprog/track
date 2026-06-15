use api::TimeZone;
use musli_web::web03::prelude::*;
use yew::prelude::*;

use crate::background::Background;
use crate::error::{CustomContext, Error, Message};
use crate::router::{PagedQuery, Route, ShowDetailQuery, SortField};
use crate::ui::{Loading, MarkWatchedPicker, PaginationButtons};
use crate::{Image, SetupChannel};

const PAGE_SIZE: usize = 20;

fn kind_title(kind: api::MediaKind) -> &'static str {
    match kind {
        api::MediaKind::Shows => "Shows",
        api::MediaKind::Movies => "Movies",
    }
}

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
    kind: api::MediaKind,
    items: Vec<api::MediaItem>,
    filter: String,
    page: usize,
    sort: SortField,
    desc: bool,
    tz: TimeZone,
    background: Background,
    _tz_handle: ContextHandle<TimeZone>,
    _setup: SetupChannel,
    _broadcast: ws::Listener,
    list_req: ws::Request,
    _mark_req: ws::Request,
    /// Movie id currently awaiting watch confirmation (movies only).
    confirming_watch: Option<u64>,
}

pub(super) enum Msg {
    Channel(Result<ws::Channel, ws::Error>),
    AppBroadcast(Result<ws::Packet<api::AppBroadcast>, ws::Error>),
    Loaded(Result<ws::Packet<api::ListMedia>, ws::Error>),
    AskMarkWatched(u64),
    CancelMarkWatch,
    MarkWatched(u64, api::MarkTime),
    MarkWatchedDone(Result<ws::Packet<api::MarkWatched>, ws::Error>),
    Filter(String),
    SetSort(SortField),
    ToggleDir,
    SetPage(usize),
    Navigate(Route),
    SetTz(TimeZone),
}

#[derive(Properties, PartialEq)]
pub(super) struct Props {
    pub(super) kind: api::MediaKind,
    pub(super) onerror: Callback<Option<Error>>,
    pub(super) page: usize,
    pub(super) filter: String,
    pub(super) sort: SortField,
    pub(super) desc: bool,
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
            kind: ctx.props().kind,
            items: Vec::new(),
            filter: ctx.props().filter.clone(),
            page: ctx.props().page,
            sort: ctx.props().sort,
            desc: ctx.props().desc,
            tz,
            background,
            _tz_handle,
            _setup,
            _broadcast,
            list_req: ws::Request::default(),
            _mark_req: ws::Request::default(),
            confirming_watch: None,
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

    fn changed(&mut self, ctx: &Context<Self>, old_props: &Self::Properties) -> bool {
        let props = ctx.props();
        let kind_changed = props.kind != old_props.kind;

        self.kind = props.kind;
        self.page = props.page;
        self.filter = props.filter.clone();
        self.sort = props.sort;
        self.desc = props.desc;

        if kind_changed {
            self.confirming_watch = None;
            self.background
                .title(Some(kind_title(props.kind).to_string()));

            if self.channel.id() != ws::ChannelId::NONE {
                self.load(ctx);
            }
        }

        true
    }

    fn rendered(&mut self, ctx: &Context<Self>, first_render: bool) {
        if first_render {
            self.background
                .title(Some(kind_title(ctx.props().kind).to_string()));
        }
    }

    fn destroy(&mut self, _ctx: &Context<Self>) {
        self.background.title(None);
    }

    fn view(&self, ctx: &Context<Self>) -> Html {
        let link = ctx.link();

        let filter = self.filter.to_lowercase();

        let mut filtered: Vec<&api::MediaItem> = self
            .items
            .iter()
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

        let dir_icon = if self.desc { "arrow-down" } else { "arrow-up" };
        let dir_title = if self.desc { "Descending" } else { "Ascending" };

        html! {
            <>
                <div class="row-fill">
                    <h1>{kind_title(self.kind)}</h1>
                    <h4 class="text-muted end">{total}</h4>
                </div>

                <div class="row">
                    <div class="input-group fill">
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

                        <PaginationButtons {page} {total_pages} on_page={link.callback(Msg::SetPage)} />
                    </div>
                </div>

                if self.list_req.is_pending() {
                    <Loading />
                } else if items.len() == 0 {
                    <div class="text-muted">{"Nothing tracked."}</div>
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

                let relevant = match self.kind {
                    api::MediaKind::Movies => matches!(
                        event.kind,
                        api::AppEventKind::MovieCreated { .. }
                            | api::AppEventKind::MovieChanged { .. }
                            | api::AppEventKind::MovieDeleted { .. }
                            | api::AppEventKind::WatchedChanged { .. }
                    ),
                    api::MediaKind::Shows => matches!(
                        event.kind,
                        api::AppEventKind::ShowCreated { .. }
                            | api::AppEventKind::ShowChanged { .. }
                            | api::AppEventKind::ShowDeleted { .. }
                            | api::AppEventKind::WatchedChanged { .. }
                    ),
                };

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

    fn emit_navigate(&self, ctx: &Context<Self>) {
        ctx.props().on_navigate.emit(Route::media(
            self.kind,
            PagedQuery {
                page: self.page,
                filter: self.filter.clone(),
                sort: self.sort,
                desc: self.desc,
            },
        ));
    }

    fn load(&mut self, ctx: &Context<Self>) {
        self.list_req = self
            .channel
            .request()
            .body(api::ListMediaRequest { kind: self.kind })
            .on_packet(ctx.link().callback(Msg::Loaded))
            .send();
    }

    fn view_row(&self, ctx: &Context<Self>, m: &api::MediaItem) -> Html {
        let id = m.id;
        let kind = self.kind;
        let onclick = ctx
            .link()
            .callback(move |_| Msg::Navigate(detail_route(kind, id)));

        let is_movie = matches!(self.kind, api::MediaKind::Movies);

        html! {
            <div class="table-entry">
                <div class="desktop-row mobile-column">
                    <Image class="banner clickable hide-desktop" onclick={&onclick} src={m.banner.clone()} />
                    <Image class="poster poster-side clickable hide-mobile" onclick={&onclick} src={m.poster.clone()} />

                    <div class="column fill top">
                        if self.confirming_watch == Some(id) {
                            <MarkWatchedPicker
                                aired_label="Released"
                                on_confirm={ctx.link().callback(move |mark_time| Msg::MarkWatched(id, mark_time))}
                                on_cancel={ctx.link().callback(|_| Msg::CancelMarkWatch)}
                            />
                        } else {
                            <div class="row-fill fill">
                                <div class="column fill">
                                    if let Some(ref title) = m.title {
                                        <span class="item-title clickable" onclick={&onclick}>{title}</span>
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
                                    if is_movie {
                                        <button class="btn-success" title="Mark watched" onclick={ctx.link().callback(move |_| Msg::AskMarkWatched(id))}>
                                            <span class="icon check" />
                                        </button>
                                    }

                                    if !m.tracked {
                                        <span class="end item-inline" title="Untracked">
                                            <span class="icon eye-slash" />
                                        </span>
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
