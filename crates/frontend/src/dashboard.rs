use musli_web::web03::prelude::*;
use yew::prelude::*;

use api::{HasAired, TimeZone};

use crate::background::Background;
use crate::error::{CustomContext, Error, Message};
use crate::router::{DashboardQuery, Route, Router, ShowDetailQuery};
use crate::ui::{ConfirmDanger, MarkPendingPicker, PaginationButtons};
use crate::{Calendar, Image, SetupChannel};

pub(super) struct Dashboard {
    channel: ws::Channel,
    pending: Vec<api::Pending>,
    pending_loaded: bool,
    config: api::Config,
    tz: TimeZone,
    background: Background,
    router: Router,
    _tz_handle: ContextHandle<TimeZone>,
    _setup: SetupChannel,
    _broadcast: ws::Listener,
    _pending_req: ws::Request,
    _config_req: ws::Request,
    _mark_req: ws::Request,
    _skip_req: ws::Request,
    _set_config_req: ws::Request,
    _add_pending_req: ws::Request,
    confirming_watch: Option<api::PendingKind>,
    confirming_skip: Option<(api::ShowId, api::EpisodeId)>,
    confirming_pending: Option<api::PendingKind>,
}

pub(super) enum Msg {
    Channel(Result<ws::Channel, ws::Error>),
    AppBroadcast(Result<ws::Packet<api::AppBroadcast>, ws::Error>),
    PendingLoaded(Result<ws::Packet<api::ListPending>, ws::Error>),
    ConfigLoaded(Result<ws::Packet<api::GetConfig>, ws::Error>),
    AskMarkWatched(api::PendingKind),
    CancelMarkWatch,
    MarkWatched(api::WatchedKind, api::MarkTime),
    MarkWatchedDone(Result<ws::Packet<api::MarkWatched>, ws::Error>),
    AskSkipEpisode(api::ShowId, api::EpisodeId),
    CancelSkipEpisode,
    SkipEpisode(api::ShowId, api::EpisodeId),
    SkipEpisodeDone(Result<ws::Packet<api::SkipEpisode>, ws::Error>),
    AskMarkPending(api::PendingKind),
    CancelMarkPending,
    MarkPending(api::PendingKind, api::MarkTime),
    MarkPendingDone(Result<ws::Packet<api::AddPending>, ws::Error>),
    AdjustPageSize(i32),
    SetConfigDone(Result<ws::Packet<api::SetConfig>, ws::Error>),
    SetPage(usize),
    Navigate(Route),
    SetTz(TimeZone),
}

#[derive(Properties, PartialEq)]
pub(super) struct Props {
    pub(super) onerror: Callback<Option<Error>>,
    pub(super) page: usize,
}

impl Component for Dashboard {
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

        let (router, _) = ctx
            .link()
            .context::<Router>(Callback::noop())
            .expect("Expected router in context");

        Self {
            channel: ws::Channel::default(),
            pending: Vec::new(),
            pending_loaded: false,
            config: api::Config::default(),
            tz,
            background,
            router,
            _tz_handle,
            _setup,
            _broadcast,
            _pending_req: ws::Request::default(),
            _config_req: ws::Request::default(),
            _mark_req: ws::Request::default(),
            _skip_req: ws::Request::default(),
            _set_config_req: ws::Request::default(),
            _add_pending_req: ws::Request::default(),
            confirming_watch: None,
            confirming_skip: None,
            confirming_pending: None,
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

    fn rendered(&mut self, _ctx: &Context<Self>, first_render: bool) {
        if first_render {
            self.background.title(Some("Dashboard".to_string()));
        }
    }

    fn destroy(&mut self, _ctx: &Context<Self>) {
        self.background.title(None);
    }

    fn view(&self, ctx: &Context<Self>) -> Html {
        html! {
            <>
                { self.view_pending(ctx) }

                <div class="column">
                    <h2>{"Schedule"}</h2>

                    <Calendar onerror={ctx.props().onerror.clone()} />
                </div>
            </>
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
                        self.clamp_page(ctx);
                        Ok(true)
                    }
                    api::AppEventKind::PendingChanged
                    | api::AppEventKind::WatchedChanged { .. }
                    | api::AppEventKind::ShowCreated { .. }
                    | api::AppEventKind::ShowChanged { .. }
                    | api::AppEventKind::ShowDeleted { .. }
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

                self.pending_loaded = true;
                self.clamp_page(ctx);
                Ok(true)
            }
            Msg::ConfigLoaded(result) => {
                self.config = result
                    .context(Message::LoadingPending)?
                    .decode()
                    .context(Message::LoadingPending)?
                    .config;

                self.clamp_page(ctx);
                Ok(true)
            }
            Msg::AskMarkWatched(pending_kind) => {
                self.confirming_watch = Some(pending_kind);
                self.confirming_pending = None;
                Ok(true)
            }
            Msg::CancelMarkWatch => {
                self.confirming_watch = None;
                Ok(true)
            }
            Msg::MarkWatched(kind, mark_time) => {
                self.confirming_watch = None;
                self._mark_req = self
                    .channel
                    .request()
                    .body(api::MarkWatchedRequest { kind, mark_time })
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
            Msg::AskSkipEpisode(show, episode) => {
                self.confirming_skip = Some((show, episode));
                self.confirming_watch = None;
                self.confirming_pending = None;
                Ok(true)
            }
            Msg::CancelSkipEpisode => {
                self.confirming_skip = None;
                Ok(true)
            }
            Msg::SkipEpisode(show, episode) => {
                self.confirming_skip = None;
                self._skip_req = self
                    .channel
                    .request()
                    .body(api::SkipEpisodeRequest { show, episode })
                    .on_packet(ctx.link().callback(Msg::SkipEpisodeDone))
                    .send();
                Ok(true)
            }
            Msg::SkipEpisodeDone(result) => {
                result.context(Message::SkippingEpisode)?;

                if self.channel.id() != ws::ChannelId::NONE {
                    self.load_pending(ctx);
                }

                Ok(false)
            }
            Msg::AskMarkPending(kind) => {
                self.confirming_pending = Some(kind);
                self.confirming_watch = None;
                self.confirming_skip = None;
                Ok(true)
            }
            Msg::CancelMarkPending => {
                self.confirming_pending = None;
                Ok(true)
            }
            Msg::MarkPending(kind, mark_time) => {
                self.confirming_pending = None;
                self._add_pending_req = self
                    .channel
                    .request()
                    .body(api::AddPendingRequest { kind, mark_time })
                    .on_packet(ctx.link().callback(Msg::MarkPendingDone))
                    .send();
                Ok(true)
            }
            Msg::MarkPendingDone(result) => {
                result.context(Message::AddingPending)?;

                if self.channel.id() != ws::ChannelId::NONE {
                    self.load_pending(ctx);
                }

                Ok(false)
            }
            Msg::AdjustPageSize(delta) => {
                let new_size = self
                    .config
                    .dashboard_page
                    .saturating_add_signed(delta)
                    .max(1);
                self.config.dashboard_page = new_size;
                self.clamp_page(ctx);

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
                result.context(Message::SavingConfig)?;
                Ok(false)
            }
            Msg::SetPage(p) => {
                self.router
                    .push(Route::Dashboard(DashboardQuery { page: p }));
                Ok(true)
            }
            Msg::Navigate(route) => {
                self.router.push(route);
                Ok(false)
            }
            Msg::SetTz(tz) => {
                self.tz = tz;
                Ok(true)
            }
        }
    }

    fn clamp_page(&self, ctx: &Context<Self>) {
        // Don't correct the page until the pending list has actually loaded —
        // otherwise an early config response would clamp against an empty list
        // and clobber a deep-linked `?page=N` before its data arrives.
        if !self.pending_loaded {
            return;
        }

        let total_pages = self.pending.len().div_ceil(self.page_size()).max(1);
        let page = ctx.props().page.min(total_pages - 1);

        if page != ctx.props().page {
            // Replace rather than push: this is a URL correction, not a
            // navigation, so it should not leave a back-button target.
            self.router
                .replace(Route::Dashboard(DashboardQuery { page }));
        }
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
        let page = ctx.props().page.min(total_pages - 1);
        let link = ctx.link();

        html! {
            <div class="column">
                <div class="row-fill">
                    <h2>{"Next"}</h2>

                    <div class="row end">
                        <div class="input-group">
                            <button class="btn" title="Show fewer" onclick={link.callback(|_| Msg::AdjustPageSize(-1))}>
                                <span class="icon minus" />
                            </button>

                            <button class="btn" title="Show more" onclick={link.callback(|_| Msg::AdjustPageSize(1))}>
                                <span class="icon plus" />
                            </button>

                            <PaginationButtons {page} {total_pages} on_page={link.callback(Msg::SetPage)} />
                        </div>
                    </div>
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
        let pending_kind = p.kind;
        let confirming_watch = self.confirming_watch.as_ref() == Some(&p.kind);
        let confirming_pending = self.confirming_pending.as_ref() == Some(&p.kind);

        let route = match (p.kind, &p.info) {
            (
                api::PendingKind::Episode { show, .. },
                api::PendingInfo::Episode { season, number, .. },
            ) => Route::ShowDetail(
                show,
                ShowDetailQuery {
                    season: Some(*season),
                    episode: Some(api::Code::new(*season, *number)),
                },
            ),
            (api::PendingKind::Episode { show, .. }, _) => {
                Route::ShowDetail(show, ShowDetailQuery::default())
            }
            (api::PendingKind::Movie { movie }, _) => Route::MovieDetail(movie),
        };

        let kind = match p.kind {
            api::PendingKind::Episode { show, episode } => {
                api::WatchedKind::Episode { show, episode }
            }
            api::PendingKind::Movie { movie } => api::WatchedKind::Movie { movie },
        };

        // "Aired" reads oddly for movies; label that choice "Released" instead.
        let aired_label = match p.kind {
            api::PendingKind::Episode { .. } => "Aired",
            api::PendingKind::Movie { .. } => "Released",
        };

        let on_navigate = ctx.link().callback({
            let route = route.clone();
            move |_| Msg::Navigate(route.clone())
        });

        let now = api::Timestamp::now();
        let aired_in_past = p.aired.is_some_and(|a| a <= now);

        let on_ask_mark = if aired_in_past {
            ctx.link()
                .callback(move |_| Msg::AskMarkWatched(pending_kind))
        } else {
            ctx.link()
                .callback(move |_| Msg::MarkWatched(kind, api::MarkTime::Now))
        };

        let skip_ids = if let api::PendingKind::Episode { show, episode } = p.kind {
            Some((show, episode))
        } else {
            None
        };

        let confirming_skip = self.confirming_skip == skip_ids;

        let title = match &p.info {
            api::PendingInfo::Movie { title, .. } => {
                html! {
                    <span class="pending-title clickable" onclick={on_navigate.clone()} title={title.clone()}>
                        {title.as_deref().unwrap_or("Untitled Movie")}
                    </span>
                }
            }
            api::PendingInfo::Episode {
                show,
                episode,
                season,
                number,
                ..
            } => {
                html! {
                    <>
                        <span class="pending-title clickable" onclick={on_navigate.clone()} title={show.clone()}>
                            {show.as_deref().unwrap_or("Untitled Show")}
                        </span>
                        <span class="pending-label clickable" onclick={on_navigate.clone()}>
                            {format!("{}E{number:02} ─ {}", season.short(), episode.as_deref().unwrap_or("Untitled Episode"))}
                        </span>
                    </>
                }
            }
        };

        let actions = 'actions: {
            if confirming_watch {
                break 'actions html! {
                    <MarkPendingPicker
                        {aired_label}
                        prompt="Watched when?"
                        on_confirm={ctx.link().callback(move |mark_time| Msg::MarkWatched(kind, mark_time))}
                        on_cancel={ctx.link().callback(|_| Msg::CancelMarkWatch)}
                    />
                };
            }

            if confirming_skip && let Some((show, episode)) = skip_ids {
                break 'actions html! {
                    <ConfirmDanger
                        prompt="Skip"
                        on_confirm={ctx.link().callback(move |_| Msg::SkipEpisode(show, episode))}
                        on_cancel={ctx.link().callback(|_| Msg::CancelSkipEpisode)}
                    />
                };
            }

            if confirming_pending {
                break 'actions html! {
                    <MarkPendingPicker
                        {aired_label}
                        prompt="Pending when?"
                        on_confirm={ctx.link().callback(move |mark_time| Msg::MarkPending(pending_kind, mark_time))}
                        on_cancel={ctx.link().callback(|_| Msg::CancelMarkPending)}
                    />
                };
            }

            html! {
                <div class="input-group">
                    <button class="btn-success" onclick={on_ask_mark} title="Mark watched">
                        <span class="icon check" />
                    </button>

                    <button class="btn" onclick={ctx.link().callback(move |_| Msg::AskMarkPending(pending_kind))} title="Mark pending">
                        <span class="icon clock" />
                    </button>

                    if let Some((show, episode)) = skip_ids {
                        <button class="btn" onclick={ctx.link().callback(move |_| Msg::AskSkipEpisode(show, episode))} title="Skip episode">
                            <span class="icon forward" />
                        </button>
                    }
                </div>
            }
        };

        html! {
            <div class="pending-item">
                <Image class="poster clickable hide-mobile" src={p.poster.clone()} onclick={on_navigate.clone()} />
                <Image class="banner clickable hide-desktop" src={p.banner.clone()} onclick={on_navigate.clone()} />

                <div class="pending-info">
                    <div class="pending-content">
                        {title}

                        if let Some(s) = p.display_at(self.tz.clone()) {
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
