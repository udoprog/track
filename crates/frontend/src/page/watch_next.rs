use core::cmp::Reverse;

use musli_web::web03::prelude::*;
use yew::prelude::*;

use api::{TimeInfo, Timed};

use crate::SetupChannel;
use crate::background::Background;
use crate::error::{CustomContext, Error, Message};
use crate::router::{MediaSelection, Route, Router, ShowDetailQuery};
use crate::ui::{
    Button, ConfirmDanger, ContextMenu, DurationInput, Image, MarkTimeMenu, PaginationButtons,
    Skeleton, TimePreset, Variant,
};

struct PendingState {
    pending: api::Pending,
    anchor: NodeRef,
}

pub(crate) struct WatchNext {
    channel: ws::Channel,
    pending: Vec<PendingState>,
    /// Indices into `pending` kept by the media-kind selection. Maintained when
    /// `pending` or the selection changes so `view` and `clamp_page` neither
    /// reallocate nor re-filter per render.
    order: Vec<usize>,
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
    /// Hover a pending item: drive the page background from its backdrop, kept.
    HoverBackdrop(Option<String>),
    AskSkipEpisode(api::ShowId, api::EpisodeId),
    CancelSkipEpisode,
    SkipEpisode(api::ShowId, api::EpisodeId),
    SkipEpisodeDone(Result<ws::Packet<api::SkipEpisode>, ws::Error>),
    MarkPending(api::PendingKind, api::MarkTime),
    MarkPendingDone(Result<ws::Packet<api::AddPending>, ws::Error>),
    AdjustPageSize(i32),
    LookaheadChanged(api::Duration),
    LookaheadSaved(Result<ws::Packet<api::SetConfig>, ws::Error>),
    SetConfigDone(Result<ws::Packet<api::SetConfig>, ws::Error>),
    SetPage(usize),
    Navigate(Route),
    SetTime(TimeInfo),
}

#[derive(Properties, PartialEq)]
pub(crate) struct Props {
    /// Currently visible page (owned by the parent's URL query).
    pub(crate) page: usize,
    /// Navigate to the given page (the parent persists it in the URL).
    pub(crate) on_set_page: Callback<usize>,
    /// Correct the page in the URL without leaving a back-button target (used
    /// when the list shrinks below the current page).
    pub(crate) on_clamp_page: Callback<usize>,
    /// Which media kinds are shown (owned by the parent's URL query).
    pub(crate) selection: MediaSelection,
}

impl Component for WatchNext {
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
            order: Vec::new(),
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

    fn changed(&mut self, ctx: &Context<Self>, old_props: &Self::Properties) -> bool {
        // The media-kind selection lives in props; rebuild the filtered order
        // when it changes. A bare page change leaves the order untouched.
        if old_props.selection != ctx.props().selection {
            self.rebuild_order(ctx);
        }

        true
    }

    fn view(&self, ctx: &Context<Self>) -> Html {
        let page_size = self.page_size();
        let total = self.order.len();
        let total_pages = total.div_ceil(page_size).max(1);
        let page = ctx.props().page.min(total_pages - 1);
        let link = ctx.link();

        html! {
            <div class="column">
                <div class="row desktop-align-end">
                    <div class="input-group desktop-only">
                        <span class="input-label has-text" title="How far into the future pending items are shown">{"Lookahead"}</span>

                        <DurationInput value={self.config.dashboard_lookahead} on_change={link.callback(Msg::LookaheadChanged)} />
                    </div>

                    <div class="input-group desktop-only">
                        <span class="input-label has-text" title="How many items are shown per page">{"Items"}</span>

                        <Button icon="minus" title="Show fewer" onclick={link.callback(|_| Msg::AdjustPageSize(-1))} />

                        <Button icon="plus" title="Show more" onclick={link.callback(|_| Msg::AdjustPageSize(1))} />
                    </div>

                    <PaginationButtons {page} {total_pages} on_page={link.callback(Msg::SetPage)} />
                </div>

                if !self.pending_loaded {
                    <div class="pending-grid" style={format!("--pending-columns: {}", page_size)}>
                        { for (0..page_size).map(|_| Self::view_pending_skeleton()) }
                    </div>
                } else if total == 0 {
                    <p class="text-muted">{"Nothing pending."}</p>
                } else {
                    <div class="pending-grid" style={format!("--pending-columns: {}", page_size)}>
                        { for self.ordered().skip(page * page_size).take(page_size).map(|p| self.view_pending_item(ctx, p)) }
                    </div>
                }

                <div class="column mobile-only">
                    <PaginationButtons {page} {total_pages} on_page={link.callback(Msg::SetPage)} />

                    <div class="input-group fill">
                        <span class="input-label has-text fill" title="How many items are shown per page">{"Items"}</span>

                        <Button icon="minus" title="Show fewer" onclick={link.callback(|_| Msg::AdjustPageSize(-1))} />
                        <Button icon="plus" title="Show more" onclick={link.callback(|_| Msg::AdjustPageSize(1))} />
                    </div>

                    <div class="input-group fill">
                        <span class="input-label has-text fill" title="How far into the future pending items are shown">{"Lookahead"}</span>

                        <DurationInput value={self.config.dashboard_lookahead} on_change={link.callback(Msg::LookaheadChanged)} />
                    </div>
                </div>
            </div>
        }
    }
}

impl WatchNext {
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
                    self.pending_loaded = false;
                    self.rebuild_order(ctx);
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
                        // A changed lookahead moves the server-side cutoff, so
                        // the list has to be reloaded to match it.
                        let lookahead_changed =
                            config.dashboard_lookahead != self.config.dashboard_lookahead;

                        self.config = config;

                        if lookahead_changed {
                            self.load_pending(ctx);
                        }

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
                self.rebuild_order(ctx);
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
            Msg::HoverBackdrop(url) => {
                if url.is_some() {
                    self.background.background(url);
                }
                Ok(false)
            }
            Msg::MarkWatched(kind, mark_time) => {
                if self.channel.id() != ws::ChannelId::NONE {
                    self._mark_req = self
                        .channel
                        .request()
                        .body(api::MarkWatchedRequest { kind, mark_time })
                        .on_packet(ctx.link().callback(Msg::MarkWatchedDone))
                        .send();
                }

                Ok(true)
            }
            Msg::MarkWatchedDone(result) => {
                let response = result
                    .context(Message::MarkingWatched)?
                    .decode()
                    .context(Message::MarkingWatched)?;

                self.background.offer_undo(&response);

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

                if self.channel.id() != ws::ChannelId::NONE {
                    self._skip_req = self
                        .channel
                        .request()
                        .body(api::SkipEpisodeRequest { show, episode })
                        .on_packet(ctx.link().callback(Msg::SkipEpisodeDone))
                        .send();
                }

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
                if self.channel.id() != ws::ChannelId::NONE {
                    self._add_pending_req = self
                        .channel
                        .request()
                        .body(api::AddPendingRequest { kind, mark_time })
                        .on_packet(ctx.link().callback(Msg::MarkPendingDone))
                        .send();
                }

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

                if self.channel.id() != ws::ChannelId::NONE {
                    self._set_config_req = self
                        .channel
                        .request()
                        .body(api::SetConfigRequest {
                            config: self.config.clone(),
                        })
                        .on_packet(ctx.link().callback(Msg::SetConfigDone))
                        .send();
                }

                Ok(true)
            }
            Msg::LookaheadChanged(lookahead) => {
                self.config.dashboard_lookahead = lookahead;

                self._set_config_req = self
                    .channel
                    .request()
                    .body(api::SetConfigRequest {
                        config: self.config.clone(),
                    })
                    .on_packet(ctx.link().callback(Msg::LookaheadSaved))
                    .send();
                Ok(true)
            }
            Msg::LookaheadSaved(result) => {
                result.context(Message::SavingConfig)?;

                // The cutoff is applied server-side against the stored config, so
                // the list can only be reloaded once the new lookahead is saved.
                self.load_pending(ctx);
                Ok(false)
            }
            Msg::SetConfigDone(result) => {
                result.context(Message::SavingConfig)?;
                Ok(false)
            }
            Msg::SetPage(page) => {
                ctx.props().on_set_page.emit(page);
                Ok(false)
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

    /// Rebuild `order` from the current media-kind selection. Called whenever
    /// `pending` or the selection changes. The pending items it indexes are what
    /// pages are counted over.
    fn rebuild_order(&mut self, ctx: &Context<Self>) {
        let selection = ctx.props().selection;

        self.order.clear();
        self.order.extend(
            self.pending
                .iter()
                .enumerate()
                .filter(|(_, p)| selection.contains(p.pending.info.media_kind()))
                .map(|(i, _)| i),
        );
    }

    /// The pending items the media filter shows, in order, as maintained in
    /// `order`.
    fn ordered(&self) -> impl ExactSizeIterator<Item = &PendingState> {
        self.order.iter().map(|&i| &self.pending[i])
    }

    fn clamp_page(&self, ctx: &Context<Self>) {
        // Don't correct the page until the pending list has actually loaded,
        // otherwise an early config response would clamp against an empty list
        // and clobber a deep-linked `?page=N` before its data arrives.
        if !self.pending_loaded {
            return;
        }

        let total_pages = self.order.len().div_ceil(self.page_size()).max(1);
        let page = ctx.props().page.min(total_pages - 1);

        if page != ctx.props().page {
            ctx.props().on_clamp_page.emit(page);
        }
    }

    fn load_pending(&mut self, ctx: &Context<Self>) {
        if self.channel.id() == ws::ChannelId::NONE {
            return;
        }

        self._pending_req = self
            .channel
            .request()
            .body(api::ListPendingRequest)
            .on_packet(ctx.link().callback(Msg::PendingLoaded))
            .send();
    }

    /// Insert or replace a single pending row, keeping the list ordered by its
    /// pending timestamp (most recent first), matching the server's ordering. An
    /// entry dated beyond the configured lookahead falls outside the "next" view
    /// and is dropped, mirroring the server's cutoff.
    fn upsert_pending(&mut self, ctx: &Context<Self>, pending: api::Pending) {
        self.pending
            .retain(|state| state.pending.info.kind() != pending.info.kind());

        let cutoff = self
            .time
            .now()
            .saturating_add(self.config.dashboard_lookahead);

        if pending.timestamp <= cutoff {
            self.pending.push(PendingState {
                pending,
                anchor: NodeRef::default(),
            });

            self.pending
                .sort_by_key(|state| Reverse(state.pending.timestamp));
        }

        self.rebuild_order(ctx);
        self.clamp_page(ctx);
    }

    fn load_config(&mut self, ctx: &Context<Self>) {
        if self.channel.id() == ws::ChannelId::NONE {
            return;
        }

        self._config_req = self
            .channel
            .request()
            .body(api::GetConfigRequest)
            .on_packet(ctx.link().callback(Msg::ConfigLoaded))
            .send();
    }

    /// Placeholder card mirroring `view_pending_item`'s structure, shown for each
    /// slot while the pending list is still loading.
    fn view_pending_skeleton() -> Html {
        html! {
            <div class="pending-item">
                <Skeleton class="poster desktop-only" />
                <Skeleton class="banner mobile-only" />

                <div class="pending-info">
                    <div class="pending-content">
                        <Skeleton class="line" style="width: 80%" />
                        <Skeleton class="line" style="width: 50%" />
                    </div>
                </div>
            </div>
        }
    }

    fn view_pending_item(&self, ctx: &Context<Self>, pending: &PendingState) -> Html {
        let PendingState { pending, anchor } = pending;

        let pending_kind = pending.info.kind();

        let on_navigate;
        let on_navigate_episode;

        match &pending.info {
            api::PendingInfo::Episode {
                show_id,
                season,
                number,
                ..
            } => {
                let show = *show_id;

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
            api::PendingInfo::Movie { movie, .. } => {
                let movie = *movie;

                on_navigate = ctx
                    .link()
                    .callback(move |_| Msg::Navigate(Route::MovieDetail(movie)));

                on_navigate_episode = on_navigate.clone();
            }
        };

        let kind = match pending_kind {
            api::PendingKind::Episode { show, episode } => {
                api::WatchedKind::Episode { show, episode }
            }
            api::PendingKind::Movie { movie } => api::WatchedKind::Movie { movie },
        };

        let preset = pending.aired.map(|timestamp| {
            // "Aired" reads oddly for movies; label that choice "Released" instead.
            let label = match pending_kind {
                api::PendingKind::Episode { .. } => "Aired",
                api::PendingKind::Movie { .. } => "Released",
            };

            TimePreset::at("calendar", label, timestamp)
        });

        let aired_in_past = pending.aired.is_some_and(|a| a <= self.time.now());

        let skip_ids = if let api::PendingKind::Episode { show, episode } = pending_kind {
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

        // Key on identity + current content: when marking watched advances this
        // row to its next episode, the key changes and Yew remounts just this
        // element, replaying the swap-in animation. Unchanged rows keep their
        // key and don't re-animate.
        let key = format!("{pending_kind:?}");

        let backdrop_url = pending.backdrop.as_ref().map(|i| i.proxy_url());
        let onmouseover = ctx
            .link()
            .callback(move |_| Msg::HoverBackdrop(backdrop_url.clone()));

        html! {
            <div {key} class="pending-item" {onmouseover}>
                <Image class="poster clickable desktop-only" src={pending.season_poster.clone().or_else(|| pending.poster.clone())} onclick={on_navigate.clone()} />
                <Image class="banner clickable mobile-only" src={pending.season_banner.clone().or_else(|| pending.banner.clone())} onclick={on_navigate.clone()} />

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
                                <MarkTimeMenu quick=true class="primary" icon="check" title="Mark watched" prompt={format!("When did you watch this {}?", pending_kind.title())} preset={preset.clone()} on_confirm={ctx.link().callback(move |mark_time| Msg::MarkWatched(kind, mark_time))} />
                            } else {
                                <Button icon="check" variant={Variant::Primary} title="Mark watched" onclick={ctx.link().callback(move |_| Msg::MarkWatched(kind, api::MarkTime::Now))} />
                            }

                            <MarkTimeMenu title="Move pending" icon="bookmark" prompt={format!("When do you want to queue this {}?", pending_kind.title())} preset={preset.clone()} on_confirm={ctx.link().callback(move |mark_time| Msg::MarkPending(pending_kind, mark_time))}>
                                <span class="icon bookmark" />
                            </MarkTimeMenu>

                            if let Some((show, episode)) = skip_ids {
                                <Button key="skip-button" node_ref={anchor.clone()} icon="forward" title="Skip episode" onclick={ctx.link().callback(move |_| Msg::AskSkipEpisode(show, episode))} />

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
