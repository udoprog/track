use musli_web::web03::prelude::*;
use yew::prelude::*;

use api::HasAired;

use crate::error::{CustomContext, Error, Message};
use crate::router::{Route, SeriesDetailQuery};
use crate::ui::{ConfirmDanger, MarkWatchedPicker, PaginationButtons};

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
    _skip_req: ws::Request,
    _set_config_req: ws::Request,
    confirming_watch: Option<api::PendingKind>,
    confirming_skip: Option<(api::SeriesId, api::EpisodeId)>,
}

pub(super) enum Msg {
    Channel(Result<ws::Channel, ws::Error>),
    AppBroadcast(Result<ws::Packet<api::AppBroadcast>, ws::Error>),
    PendingLoaded(Result<ws::Packet<api::ListPending>, ws::Error>),
    ConfigLoaded(Result<ws::Packet<api::GetConfig>, ws::Error>),
    AskMarkWatched(api::PendingKind),
    CancelMarkWatch,
    MarkWatched(api::WatchedKind, Option<api::Timestamp>),
    MarkWatchedDone(Result<ws::Packet<api::MarkWatched>, ws::Error>),
    AskSkipEpisode(api::SeriesId, api::EpisodeId),
    CancelSkipEpisode,
    SkipEpisode(api::SeriesId, api::EpisodeId),
    SkipEpisodeDone(Result<ws::Packet<api::SkipEpisode>, ws::Error>),
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
            _skip_req: ws::Request::default(),
            _set_config_req: ws::Request::default(),
            confirming_watch: None,
            confirming_skip: None,
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
            Msg::AskMarkWatched(pending_kind) => {
                self.confirming_watch = Some(pending_kind);
                Ok(true)
            }
            Msg::CancelMarkWatch => {
                self.confirming_watch = None;
                Ok(true)
            }
            Msg::MarkWatched(kind, timestamp) => {
                self.confirming_watch = None;
                self._mark_req = self
                    .channel
                    .request()
                    .body(api::MarkWatchedRequest { kind, timestamp })
                    .on_packet(ctx.link().callback(Msg::MarkWatchedDone))
                    .send();
                Ok(true)
            }
            Msg::MarkWatchedDone(result) => {
                result.context(Message::MarkingWatched)?;

                if self.channel.id() != ws::ChannelId::NONE {
                    self.load_pending(ctx);
                }

                Ok(false)
            }
            Msg::AskSkipEpisode(series, episode) => {
                self.confirming_skip = Some((series, episode));
                self.confirming_watch = None;
                Ok(true)
            }
            Msg::CancelSkipEpisode => {
                self.confirming_skip = None;
                Ok(true)
            }
            Msg::SkipEpisode(series, episode) => {
                self.confirming_skip = None;
                self._skip_req = self
                    .channel
                    .request()
                    .body(api::SkipEpisodeRequest { series, episode })
                    .on_packet(ctx.link().callback(Msg::SkipEpisodeDone))
                    .send();
                Ok(true)
            }
            Msg::SkipEpisodeDone(result) => {
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
        let tz = ctx
            .link()
            .context::<crate::SystemTz>(Callback::noop())
            .map(|(t, _)| t.get().clone())
            .unwrap_or(jiff::tz::TimeZone::UTC);
        let pending_kind = p.kind.clone();
        let confirming_watch = self.confirming_watch.as_ref() == Some(&p.kind);
        let route = match p.kind {
            api::PendingKind::Episode { series, .. } => {
                Route::SeriesDetail(series, SeriesDetailQuery::default())
            }
            api::PendingKind::Movie { movie } => Route::MovieDetail(movie),
        };
        let watched_kind = match p.kind {
            api::PendingKind::Episode { series, episode } => {
                api::WatchedKind::Episode { series, episode }
            }
            api::PendingKind::Movie { movie } => api::WatchedKind::Movie { movie },
        };
        let route_poster = route.clone();
        let on_navigate = ctx.link().callback(move |_| Msg::Navigate(route.clone()));
        let on_navigate_series = on_navigate.clone();
        let on_navigate_poster = ctx
            .link()
            .callback(move |_| Msg::Navigate(route_poster.clone()));
        let on_ask_mark = ctx
            .link()
            .callback(move |_| Msg::AskMarkWatched(pending_kind.clone()));
        let skip_ids = if let api::PendingKind::Episode { series, episode } = p.kind {
            Some((series, episode))
        } else {
            None
        };
        let confirming_skip = self.confirming_skip == skip_ids;
        let aired_at = p.aired_at;
        let aired = p.aired;
        let label = p.label.clone();

        let actions = 'actions: {
            if confirming_watch {
                break 'actions html! {
                    <MarkWatchedPicker
                        {aired_at}
                        {aired}
                        on_confirm={ctx.link().callback(move |ts| Msg::MarkWatched(watched_kind, ts))}
                        on_cancel={ctx.link().callback(|_| Msg::CancelMarkWatch)}
                    />
                };
            }

            if confirming_skip && let Some((series, episode)) = skip_ids {
                break 'actions html! {
                    <ConfirmDanger
                        prompt="Skip"
                        {label}
                        on_confirm={ctx.link().callback(move |_| Msg::SkipEpisode(series, episode))}
                        on_cancel={ctx.link().callback(|_| Msg::CancelSkipEpisode)}
                    />
                };
            }

            html! {
                <>
                    <button class="btn-icon-success" onclick={on_ask_mark} title="Mark watched">
                        <span class="icon check" />
                    </button>

                    if let Some((series, episode)) = skip_ids {
                        <button class="btn-icon" onclick={ctx.link().callback(move |_| Msg::AskSkipEpisode(series, episode))} title="Skip episode">
                            <span class="icon forward" />
                        </button>
                    }
                </>
            }
        };

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

                        if let Some(s) = p.display_at(&tz) {
                            <span class="pending-date">{s}</span>
                        }
                    </div>

                    <div class="pending-actions">
                        {actions}
                    </div>
                </div>
            </div>
        }
    }
}
