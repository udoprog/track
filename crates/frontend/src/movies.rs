use api::TimeZone;
use musli_web::web03::prelude::*;
use yew::prelude::*;

use crate::SetupChannel;
use crate::error::{CustomContext, Error, Message};
use crate::router::{PagedQuery, Route};
use crate::ui::MarkWatchedPicker;

const PAGE_SIZE: usize = 20;

pub(super) struct MoviesList {
    channel: ws::Channel,
    movies: Vec<api::Movie>,
    filter: String,
    page: usize,
    tz: TimeZone,
    _tz_handle: ContextHandle<TimeZone>,
    _setup: SetupChannel,
    _broadcast: ws::Listener,
    _list_req: ws::Request,
    _mark_req: ws::Request,
    confirming_watch: Option<api::MovieId>,
}

pub(super) enum Msg {
    Channel(Result<ws::Channel, ws::Error>),
    AppBroadcast(Result<ws::Packet<api::AppBroadcast>, ws::Error>),
    MoviesLoaded(Result<ws::Packet<api::ListMovies>, ws::Error>),
    AskMarkWatched(api::MovieId),
    CancelMarkWatch,
    MarkWatched(api::MovieId, api::MarkTime),
    MarkWatchedDone(Result<ws::Packet<api::MarkWatched>, ws::Error>),
    Filter(String),
    SetPage(usize),
    Navigate(Route),
    SetTz(TimeZone),
}

#[derive(Properties, PartialEq)]
pub(super) struct Props {
    pub(super) page: usize,
    pub(super) filter: String,
    pub(super) onerror: Callback<Error>,
    pub(super) on_navigate: Callback<Route>,
}

impl Component for MoviesList {
    type Message = Msg;
    type Properties = Props;

    fn create(ctx: &Context<Self>) -> Self {
        let (ws, _) = ctx
            .link()
            .context::<ws::Handle>(Callback::noop())
            .expect("ws::Handle context not found");

        let _setup = SetupChannel::new(ws.clone(), ctx.link().callback(Msg::Channel));
        let _broadcast = ws.on_broadcast(ctx.link().callback(Msg::AppBroadcast));

        let (tz, _tz_handle) = ctx
            .link()
            .context::<TimeZone>(ctx.link().callback(Msg::SetTz))
            .expect("time zone not found");

        Self {
            channel: ws::Channel::default(),
            movies: Vec::new(),
            page: ctx.props().page,
            filter: ctx.props().filter.clone(),
            tz,
            _tz_handle,
            _setup,
            _broadcast,
            _list_req: ws::Request::default(),
            _mark_req: ws::Request::default(),
            confirming_watch: None,
        }
    }

    fn update(&mut self, ctx: &Context<Self>, msg: Self::Message) -> bool {
        match self.try_update(ctx, msg) {
            Ok(render) => render,
            Err(e) => {
                ctx.props().onerror.emit(e);
                false
            }
        }
    }

    fn view(&self, ctx: &Context<Self>) -> Html {
        let link = ctx.link();

        let filter = self.filter.to_lowercase();

        let filtered: Vec<&api::Movie> = self
            .movies
            .iter()
            .filter(|m| {
                filter.is_empty()
                    || m.title
                        .as_ref()
                        .map_or(false, |t| t.to_lowercase().contains(&filter))
            })
            .collect();

        let total = filtered.len();
        let total_pages = total.div_ceil(PAGE_SIZE).max(1);
        let page = self.page.min(total_pages - 1);
        let prev_page = page.checked_sub(1);
        let next_page = (page + 1 < total_pages).then_some(page + 1);

        let page_items: Vec<&api::Movie> = filtered
            .into_iter()
            .skip(page * PAGE_SIZE)
            .take(PAGE_SIZE)
            .collect();

        let on_filter = link.callback(|e: InputEvent| {
            let input: web_sys::HtmlInputElement = e.target_unchecked_into();
            Msg::Filter(input.value())
        });

        html! {
            <div class="page">
                <div class="page-title row">
                    <span class="fill">{"Movies"}</span>
                    <span class="text-muted">{total}</span>
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
                            <button class="btn-icon" title="Clear filter"
                                onclick={link.callback(|_| Msg::Filter(String::new()))}>
                                <span class="icon backspace" />
                            </button>
                        }
                    </div>
                </div>
                if page_items.is_empty() {
                    <div class="empty text-muted">{"No movies tracked."}</div>
                } else {
                    <div class="table">
                    { for page_items.into_iter().map(|m| self.view_row(ctx, m)) }
                    </div>
                    if total_pages > 1 {
                        <div class="row center">
                            <button class="btn-icon" disabled={prev_page.is_none()}
                                onclick={link.callback(move |_| Msg::SetPage(prev_page.unwrap_or(0)))}>
                                <span class="icon arrow-left" />
                            </button>
                            <span class="text-muted">{format!("{} / {}", page + 1, total_pages)}</span>
                            <button class="btn-icon" disabled={next_page.is_none()}
                                onclick={link.callback(move |_| Msg::SetPage(next_page.unwrap_or(page)))}>
                                <span class="icon arrow-right" />
                            </button>
                        </div>
                    }
                }
            </div>
        }
    }
}

impl MoviesList {
    fn try_update(&mut self, ctx: &Context<Self>, msg: Msg) -> Result<bool, Error> {
        match msg {
            Msg::Channel(result) => {
                self.channel = result?;

                if self.channel.id() != ws::ChannelId::NONE {
                    self.load(ctx);
                } else {
                    self.movies.clear();
                }

                Ok(true)
            }
            Msg::AppBroadcast(packet) => {
                let event = packet?.decode_event()?;

                if event.channel == self.channel.id() {
                    return Ok(false);
                }

                match event.kind {
                    api::AppEventKind::MovieCreated { .. }
                    | api::AppEventKind::MovieChanged { .. }
                    | api::AppEventKind::MovieDeleted { .. }
                    | api::AppEventKind::WatchedChanged { .. } => {
                        self.load(ctx);
                        Ok(false)
                    }
                    _ => Ok(false),
                }
            }
            Msg::MoviesLoaded(result) => {
                self.movies = result
                    .context(Message::LoadingMovies)?
                    .decode()
                    .context(Message::LoadingMovies)?
                    .movies;
                Ok(true)
            }
            Msg::AskMarkWatched(movie_id) => {
                self.confirming_watch = Some(movie_id);
                Ok(true)
            }
            Msg::CancelMarkWatch => {
                self.confirming_watch = None;
                Ok(true)
            }
            Msg::MarkWatched(movie_id, mark_time) => {
                self.confirming_watch = None;
                self._mark_req = self
                    .channel
                    .request()
                    .body(api::MarkWatchedRequest {
                        kind: api::WatchedKind::Movie { movie: movie_id },
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

                ctx.props().on_navigate.emit(Route::Movies(PagedQuery {
                    page: 0,
                    filter: self.filter.clone(),
                }));

                Ok(true)
            }
            Msg::SetPage(p) => {
                self.page = p;

                ctx.props().on_navigate.emit(Route::Movies(PagedQuery {
                    page: self.page,
                    filter: self.filter.clone(),
                }));

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

    fn load(&mut self, ctx: &Context<Self>) {
        self._list_req = self
            .channel
            .request()
            .body(api::ListMoviesRequest)
            .on_packet(ctx.link().callback(Msg::MoviesLoaded))
            .send();
    }

    fn view_row(&self, ctx: &Context<Self>, m: &api::Movie) -> Html {
        let movie_id = m.id;
        let onclick = ctx
            .link()
            .callback(move |_| Msg::Navigate(Route::MovieDetail(movie_id)));

        html! {
            <div class="table-entry clickable" {onclick}>
                <div class="desktop-row mobile-column">
                    if let Some(poster) = m.poster.as_ref() {
                        <img class="poster-sm hide-mobile" src={poster.proxy_url()} />
                    } else {
                        <div class="poster-sm hide-mobile" />
                    }

                    if let Some(banner) = m.banner.as_ref() {
                        <img class="banner hide-desktop" src={banner.proxy_url()} />
                    } else {
                        <div class="banner hide-desktop" />
                    }

                    <div class="row-fill fill top">
                        <div class="column fill">
                            if let Some(ref title) = m.title {
                                <span class="item-title">{title}</span>
                            }

                            if let Some(ref overview) = m.overview {
                                <div class="overview">
                                    {overview}
                                </div>
                            }
                        </div>

                        if self.confirming_watch == Some(movie_id) {
                            <MarkWatchedPicker
                                on_confirm={ctx.link().callback(move |mark_time| Msg::MarkWatched(movie_id, mark_time))}
                                on_cancel={ctx.link().callback(|_| Msg::CancelMarkWatch)}
                            />
                        } else {
                            <div class="row end top">
                                if let Some(date) = m.release_date {
                                    <span class="text-muted">{date.date(self.tz.clone()).year()}</span>
                                }

                                <button class="btn-icon-success" title="Mark watched" onclick={ctx.link().callback(move |_| Msg::AskMarkWatched(movie_id))}>
                                    <span class="icon check" />
                                </button>

                                if !m.tracked {
                                    <span class="end icon-inline" title="Untracked movie">
                                        <span class="icon eye-slash" />
                                    </span>
                                }
                            </div>
                        }
                    </div>

                    <span class="icon-inline"><span class="icon chevron-right" /></span>
                </div>
            </div>
        }
    }
}
