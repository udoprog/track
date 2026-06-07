use musli_web::web03::prelude::*;
use yew::prelude::*;

use crate::error::{CustomContext, Error, Message};
use crate::router::{Route, SeriesQuery};
use crate::ui::PaginationButtons;

pub(super) struct Dashboard {
    channel: ws::Channel,
    pending: Vec<api::Pending>,
    config: api::Config,
    page: usize,
    _setup: crate::SetupChannel,
    _broadcast: ws::Listener,
    _pending_req: ws::Request,
    _config_req: ws::Request,
    _mark_req: ws::Request,
    _remove_pending_req: ws::Request,
    _set_config_req: ws::Request,
}

pub(super) enum Msg {
    Channel(Result<ws::Channel, ws::Error>),
    AppBroadcast(Result<ws::Packet<api::AppBroadcast>, ws::Error>),
    PendingLoaded(Result<ws::Packet<api::ListPending>, ws::Error>),
    ConfigLoaded(Result<ws::Packet<api::GetConfig>, ws::Error>),
    MarkWatched(api::WatchedKind),
    MarkWatchedDone(Result<ws::Packet<api::MarkWatched>, ws::Error>),
    RemovePending(api::PendingKind),
    RemovePendingDone(Result<ws::Packet<api::RemovePending>, ws::Error>),
    AdjustPageSize(i32),
    SetConfigDone(Result<ws::Packet<api::SetConfig>, ws::Error>),
    SetPage(usize),
    Navigate(Route),
}

#[derive(Properties, PartialEq)]
pub(super) struct Props {
    pub(super) onerror: Callback<Error>,
    pub(super) on_navigate: Callback<Route>,
}

impl Component for Dashboard {
    type Message = Msg;
    type Properties = Props;

    fn create(ctx: &Context<Self>) -> Self {
        let (ws, _) = ctx
            .link()
            .context::<ws::Handle>(Callback::noop())
            .expect("ws::Handle context not found");

        let _setup = crate::SetupChannel::new(ws.clone(), ctx.link().callback(Msg::Channel));
        let _broadcast = ws.on_broadcast(ctx.link().callback(Msg::AppBroadcast));

        Self {
            channel: ws::Channel::default(),
            pending: Vec::new(),
            config: api::Config::default(),
            page: 0,
            _setup,
            _broadcast,
            _pending_req: ws::Request::default(),
            _config_req: ws::Request::default(),
            _mark_req: ws::Request::default(),
            _remove_pending_req: ws::Request::default(),
            _set_config_req: ws::Request::default(),
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
        html! {
            <div class="page">
                { self.view_pending(ctx) }

                <div class="section">
                    <div class="row"><h2>{"Coming Up"}</h2></div>
                    <crate::Calendar
                        on_navigate={ctx.props().on_navigate.clone()}
                        onerror={ctx.props().onerror.clone()}
                    />
                </div>
            </div>
        }
    }
}

impl Dashboard {
    fn page_size(&self) -> usize {
        self.config.dashboard_page.max(1) as usize
    }

    fn try_update(&mut self, ctx: &Context<Self>, msg: Msg) -> Result<bool, Error> {
        match msg {
            Msg::Channel(result) => {
                self.channel = result?;
                if self.channel.id() != ws::ChannelId::NONE {
                    self.load_pending(ctx);
                    self.load_config(ctx);
                } else {
                    self.pending.clear();
                }
                Ok(true)
            }
            Msg::AppBroadcast(packet) => {
                let event = packet?.decode_event()?;

                if event.channel == self.channel.id() {
                    return Ok(false);
                }

                match event.kind {
                    api::AppEventKind::ConfigChanged { config } => {
                        self.config = config;
                        self.clamp_page();
                        Ok(true)
                    }
                    api::AppEventKind::PendingChanged
                    | api::AppEventKind::WatchedChanged { .. }
                    | api::AppEventKind::SeriesCreated { .. }
                    | api::AppEventKind::SeriesChanged { .. }
                    | api::AppEventKind::SeriesDeleted { .. }
                    | api::AppEventKind::MovieCreated { .. }
                    | api::AppEventKind::MovieChanged { .. }
                    | api::AppEventKind::MovieDeleted { .. }
                    | api::AppEventKind::TaskCompleted { .. } => {
                        self.load_pending(ctx);
                        Ok(false)
                    }
                    _ => Ok(false),
                }
            }
            Msg::PendingLoaded(result) => {
                self.pending = result
                    .context(Message::LoadingPending)?
                    .decode()
                    .context(Message::LoadingPending)?
                    .pending;
                self.clamp_page();
                Ok(true)
            }
            Msg::ConfigLoaded(result) => {
                self.config = result
                    .context(Message::LoadingPending)?
                    .decode()
                    .context(Message::LoadingPending)?
                    .config;
                self.clamp_page();
                Ok(true)
            }
            Msg::MarkWatched(kind) => {
                self._mark_req = self
                    .channel
                    .request()
                    .body(api::MarkWatchedRequest {
                        kind,
                        timestamp: None,
                    })
                    .on_packet(ctx.link().callback(Msg::MarkWatchedDone))
                    .send();
                Ok(false)
            }
            Msg::MarkWatchedDone(result) => {
                result.context(Message::MarkingWatched)?;
                if self.channel.id() != ws::ChannelId::NONE {
                    self.load_pending(ctx);
                }
                Ok(false)
            }
            Msg::RemovePending(kind) => {
                self._remove_pending_req = self
                    .channel
                    .request()
                    .body(api::RemovePendingRequest { kind })
                    .on_packet(ctx.link().callback(Msg::RemovePendingDone))
                    .send();
                Ok(false)
            }
            Msg::RemovePendingDone(result) => {
                result.context(Message::SyncingSeries)?;
                if self.channel.id() != ws::ChannelId::NONE {
                    self.load_pending(ctx);
                }
                Ok(false)
            }
            Msg::AdjustPageSize(delta) => {
                let new_size = (self.config.dashboard_page as i32 + delta).max(1) as u32;
                self.config.dashboard_page = new_size;
                self.clamp_page();
                self._set_config_req = self
                    .channel
                    .request()
                    .body(api::SetConfigRequest {
                        config: self.config.clone(),
                    })
                    .on_packet(ctx.link().callback(Msg::SetConfigDone))
                    .send();
                Ok(true)
            }
            Msg::SetConfigDone(result) => {
                result.context(Message::SyncingSeries)?;
                Ok(false)
            }
            Msg::SetPage(p) => {
                self.page = p;
                Ok(true)
            }
            Msg::Navigate(route) => {
                ctx.props().on_navigate.emit(route);
                Ok(false)
            }
        }
    }

    fn clamp_page(&mut self) {
        let total_pages = self.pending.len().div_ceil(self.page_size()).max(1);
        self.page = self.page.min(total_pages - 1);
    }

    fn load_pending(&mut self, ctx: &Context<Self>) {
        self._pending_req = self
            .channel
            .request()
            .body(api::ListPendingRequest)
            .on_packet(ctx.link().callback(Msg::PendingLoaded))
            .send();
    }

    fn load_config(&mut self, ctx: &Context<Self>) {
        self._config_req = self
            .channel
            .request()
            .body(api::GetConfigRequest)
            .on_packet(ctx.link().callback(Msg::ConfigLoaded))
            .send();
    }

    fn view_pending(&self, ctx: &Context<Self>) -> Html {
        let page_size = self.page_size();
        let total = self.pending.len();
        let total_pages = total.div_ceil(page_size).max(1);
        let page = self.page.min(total_pages - 1);
        let link = ctx.link();

        html! {
            <div class="section">
                <div class="row">
                    <h2 class="fill">{"Up Next"}</h2>
                    <button class="btn-icon" title="Show fewer"
                        onclick={link.callback(|_| Msg::AdjustPageSize(-1))}>
                        <span class="icon minus" />
                    </button>
                    <button class="btn-icon" title="Show more"
                        onclick={link.callback(|_| Msg::AdjustPageSize(1))}>
                        <span class="icon plus" />
                    </button>
                    <PaginationButtons
                        {page}
                        {total_pages}
                        on_page={link.callback(Msg::SetPage)}
                    />
                </div>
                if self.pending.is_empty() {
                    <p class="text-muted">{"Nothing pending."}</p>
                } else {
                    <div class="pending-grid" style={format!("--pending-columns: {}", page_size)}>
                        { for self.pending.iter().skip(page * page_size).take(page_size).map(|p| self.view_pending_item(ctx, p)) }
                    </div>
                }
            </div>
        }
    }

    fn view_pending_item(&self, ctx: &Context<Self>, p: &api::Pending) -> Html {
        let kind = p.kind.clone();
        let remove_kind = p.kind.clone();
        let route = match p.kind {
            api::PendingKind::Episode { series, .. } => {
                Route::SeriesDetail(series, SeriesQuery::default())
            }
            api::PendingKind::Movie { movie } => Route::MovieDetail(movie),
        };
        let route_poster = route.clone();
        let on_navigate = ctx.link().callback(move |_| Msg::Navigate(route.clone()));
        let on_navigate_series = on_navigate.clone();
        let on_navigate_poster = ctx
            .link()
            .callback(move |_| Msg::Navigate(route_poster.clone()));
        let on_mark = ctx.link().callback(move |_| {
            Msg::MarkWatched(match kind {
                api::PendingKind::Episode { series, episode } => {
                    api::WatchedKind::Episode { series, episode }
                }
                api::PendingKind::Movie { movie } => api::WatchedKind::Movie { movie },
            })
        });
        let on_remove = ctx
            .link()
            .callback(move |_| Msg::RemovePending(remove_kind.clone()));

        html! {
            <div class="pending-item">
                if let Some(ref poster) = p.poster {
                    <img class="pending-poster clickable" src={poster.proxy_url()} onclick={on_navigate_poster} />
                } else {
                    <div class="pending-poster pending-poster-placeholder" />
                }

                <div class="pending-info">
                    <div class="pending-content">
                        if let Some(ref title) = p.series_title {
                            <span class="pending-label clickable" onclick={on_navigate_series}>{title}</span>
                        }

                        if matches!(p.kind, api::PendingKind::Movie { .. }) {
                            <span class="pending-label clickable" onclick={on_navigate.clone()}>{&p.label}</span>
                        } else {
                            <span class="pending-label">{&p.label}</span>
                        }

                        if let Some(date) = p.aired_at {
                            <span class="pending-date">{date.to_string()}</span>
                        } else if let Some(date) = p.aired {
                            <span class="pending-date">{date.to_string()}</span>
                        }
                    </div>

                    <div class="pending-actions">
                        <button class="btn-icon-success" onclick={on_mark} title="Mark watched">
                            <span class="icon check" />
                        </button>
                        <button class="btn-icon" onclick={on_remove} title="Remove from pending">
                            <span class="icon bookmark-slash" />
                        </button>
                    </div>
                </div>
            </div>
        }
    }
}
