use core::cmp::Reverse;

use musli_web::web03::prelude::*;
use yew::prelude::*;

use api::{TimeInfo, Timed};

use crate::SetupChannel;
use crate::background::Background;
use crate::error::{CustomContext, Error, Message};
use crate::router::{DashboardQuery, Route, Router, ShowDetailQuery};
use crate::ui::{
    Button, ConfirmDanger, ContextMenu, Image, MarkTimeMenu, PaginationButtons, TimePreset, Variant,
};

use super::Calendar;

struct PendingState {
    pending: api::Pending,
    anchor: NodeRef,
}

pub(crate) struct Dashboard {
    channel: ws::Channel,
    pending: Vec<PendingState>,
    pending_loaded: bool,
    config: api::Config,
    time: TimeInfo,
    _time_handle: ContextHandle<TimeInfo>,
    background: Background,
    router: Router,
    _setup: SetupChannel,
    _broadcast: ws::Listener,
    _pending_req: ws::Request,
    _config_req: ws::Request,
    _mark_req: ws::Request,
    _skip_req: ws::Request,
    _set_config_req: ws::Request,
    _add_pending_req: ws::Request,
    confirming_skip: Option<(api::ShowId, api::EpisodeId)>,
}

pub(crate) enum Msg {
    Channel(Result<ws::Channel, ws::Error>),
    AppBroadcast(Result<ws::Packet<api::AppBroadcast>, ws::Error>),
    PendingLoaded(Result<ws::Packet<api::ListPending>, ws::Error>),
    ConfigLoaded(Result<ws::Packet<api::GetConfig>, ws::Error>),
    MarkWatched(api::WatchedKind, api::MarkTime),
    MarkWatchedDone(Result<ws::Packet<api::MarkWatched>, ws::Error>),
    AskSkipEpisode(api::ShowId, api::EpisodeId),
    CancelSkipEpisode,
    SkipEpisode(api::ShowId, api::EpisodeId),
    SkipEpisodeDone(Result<ws::Packet<api::SkipEpisode>, ws::Error>),
    MarkPending(api::PendingKind, api::MarkTime),
    MarkPendingDone(Result<ws::Packet<api::AddPending>, ws::Error>),
    AdjustPageSize(i32),
    SetConfigDone(Result<ws::Packet<api::SetConfig>, ws::Error>),
    SetPage(usize),
    Navigate(Route),
    SetTime(TimeInfo),
}

#[derive(Properties, PartialEq)]
pub(crate) struct Props {
    pub(crate) page: usize,
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
            pending: Vec::new(),
            pending_loaded: false,
            config: api::Config::default(),
            time,
            _time_handle,
            background,
            router,
            _setup,
            _broadcast,
            _pending_req: ws::Request::default(),
            _config_req: ws::Request::default(),
            _mark_req: ws::Request::default(),
            _skip_req: ws::Request::default(),
            _set_config_req: ws::Request::default(),
            _add_pending_req: ws::Request::default(),
            confirming_skip: None,
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
                    <h1 class="center">{"Schedule"}</h1>

                    <Calendar />
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
                    api::AppEventKind::PendingEntryChanged { pending } => {
                        self.upsert_pending(ctx, pending);
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
                self.pending.clear();

                let pending = result
                    .context(Message::LoadingPending)?
                    .decode()
                    .context(Message::LoadingPending)?
                    .pending;

                for pending in pending {
                    self.pending.push(PendingState {
                        pending,
                        anchor: NodeRef::default(),
                    });
                }

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
            Msg::MarkWatched(kind, mark_time) => {
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
            Msg::MarkPending(kind, mark_time) => {
                self._add_pending_req = self
                    .channel
                    .request()
                    .body(api::AddPendingRequest { kind, mark_time })
                    .on_packet(ctx.link().callback(Msg::MarkPendingDone))
                    .send();
                Ok(true)
            }
            Msg::MarkPendingDone(result) => {
                let resp = result
                    .context(Message::AddingPending)?
                    .decode()
                    .context(Message::AddingPending)?;

                match resp.pending {
                    // Update just the affected row in place.
                    Some(pending) => self.upsert_pending(ctx, pending),
                    // Media is no longer tracked; reconcile by reloading.
                    None if self.channel.id() != ws::ChannelId::NONE => self.load_pending(ctx),
                    None => {}
                }

                Ok(true)
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
            Msg::SetTime(time) => {
                self.time = time;
                Ok(true)
            }
        }
    }

    fn clamp_page(&self, ctx: &Context<Self>) {
        // Don't correct the page until the pending list has actually loaded,
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

    /// Insert or replace a single pending row, keeping the list ordered by its
    /// pending timestamp (most recent first), matching the server's ordering. An
    /// entry dated in the future falls outside the "next" view and is dropped.
    fn upsert_pending(&mut self, ctx: &Context<Self>, pending: api::Pending) {
        self.pending
            .retain(|state| state.pending.kind != pending.kind);

        if pending.timestamp <= self.time.now() {
            self.pending.push(PendingState {
                pending,
                anchor: NodeRef::default(),
            });

            self.pending
                .sort_by_key(|state| Reverse(state.pending.timestamp));
        }

        self.clamp_page(ctx);
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
                <h1 class="center">{"What's next?"}</h1>

                <div class="row desktop-align-end">
                    <div class="input-group desktop-only">
                        <Button icon="minus" title="Show fewer" onclick={link.callback(|_| Msg::AdjustPageSize(-1))} />

                        <Button icon="plus" title="Show more" onclick={link.callback(|_| Msg::AdjustPageSize(1))} />
                    </div>

                    <PaginationButtons {page} {total_pages} on_page={link.callback(Msg::SetPage)} />
                </div>

                if self.pending.is_empty() {
                    <p class="text-muted">{"Nothing pending."}</p>
                } else {
                    <div class="pending-grid" style={format!("--pending-columns: {}", page_size)}>
                        { for self.pending.iter().skip(page * page_size).take(page_size).map(|p| self.view_pending_item(ctx, p)) }
                    </div>
                }

                <div class="row-split mobile-only">
                    <div class="input-group">
                        <Button icon="minus" title="Show fewer" onclick={link.callback(|_| Msg::AdjustPageSize(-1))} />

                        <Button icon="plus" title="Show more" onclick={link.callback(|_| Msg::AdjustPageSize(1))} />
                    </div>

                    <PaginationButtons {page} {total_pages} on_page={link.callback(Msg::SetPage)} />
                </div>
            </div>
        }
    }

    fn view_pending_item(&self, ctx: &Context<Self>, pending: &PendingState) -> Html {
        let PendingState { pending, anchor } = pending;

        let pending_kind = pending.kind;

        let on_navigate;
        let on_navigate_episode;

        match (pending.kind, &pending.info) {
            (
                api::PendingKind::Episode { show, .. },
                api::PendingInfo::Episode { season, number, .. },
            ) => {
                on_navigate = ctx.link().callback({
                    move |_| Msg::Navigate(Route::ShowDetail(show, ShowDetailQuery::default()))
                });

                on_navigate_episode = ctx.link().callback({
                    let season = *season;
                    let code = api::Code::new(season, *number);

                    move |_| {
                        Msg::Navigate(Route::ShowDetail(
                            show,
                            ShowDetailQuery {
                                season,
                                episode: Some(code),
                                orphaned: false,
                            },
                        ))
                    }
                });
            }
            (api::PendingKind::Episode { show, .. }, _) => {
                on_navigate = ctx.link().callback({
                    move |_| Msg::Navigate(Route::ShowDetail(show, ShowDetailQuery::default()))
                });

                on_navigate_episode = on_navigate.clone();
            }
            (api::PendingKind::Movie { movie }, _) => {
                on_navigate = ctx
                    .link()
                    .callback(move |_| Msg::Navigate(Route::MovieDetail(movie)));

                on_navigate_episode = on_navigate.clone();
            }
        };

        let kind = match pending.kind {
            api::PendingKind::Episode { show, episode } => {
                api::WatchedKind::Episode { show, episode }
            }
            api::PendingKind::Movie { movie } => api::WatchedKind::Movie { movie },
        };

        let preset = pending.aired.map(|timestamp| {
            // "Aired" reads oddly for movies; label that choice "Released" instead.
            let label = match pending.kind {
                api::PendingKind::Episode { .. } => "Aired",
                api::PendingKind::Movie { .. } => "Released",
            };

            TimePreset::at("clock", label, timestamp)
        });

        let aired_in_past = pending.aired.is_some_and(|a| a <= self.time.now());

        let skip_ids = if let api::PendingKind::Episode { show, episode } = pending.kind {
            Some((show, episode))
        } else {
            None
        };

        let skip_code = if let api::PendingInfo::Episode { season, number, .. } = pending.info {
            Some(api::Code::new(season, number))
        } else {
            None
        };

        let confirming = self.confirming_skip.is_some() && (self.confirming_skip == skip_ids);

        let title = match &pending.info {
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

                        <span class="pending-label clickable" onclick={on_navigate_episode.clone()}>
                            {format!("{}E{number:02} ─ {}", season.short(), episode.as_deref().unwrap_or("Untitled Episode"))}
                        </span>
                    </>
                }
            }
        };

        html! {
            <div class="pending-item">
                <Image class="poster clickable desktop-only" src={pending.poster.clone()} onclick={on_navigate.clone()} />
                <Image class="banner clickable mobile-only" src={pending.banner.clone()} onclick={on_navigate.clone()} />

                <div class="pending-info">
                    <div class="pending-content">
                        {title}

                        if let Some(s) = pending.human_date_time(self.time.clone()) {
                            <span class="pending-date">{s}</span>
                        }
                    </div>

                    <div class="pending-actions">
                        <div class="input-group">
                            if aired_in_past {
                                <MarkTimeMenu class="success" icon="check" title="Mark watched" prompt={format!("When did you watch this {}?", pending.kind.title())} preset={preset.clone()} on_confirm={ctx.link().callback(move |mark_time| Msg::MarkWatched(kind, mark_time))}>
                                    <span class="icon check" />
                                </MarkTimeMenu>
                            } else {
                                <Button icon="check" variant={Variant::Success} title="Mark watched" onclick={ctx.link().callback(move |_| Msg::MarkWatched(kind, api::MarkTime::Now))} />
                            }

                            <MarkTimeMenu class="primary" title="Move pending" icon="bookmark" prompt={format!("When do you want to queue this {}?", pending.kind.title())} preset={preset.clone()} on_confirm={ctx.link().callback(move |mark_time| Msg::MarkPending(pending_kind, mark_time))}>
                                <span class="icon bookmark" />
                            </MarkTimeMenu>

                            if let Some((show, episode)) = skip_ids {
                                <Button key="skip-button" node_ref={anchor.clone()} icon="forward" variant={Variant::Danger} title="Skip episode" onclick={ctx.link().callback(move |_| Msg::AskSkipEpisode(show, episode))} />

                                if confirming && let Some(code) = skip_code {
                                    <ContextMenu icon="forward" prompt="Skip episode" label={code} anchor={anchor.clone()} on_close={ctx.link().callback(|_| Msg::CancelSkipEpisode)}>
                                        <ConfirmDanger
                                            on_confirm={ctx.link().callback(move |_| Msg::SkipEpisode(show, episode))}
                                            on_cancel={ctx.link().callback(|_| Msg::CancelSkipEpisode)}
                                        />
                                    </ContextMenu>
                                }
                            }
                        </div>
                    </div>
                </div>
            </div>
        }
    }
}
