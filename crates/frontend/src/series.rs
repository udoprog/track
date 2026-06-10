use api::TimeZone;
use musli_web::web03::prelude::*;
use yew::prelude::*;

use crate::SetupChannel;
use crate::error::{CustomContext, Error, Message};
use crate::router::{PagedQuery, Route, SeriesDetailQuery};

const PAGE_SIZE: usize = 20;

pub(super) struct SeriesList {
    channel: ws::Channel,
    series: Vec<api::Series>,
    page: usize,
    filter: String,
    tz: TimeZone,
    _tz_handle: ContextHandle<TimeZone>,
    _setup: SetupChannel,
    _broadcast: ws::Listener,
    _list_req: ws::Request,
}

pub(super) enum Msg {
    Channel(Result<ws::Channel, ws::Error>),
    AppBroadcast(Result<ws::Packet<api::AppBroadcast>, ws::Error>),
    SeriesLoaded(Result<ws::Packet<api::ListSeries>, ws::Error>),
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

impl Component for SeriesList {
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
            series: Vec::new(),
            page: ctx.props().page,
            filter: ctx.props().filter.clone(),
            tz,
            _tz_handle,
            _setup,
            _broadcast,
            _list_req: ws::Request::default(),
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

        let filtered: Vec<&api::Series> = self
            .series
            .iter()
            .filter(|s| {
                filter.is_empty()
                    || s.title
                        .as_ref()
                        .is_some_and(|t| t.to_lowercase().contains(&filter))
            })
            .collect();

        let total = filtered.len();
        let total_pages = total.div_ceil(PAGE_SIZE).max(1);
        let page = self.page.min(total_pages - 1);
        let prev_page = page.checked_sub(1);
        let next_page = (page + 1 < total_pages).then_some(page + 1);

        let page_items: Vec<&api::Series> = filtered
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
                    <span class="fill">{"Series"}</span>
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
                    <div class="empty text-muted">{"No series tracked."}</div>
                } else {
                    <div class="table">
                        { for page_items.into_iter().map(|s| self.view_row(ctx, s)) }
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

impl SeriesList {
    fn try_update(&mut self, ctx: &Context<Self>, msg: Msg) -> Result<bool, Error> {
        match msg {
            Msg::Channel(result) => {
                self.channel = result?;
                if self.channel.id() != ws::ChannelId::NONE {
                    self.load(ctx);
                } else {
                    self.series.clear();
                }
                Ok(true)
            }
            Msg::AppBroadcast(packet) => {
                let event = packet?.decode_event()?;
                if event.channel == self.channel.id() {
                    return Ok(false);
                }
                match event.kind {
                    api::AppEventKind::SeriesCreated { .. }
                    | api::AppEventKind::SeriesChanged { .. }
                    | api::AppEventKind::SeriesDeleted { .. } => {
                        if self.channel.id() != ws::ChannelId::NONE {
                            self.load(ctx);
                        }
                        Ok(false)
                    }
                    _ => Ok(false),
                }
            }
            Msg::SeriesLoaded(result) => {
                self.series = result
                    .context(Message::LoadingSeries)?
                    .decode()
                    .context(Message::LoadingSeries)?
                    .series;
                Ok(true)
            }
            Msg::Filter(s) => {
                self.filter = s;

                ctx.props().on_navigate.emit(Route::Series(PagedQuery {
                    page: 0,
                    filter: self.filter.clone(),
                }));

                Ok(true)
            }
            Msg::SetPage(p) => {
                self.page = p;

                ctx.props().on_navigate.emit(Route::Series(PagedQuery {
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
            .body(api::ListSeriesRequest)
            .on_packet(ctx.link().callback(Msg::SeriesLoaded))
            .send();
    }

    fn view_row(&self, ctx: &Context<Self>, s: &api::Series) -> Html {
        let id = s.id;
        let onclick = ctx.link().callback(move |_| {
            Msg::Navigate(Route::SeriesDetail(id, SeriesDetailQuery::default()))
        });

        html! {
            <div class="table-entry clickable" {onclick}>
                <div class="row">
                    if let Some(poster) = s.poster.as_ref() {
                        <img class="poster-sm" src={poster.proxy_url()} />
                    } else {
                        <div class="poster-sm" />
                    }

                    <div class="row-fill fill">
                        <div class="column fill">
                            if let Some(ref title) = s.title {
                                <span class="item-title">{title}</span>
                            } else {
                                <span class="item-title text-muted">{"Untitled Series"}</span>
                            }

                            if let Some(ref overview) = s.overview {
                                <div class="overview">{overview}</div>
                            }
                        </div>

                        <div class="row top">
                            if let Some(date) = s.first_air_date {
                                <span class="text-muted end">{date.date(self.tz.clone()).year().to_string()}</span>
                            }

                            if !s.tracked {
                                <span class="end icon-inline" title="Untracked series">
                                    <span class="icon eye-slash" />
                                </span>
                            }
                        </div>
                    </div>

                    <span class="icon-inline"><span class="icon chevron-right" /></span>
                </div>
            </div>
        }
    }
}
