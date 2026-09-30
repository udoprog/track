use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};

use musli_web::web03::prelude::*;
use yew::prelude::*;

use api::{TimeInfo, Timed};

use crate::SetupChannel;
use crate::background::Background;
use crate::error::{CustomContext, Error, Message};
use crate::router::{MediaQuery, Route, Router, ShowDetailQuery};
use crate::ui::{
    Button, ConfirmDanger, ContextMenu, DetailHero, DetailSkeleton, EpisodeCacheModal,
    EpisodePicker, GraphicsSourceFilter, Image, ImageGallery, ImageItem, MarkTimeMenu,
    MediaSettingsModal, Modal, OutlineControl, OutlineEntry, OutlineHandle, ReleaseModal,
    ReleaseTarget, RemoteEditor, RemoteSourceKind, SettingsTarget, TimePreset, Tracked,
    TranslatedText, TranslationsModal, Variant,
};

const ORPHAN_HINT: &str = r#"
    These are orphaned watches of this series.
    Orphaned watches are watches that are not associated
    with any valid season and episode, and can occur if
    the source of the series changes the season or
    episode structure of the show.
"#;

const CAP: usize = 6;

struct WatchedState {
    context_anchor: NodeRef,
    watched: api::WatchedEpisode,
}

struct OrphanedWatchedState {
    context_anchor: NodeRef,
    watched: api::OrphanedWatched,
}

/// Load state for the show this page renders.
enum ShowState {
    /// The initial request has not resolved yet.
    Loading,
    /// The backend confirmed there is no such show (e.g. deleted or a
    /// hand-edited URL). Resolves itself if a matching create/change broadcast
    /// arrives.
    Missing,
    Loaded(Box<api::Show>),
}

pub(crate) struct ShowDetail {
    channel: ws::Channel,
    show: ShowState,
    graphics: BTreeMap<api::ImageKind, Vec<ImageItem>>,
    present: BTreeSet<api::ImageSource>,
    graphics_hidden_sources: HashSet<api::ImageSource>,
    season_graphics: BTreeMap<api::ImageKind, Vec<ImageItem>>,
    seasons: Vec<api::Season>,
    credits: Vec<api::Credit>,
    /// Whether the full cast list is expanded past the initial cap.
    credits_expanded: bool,
    selected: Option<usize>,
    expanded_seasons: bool,
    episodes: Vec<api::Episode>,
    pending_episode: Option<(api::Code, api::EpisodeId)>,
    next_unwatched: Option<(api::Code, api::EpisodeId)>,
    confirm_remove: bool,
    remove_anchor: NodeRef,
    syncing: bool,
    /// Episodes with a queued or running `SyncEpisode` task, so their sync button
    /// spins. Driven entirely by the task broadcasts, like [`Self::syncing`].
    syncing_episodes: HashSet<api::EpisodeId>,
    actions_expanded: bool,
    /// The episode whose overflow menu is open, anchored to its trigger.
    episode_menu: Option<api::EpisodeId>,
    episode_menu_anchor: NodeRef,
    season_actions_expanded: HashSet<api::SeasonNumber>,
    confirm_remove_watch: Option<api::WatchedId>,
    watched_by_episode: HashMap<api::EpisodeId, Vec<WatchedState>>,
    history_expanded: HashSet<api::EpisodeId>,
    /// Watched episodes shown in full rather than as a compact row.
    expanded_episodes: HashSet<api::EpisodeId>,
    orphaned: Vec<OrphanedWatchedState>,
    fixing_watched: Option<api::WatchedId>,
    image_modal: bool,
    season_image_modal: bool,
    settings_modal: bool,
    remote_editor: bool,
    show_translations_modal: bool,
    season_translations_modal: bool,
    episode_translations: Option<api::EpisodeId>,
    /// The episode whose air-date releases modal is open. The modal ([`ReleaseModal`])
    /// fetches and renders the releases itself.
    episode_releases_modal: Option<api::EpisodeId>,
    /// The episode whose cache modal is open. The modal ([`EpisodeCacheModal`]) fetches
    /// and clears the entries itself.
    episode_cache_modal: Option<api::EpisodeId>,
    global_sync_kinds: Vec<api::SourceSyncKinds>,
    background: Background,
    router: Router,
    /// Control for the shared outline rail. `_outline` is the live handle whose
    /// drop clears the outline when this view is destroyed.
    outline: OutlineControl,
    _outline: Option<OutlineHandle>,
    /// Fragment (episode code) from the initial URL hash to scroll to once the
    /// referenced element has been rendered. Episodes load asynchronously, so
    /// the element does not exist when the browser first tries to honor the
    /// hash; we retry on each render until it appears, then clear this.
    scroll_target: Option<String>,
    time: TimeInfo,
    _time_info_handle: ContextHandle<TimeInfo>,
    _setup: SetupChannel,
    _broadcast: ws::Listener,
    _show_req: ws::Request,
    _seasons_req: ws::Request,
    _credits_req: ws::Request,
    _episodes_req: ws::Request,
    _mark_req: ws::Request,
    _remove_watch_req: ws::Request,
    _untrack_req: ws::Request,
    _remove_req: ws::Request,
    _sync_req: ws::Request,
    _sync_episode_req: ws::Request,
    _watch_remaining_reqs: ws::Request,
    _watched_req: ws::Request,
    _set_next_req: ws::Request,
    _select_image_req: ws::Request,
    _clear_image_req: ws::Request,
    _pick_best_image_req: ws::Request,
    _reset_image_req: ws::Request,
    _season_images_req: ws::Request,
    _select_season_image_req: ws::Request,
    _clear_season_image_req: ws::Request,
    _set_remote_enabled_req: ws::Request,
    _reorder_remotes_req: ws::Request,
    _set_remote_sync_kinds_req: ws::Request,
    _config_req: ws::Request,
    _orphaned_req: ws::Request,
    _move_req: ws::Request,
    _remote_req: ws::Request,
}

pub(crate) enum Msg {
    Channel(Result<ws::Channel, ws::Error>),
    AppBroadcast(Result<ws::Packet<api::AppBroadcast>, ws::Error>),
    ShowLoaded(Result<ws::Packet<api::GetShow>, ws::Error>),
    SeasonsLoaded(Result<ws::Packet<api::ListSeasons>, ws::Error>),
    CreditsLoaded(Result<ws::Packet<api::ListCredits>, ws::Error>),
    ToggleCreditsExpanded,
    SelectSeason(api::SeasonNumber),
    ToggleExpandSeasons,
    EpisodesLoaded(Result<ws::Packet<api::ListEpisodes>, ws::Error>),
    MarkWatched(api::ShowId, api::EpisodeId, api::MarkTime),
    MarkWatchedDone(Result<ws::Packet<api::MarkWatched>, ws::Error>),
    RemoveWatched(api::WatchedId, api::WatchedKind),
    RemoveWatchedDone(Result<ws::Packet<api::RemoveWatched>, ws::Error>),
    ConfirmRemoveWatch(api::WatchedId),
    CancelRemoveWatch,
    WatchRemaining(api::SeasonNumber, api::MarkTime),
    WatchRemainingDone(Result<ws::Packet<api::MarkWatched>, ws::Error>),
    SetTracked(bool),
    SetTrackedDone(bool, Result<ws::Packet<api::UntrackShow>, ws::Error>),
    ConfirmRemove,
    CancelRemove,
    RemoveShow,
    RemoveDone(Result<ws::Packet<api::RemoveShow>, ws::Error>),
    SyncShow,
    SyncDone(Result<ws::Packet<api::SyncShow>, ws::Error>),
    SyncEpisode(api::EpisodeId),
    SyncEpisodeDone(Result<ws::Packet<api::SyncEpisode>, ws::Error>),
    ToggleHistory(api::EpisodeId),
    ToggleEpisodeDetails(api::EpisodeId),
    WatchedLoaded(Result<ws::Packet<api::ListEpisodesWatched>, ws::Error>),
    OnWatchNext(api::EpisodeId, api::MarkTime),
    AddPendingDone(Result<ws::Packet<api::AddPending>, ws::Error>),
    OnRemoveNext(api::EpisodeId),
    RemovePendingDone(Result<ws::Packet<api::RemovePending>, ws::Error>),
    SelectImage(api::ImageKind, api::ImageId),
    ClearSelectedImage(api::ImageKind),
    SelectImageDone(Result<ws::Packet<api::SelectImage>, ws::Error>),
    ClearSelectedImageDone(Result<ws::Packet<api::ClearSelectedImage>, ws::Error>),
    PickBestImage(Option<api::ImageKind>),
    ResetImageSelection(api::ImageKind),
    PickBestImageDone(Result<ws::Packet<api::PickBestImages>, ws::Error>),
    ResetImageSelectionDone(Result<ws::Packet<api::ResetImageSelection>, ws::Error>),
    ToggleGraphicsSource(api::ImageSource),
    SetRemoteEnabled(api::RemoteId, bool),
    SetRemoteEnabledDone(Result<ws::Packet<api::SetShowRemoteEnabled>, ws::Error>),
    SetRemoteSyncKinds(api::RemoteId, Option<api::SyncKindSet>),
    SetRemoteSyncKindsDone(Result<ws::Packet<api::SetShowRemoteSyncKinds>, ws::Error>),
    ReorderRemotes(Vec<api::RemoteId>),
    ReorderRemotesDone(Result<ws::Packet<api::ReorderShowRemotes>, ws::Error>),
    ConfigLoaded(Result<ws::Packet<api::GetConfig>, ws::Error>),
    OpenImageModal,
    CloseImageModal,
    OpenSeasonImageModal,
    CloseSeasonImageModal,
    SeasonImagesLoaded(Result<ws::Packet<api::GetSeasonImages>, ws::Error>),
    SelectSeasonImage(api::ImageKind, api::ImageId),
    SelectSeasonImageDone(Result<ws::Packet<api::SelectImage>, ws::Error>),
    ClearSelectedSeasonImage(api::ImageKind),
    ClearSelectedSeasonImageDone(Result<ws::Packet<api::ClearSelectedImage>, ws::Error>),
    OpenSettingsModal,
    CloseSettingsModal,
    OpenShowTranslations,
    CloseShowTranslations,
    OpenSeasonTranslations,
    CloseSeasonTranslations,
    OpenEpisodeTranslations(api::EpisodeId),
    CloseEpisodeTranslations,
    OpenEpisodeReleases(api::EpisodeId),
    CloseEpisodeReleases,
    OpenEpisodeCache(api::EpisodeId),
    CloseEpisodeCache,
    OpenRemoteEditor,
    CloseRemoteEditor,
    AddRemote(Option<String>, api::Remote),
    EditRemote(api::RemoteId, Option<String>, api::Remote),
    RemoveRemote(api::RemoteId),
    PurgeRemoteCache(api::RemoteId),
    RemoteDone(Result<(), ws::Error>),
    SetTime(TimeInfo),
    FixWatched(api::WatchedId),
    CancelFixWatched,
    MoveWatched(api::WatchedId, api::SeasonNumber, u32),
    MoveWatchedDone(Result<ws::Packet<api::MoveWatchedEpisode>, ws::Error>),
    OrphanedLoaded(Result<ws::Packet<api::ListOrphanedWatched>, ws::Error>),
    ToggleActionsExpanded,
    ToggleEpisodeMenu(api::EpisodeId),
    ToggleSeasonActionsExpanded(api::SeasonNumber),
    ToggleOrphaned,
}

#[derive(Properties, PartialEq)]
pub(crate) struct Props {
    pub(crate) show_id: api::ShowId,
    #[prop_or_default]
    pub(crate) season: api::SeasonNumber,
    pub(crate) orphaned: bool,
}

impl Component for ShowDetail {
    type Message = Msg;
    type Properties = Props;

    fn create(ctx: &Context<Self>) -> Self {
        let (ws, _) = ctx
            .link()
            .context::<ws::Handle>(Callback::noop())
            .expect("Expected ws::Handle in context");

        let _setup = SetupChannel::new(ws.clone(), ctx.link().callback(Msg::Channel));
        let _broadcast = ws.on_broadcast(ctx.link().callback(Msg::AppBroadcast));

        let (time_info, _time_info_handle) = ctx
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

        let (outline, _) = ctx
            .link()
            .context::<OutlineControl>(Callback::noop())
            .expect("Expected outline control in context");

        let scroll_target = router.hash();

        Self {
            channel: ws::Channel::default(),
            show: ShowState::Loading,
            graphics: BTreeMap::new(),
            present: BTreeSet::new(),
            graphics_hidden_sources: HashSet::new(),
            season_graphics: BTreeMap::new(),
            seasons: Vec::new(),
            credits: Vec::new(),
            credits_expanded: false,
            selected: None,
            expanded_seasons: false,
            episodes: Vec::new(),
            pending_episode: None,
            next_unwatched: None,
            confirm_remove: false,
            remove_anchor: NodeRef::default(),
            syncing: false,
            syncing_episodes: HashSet::new(),
            actions_expanded: false,
            episode_menu: None,
            episode_menu_anchor: NodeRef::default(),
            season_actions_expanded: HashSet::new(),
            confirm_remove_watch: None,
            watched_by_episode: HashMap::new(),
            history_expanded: HashSet::new(),
            expanded_episodes: HashSet::new(),
            orphaned: Vec::new(),
            fixing_watched: None,
            image_modal: false,
            season_image_modal: false,
            settings_modal: false,
            remote_editor: false,
            show_translations_modal: false,
            season_translations_modal: false,
            episode_translations: None,
            episode_releases_modal: None,
            episode_cache_modal: None,
            global_sync_kinds: Vec::new(),
            background,
            router,
            outline,
            _outline: None,
            scroll_target,
            time: time_info,
            _time_info_handle,
            _setup,
            _broadcast,
            _show_req: ws::Request::default(),
            _seasons_req: ws::Request::default(),
            _credits_req: ws::Request::default(),
            _episodes_req: ws::Request::default(),
            _mark_req: ws::Request::default(),
            _remove_watch_req: ws::Request::default(),
            _untrack_req: ws::Request::default(),
            _remove_req: ws::Request::default(),
            _sync_req: ws::Request::default(),
            _sync_episode_req: ws::Request::default(),
            _watch_remaining_reqs: ws::Request::default(),
            _watched_req: ws::Request::default(),
            _set_next_req: ws::Request::default(),
            _select_image_req: ws::Request::default(),
            _clear_image_req: ws::Request::default(),
            _pick_best_image_req: ws::Request::default(),
            _reset_image_req: ws::Request::default(),
            _season_images_req: ws::Request::default(),
            _select_season_image_req: ws::Request::default(),
            _clear_season_image_req: ws::Request::default(),
            _set_remote_enabled_req: ws::Request::default(),
            _reorder_remotes_req: ws::Request::default(),
            _set_remote_sync_kinds_req: ws::Request::default(),
            _config_req: ws::Request::default(),
            _orphaned_req: ws::Request::default(),
            _move_req: ws::Request::default(),
            _remote_req: ws::Request::default(),
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

    fn destroy(&mut self, _ctx: &Context<Self>) {
        self.background.title(None);
    }

    fn rendered(&mut self, _ctx: &Context<Self>, _first_render: bool) {
        // Honor an initial URL hash (e.g. `#S01E05`) once its episode has been
        // rendered. The element is absent on the first renders while episodes
        // stream in, so we retry each render and clear the target on success.
        let Some(target) = self.scroll_target.as_deref() else {
            return;
        };

        if self.router.scroll_to_id(target) {
            self.scroll_target = None;
        }
    }

    fn view(&self, ctx: &Context<Self>) -> Html {
        let show = match &self.show {
            ShowState::Loading => return html!(<DetailSkeleton />),
            ShowState::Missing => {
                return html! {
                    <div class="box info">
                        <span class="icon exclamation-triangle" />
                        <span>{"No such show"}</span>
                    </div>
                };
            }
            ShowState::Loaded(show) => show,
        };

        let link = ctx.link();
        let props = ctx.props();

        html! {
            <>
                <DetailHero
                    title={show.strings.title().unwrap_or("Untitled Show").to_owned()}
                    meta={show.first_air_date.map(|date| date.date(self.time.clone()).year().to_string())}
                    backdrop={show.backdrop.clone()}
                />

                <div class="toolbar">
                    <div class="row detail-sources">
                        {for show.remotes.iter().filter_map(|r| {
                            let url = r.remote.show_url(r.slug.as_deref())?;
                            let id = r.remote.source().as_id();

                            Some(html! {
                                <a class="item-inline-source" href={url} target="_blank" rel="noopener noreferrer" title={format!("Open on {id}")}>
                                    <span class={classes!("logo", id)} />
                                </a>
                            })
                        })}
                    </div>

                    <div class="toolbar-toggle">
                        <Button icon={if self.actions_expanded { "ellipsis-horizontal" } else { "bars-2" }} title="Actions" onclick={link.callback(|_| Msg::ToggleActionsExpanded)} />
                    </div>

                    <div class={classes!("toolbar-dropdown", "desktop-input-group", (!self.actions_expanded).then_some("desktop-only"))}>
                        if !self.orphaned.is_empty() || props.orphaned {
                            <Button
                                icon={if props.orphaned { "ellipsis-horizontal" } else { "exclamation-triangle" }}
                                variant={Variant::Danger}
                                title={if props.orphaned { "View orphaned watches" } else { "Hide orphaned watches" }}
                                text="Orphaned watches"
                                onclick={link.callback(|_| Msg::ToggleOrphaned)}
                            />
                        }

                        <Tracked tracked={show.tracked} ontoggle={link.callback(Msg::SetTracked)} />

                        if !show.remotes.is_empty() {
                            <Button icon="arrow-path" spin={self.syncing} onclick={link.callback(|_| Msg::SyncShow)} title="Sync now" text="Sync" />
                        }

                        <Button icon="language" title="Translations" text="Translations" onclick={link.callback(|_| Msg::OpenShowTranslations)} />

                        <Button icon="cog-6-tooth" title="Settings" text="Settings" onclick={link.callback(|_| Msg::OpenSettingsModal)} />

                        <Button node_ref={self.remove_anchor.clone()} icon="trash" variant={Variant::Danger} class="detached" title="Remove show" text="Remove" onclick={link.callback(|_| Msg::ConfirmRemove)} />

                        if self.confirm_remove {
                            <ContextMenu prompt="Remove show" label={show.strings.title().map(str::to_owned)} anchor={self.remove_anchor.clone()} on_close={ctx.link().callback(|_| Msg::CancelRemove)}>
                                <ConfirmDanger on_confirm={link.callback(|_| Msg::RemoveShow)} on_cancel={link.callback(|_| Msg::CancelRemove)} />
                            </ContextMenu>
                        }
                    </div>
                </div>

                if props.orphaned {
                    <p class="hint">{ORPHAN_HINT}</p>

                    <div class="detail-layout">
                        if let Some(season) = self.selected() {
                            { self.view_sidebar(ctx, show, season) }
                        } else {
                            <div id="detail-sidebar" />
                        }

                        { self.view_orphaned(ctx) }
                    </div>
                } else {
                    <TranslatedText strings={show.strings.clone()} />

                    <div class="detail-layout">
                        <div class="mobile-only">
                            if let Some(ref banner) = show.banner {
                                <Image class="banner" src={banner.clone()} />
                            } else if let Some(ref backdrop) = show.backdrop {
                                <Image class="backdrop" src={backdrop.clone()} />
                            }
                        </div>

                        if let Some(season) = self.selected() {
                            { self.view_sidebar(ctx, show, season) }

                            { self.view_episodes(ctx, season) }
                        }
                    </div>
                }

                if self.image_modal {
                    { self.view_image_modal(ctx) }
                }

                if self.season_image_modal {
                    { self.view_season_image_modal(ctx) }
                }

                if self.settings_modal {
                    <MediaSettingsModal
                        target={SettingsTarget::Show(show.id)}
                        on_edit_graphics={link.callback(|_| Msg::OpenImageModal)}
                        on_edit_remotes={link.callback(|_| Msg::OpenRemoteEditor)}
                        on_close={link.callback(|_| Msg::CloseSettingsModal)}
                    />
                }

                if self.remote_editor {
                    <RemoteEditor
                        title={show.strings.title().unwrap_or("Untitled Show").to_owned()}
                        kind={RemoteSourceKind::Show}
                        remotes={show.remotes.clone()}
                        on_add={link.callback(|(slug, remote)| Msg::AddRemote(slug, remote))}
                        on_edit={link.callback(|(id, slug, remote)| Msg::EditRemote(id, slug, remote))}
                        on_remove={link.callback(Msg::RemoveRemote)}
                        on_purge_cache={link.callback(Msg::PurgeRemoteCache)}
                        on_set_enabled={link.callback(|(id, enabled)| Msg::SetRemoteEnabled(id, enabled))}
                        on_reorder={link.callback(Msg::ReorderRemotes)}
                        on_set_sync_kinds={link.callback(|(id, kinds)| Msg::SetRemoteSyncKinds(id, kinds))}
                        global_sync_kinds={self.global_sync_kinds.clone()}
                        on_close={link.callback(|_| Msg::CloseRemoteEditor)}
                    />
                }

                if self.show_translations_modal {
                    <TranslationsModal
                        target={api::TranslationTarget::Show(show.id)}
                        on_close={link.callback(|_| Msg::CloseShowTranslations)}
                    />
                }

                if self.season_translations_modal {
                    if let Some(season) = self.selected() {
                        <TranslationsModal
                            target={api::TranslationTarget::Season(season.id)}
                            on_close={link.callback(|_| Msg::CloseSeasonTranslations)}
                        />
                    }
                }

                if let Some(episode_id) = self.episode_translations {
                    <TranslationsModal
                        target={api::TranslationTarget::Episode(episode_id)}
                        on_close={link.callback(|_| Msg::CloseEpisodeTranslations)}
                    />
                }
            </>
        }
    }

    fn changed(&mut self, ctx: &Context<Self>, old: &Props) -> bool {
        let props = ctx.props();

        if props.show_id != old.show_id {
            self.show = ShowState::Loading;
            self.seasons.clear();
            self.selected = None;
            self.expanded_seasons = false;
            self.episodes.clear();
            self.pending_episode = None;
            self.next_unwatched = None;
            self.confirm_remove = false;
            self.syncing = false;
            self.confirm_remove_watch = None;
            self.watched_by_episode.clear();

            if self.channel.id() != ws::ChannelId::NONE {
                self.load_show(ctx);
                self.load_seasons(ctx);
                self.load_credits(ctx);
                self.load_history(ctx);
            }

            return true;
        }

        if self.selected().map(|s| s.season) != Some(props.season) {
            self.selected = self
                .seasons
                .iter()
                .enumerate()
                .find(|(_, s)| s.season == props.season)
                .map(|(i, _)| i);

            self.episodes.clear();
            self.pending_episode = None;
            self.next_unwatched = None;
            self.confirm_remove_watch = None;
            self.watched_by_episode.clear();

            if self.channel.id() != ws::ChannelId::NONE {
                self.load_episodes(ctx);
                self.load_orphaned(ctx);
                self.load_history(ctx);
            }

            return true;
        }

        props.orphaned != old.orphaned
    }
}

impl ShowDetail {
    fn try_update(&mut self, ctx: &Context<Self>, msg: Msg) -> Result<bool, Error> {
        let props = ctx.props();

        match msg {
            Msg::Channel(result) => {
                self.channel = result?;

                if self.channel.id() != ws::ChannelId::NONE {
                    self.load_show(ctx);
                    self.load_seasons(ctx);
                    self.load_credits(ctx);
                    self.load_orphaned(ctx);
                    self.load_config(ctx);
                } else {
                    self.show = ShowState::Loading;
                    self.seasons.clear();
                    self.episodes.clear();
                    self.pending_episode = None;
                    self.next_unwatched = None;
                    self.orphaned.clear();
                }

                Ok(true)
            }
            Msg::AppBroadcast(packet) => {
                let event = packet?.decode_event()?;
                if event.channel == self.channel.id() {
                    return Ok(false);
                }
                match &event.kind {
                    api::AppEventKind::ShowChanged { show }
                    | api::AppEventKind::ShowCreated { show }
                        if show.id == props.show_id =>
                    {
                        self.background
                            .background(show.backdrop.as_ref().map(|i| i.proxy_url()));
                        self.show = ShowState::Loaded(Box::new(show.clone()));
                        self.update_graphics();
                        Ok(true)
                    }
                    api::AppEventKind::SeasonsChanged { show_id, .. }
                        if *show_id == props.show_id =>
                    {
                        if self.channel.id() != ws::ChannelId::NONE {
                            self.load_seasons(ctx);
                        }

                        Ok(false)
                    }
                    api::AppEventKind::CreditsChanged {
                        target: api::TranslationTarget::Show(show_id),
                    } if *show_id == props.show_id => {
                        if self.channel.id() != ws::ChannelId::NONE {
                            self.load_credits(ctx);
                        }

                        Ok(false)
                    }
                    api::AppEventKind::PersonChanged { person_id }
                        if self.credits.iter().any(|c| c.person_id == *person_id) =>
                    {
                        if self.channel.id() != ws::ChannelId::NONE {
                            self.load_credits(ctx);
                        }

                        Ok(false)
                    }
                    api::AppEventKind::EpisodesChanged { show_id, season }
                        if *show_id == props.show_id =>
                    {
                        if props.season == *season {
                            self.load_episodes(ctx);
                        }

                        self.load_orphaned(ctx);
                        Ok(false)
                    }
                    api::AppEventKind::PendingChanged
                    | api::AppEventKind::PendingEntryChanged { .. } => {
                        self.load_episodes(ctx);
                        self.load_orphaned(ctx);
                        Ok(false)
                    }
                    api::AppEventKind::TaskAdded { task }
                    | api::AppEventKind::TaskStarted { task } => {
                        if matches!(&task.kind, api::TaskKind::SyncShow { show_id, .. } if *show_id == props.show_id)
                        {
                            self.syncing = true;
                            return Ok(true);
                        }

                        if let api::TaskKind::SyncEpisode {
                            show_id,
                            episode_id,
                            ..
                        } = &task.kind
                            && *show_id == props.show_id
                        {
                            self.syncing_episodes.insert(*episode_id);
                            return Ok(true);
                        }

                        Ok(false)
                    }
                    api::AppEventKind::TaskCompleted { task } => {
                        if matches!(&task.kind, api::TaskKind::SyncShow { show_id, .. } if *show_id == props.show_id)
                        {
                            self.syncing = false;
                            self.load_episodes(ctx);
                            self.load_show(ctx);
                            self.load_seasons(ctx);
                            self.load_orphaned(ctx);
                            return Ok(true);
                        }

                        // The episode itself arrives via EpisodesChanged; this just
                        // stops the button spinning.
                        if let api::TaskKind::SyncEpisode {
                            show_id,
                            episode_id,
                            ..
                        } = &task.kind
                            && *show_id == props.show_id
                        {
                            self.syncing_episodes.remove(episode_id);
                            return Ok(true);
                        }

                        Ok(false)
                    }
                    api::AppEventKind::WatchedChanged { event: kind } => {
                        let relevant = match kind {
                            api::WatchedEvent::Episode { show, .. } => *show == props.show_id,
                            api::WatchedEvent::RemainingSeason { show, .. } => {
                                *show == props.show_id
                            }
                            api::WatchedEvent::Movie { .. } => false,
                        };

                        if relevant {
                            self.load_episodes(ctx);
                            self.load_orphaned(ctx);
                            self.load_history(ctx);
                        }

                        Ok(false)
                    }
                    _ => Ok(false),
                }
            }
            Msg::ShowLoaded(result) => {
                let show = result
                    .context(Message::LoadingShow)?
                    .decode()
                    .context(Message::LoadingShow)?;

                match show {
                    Some(show) => {
                        self.background
                            .background(show.backdrop.as_ref().map(|i| i.proxy_url()));
                        self.background
                            .title(show.strings.title().map(str::to_owned));
                        self.show = ShowState::Loaded(Box::new(show));
                        self.update_graphics();
                    }
                    None => {
                        self.background.background(None);
                        self.background.title(None);
                        self.show = ShowState::Missing;
                        self.update_graphics();
                    }
                }

                Ok(true)
            }
            Msg::SeasonsLoaded(result) => {
                self.seasons = result
                    .context(Message::LoadingSeasons)?
                    .decode()
                    .context(Message::LoadingSeasons)?
                    .seasons;

                if self.selected.is_none() {
                    let initial = self
                        .seasons
                        .iter()
                        .enumerate()
                        .find(|(_, s)| s.season == props.season)
                        .map(|(i, _)| i);

                    self.selected = initial.or_else(|| {
                        self.seasons
                            .iter()
                            .enumerate()
                            .find(|(_, s)| !s.season.is_special())
                            .map(|(i, _)| i)
                    });

                    self.load_episodes(ctx);
                    self.load_history(ctx);
                    self.load_orphaned(ctx);
                }

                Ok(true)
            }
            Msg::CreditsLoaded(result) => {
                self.credits = result
                    .context(Message::LoadingCredits)?
                    .decode()
                    .context(Message::LoadingCredits)?
                    .credits;

                Ok(true)
            }
            Msg::ToggleCreditsExpanded => {
                self.credits_expanded = !self.credits_expanded;
                Ok(true)
            }
            Msg::SelectSeason(season) => {
                if self.selected().map(|s| s.season) != Some(season) || props.orphaned {
                    let id = props.show_id;

                    self.router.push(Route::ShowDetail(
                        id,
                        ShowDetailQuery {
                            season,
                            episode: None,
                            orphaned: false,
                        },
                    ));
                }

                self.expanded_seasons = false;
                Ok(false)
            }
            Msg::ToggleExpandSeasons => {
                self.expanded_seasons = !self.expanded_seasons;

                if props.orphaned {
                    self.router.push(Route::ShowDetail(
                        props.show_id,
                        ShowDetailQuery {
                            season: props.season,
                            episode: None,
                            orphaned: false,
                        },
                    ));
                }

                Ok(true)
            }
            Msg::EpisodesLoaded(result) => {
                let result = result
                    .context(Message::LoadingEpisodes)?
                    .decode()
                    .context(Message::LoadingEpisodes)?;

                self.watched_by_episode.clear();

                self.episodes = result.episodes;

                self.pending_episode = self
                    .episodes
                    .iter()
                    .find(|e| e.pending.is_some())
                    .map(|e| (e.code(), e.id));

                self.next_unwatched = self
                    .episodes
                    .iter()
                    .find(|e| e.watched_count == 0)
                    .map(|e| (e.code(), e.id));

                self.update_outline();

                for watched in result.watched {
                    self.watched_by_episode
                        .entry(watched.episode_id)
                        .or_default()
                        .push(WatchedState {
                            watched,
                            context_anchor: NodeRef::default(),
                        });
                }

                self.history_expanded.retain(|episode_id| {
                    self.watched_by_episode
                        .get(episode_id)
                        .is_some_and(|watched| !watched.is_empty())
                });

                Ok(true)
            }
            Msg::MarkWatched(show, episode, mark_time) => {
                self.episode_menu = None;

                if self.channel.id() != ws::ChannelId::NONE {
                    self._mark_req = self
                        .channel
                        .request()
                        .body(api::MarkWatchedRequest {
                            kind: api::WatchedKind::Episode { show, episode },
                            mark_time,
                        })
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

                // Refresh season counts so the progress bars reflect the mark.
                self.load_episodes(ctx);
                self.load_seasons(ctx);
                self.load_history(ctx);
                self.load_orphaned(ctx);
                Ok(false)
            }
            Msg::RemoveWatched(id, kind) => {
                self.orphaned.retain(|w| w.watched.id != id);

                self.actions_expanded = false;

                if let api::WatchedKind::Episode { .. } = kind {
                    self.episode_menu = None;
                }

                if self.orphaned.is_empty() && props.orphaned {
                    self.router.push(Route::ShowDetail(
                        props.show_id,
                        ShowDetailQuery {
                            season: props.season,
                            episode: None,
                            orphaned: false,
                        },
                    ));
                }

                if self.channel.id() != ws::ChannelId::NONE {
                    self._remove_watch_req = self
                        .channel
                        .request()
                        .body(api::RemoveWatchedRequest { id, kind })
                        .on_packet(ctx.link().callback(Msg::RemoveWatchedDone))
                        .send();
                }

                Ok(false)
            }
            Msg::RemoveWatchedDone(result) => {
                result.context(Message::RemovingWatched)?;
                self.confirm_remove_watch = None;

                // Refresh season counts so the progress bars reflect the change.
                self.load_episodes(ctx);
                self.load_seasons(ctx);
                self.load_history(ctx);
                self.load_orphaned(ctx);
                Ok(false)
            }
            Msg::ConfirmRemoveWatch(episode_id) => {
                self.confirm_remove_watch = Some(episode_id);
                Ok(true)
            }
            Msg::CancelRemoveWatch => {
                self.confirm_remove_watch = None;
                Ok(true)
            }
            Msg::WatchRemaining(season, mark_time) => {
                let show_id = props.show_id;

                if self.channel.id() != ws::ChannelId::NONE {
                    self._watch_remaining_reqs = self
                        .channel
                        .request()
                        .body(api::MarkWatchedRemainingRequest {
                            show_id,
                            season,
                            mark_time,
                        })
                        .on_packet(ctx.link().callback(Msg::WatchRemainingDone))
                        .send();
                }

                Ok(false)
            }
            Msg::WatchRemainingDone(result) => {
                result.context(Message::MarkingWatched)?;

                // Refresh season counts so the progress bars reflect the marks.
                self.load_episodes(ctx);
                self.load_seasons(ctx);
                self.load_history(ctx);
                self.load_orphaned(ctx);
                Ok(false)
            }
            Msg::SetTracked(tracked) => {
                self.actions_expanded = false;

                let id = props.show_id;

                if self.channel.id() != ws::ChannelId::NONE {
                    self._untrack_req = self
                        .channel
                        .request()
                        .body(api::UntrackShowRequest { id, tracked })
                        .on_packet(
                            ctx.link()
                                .callback(move |r| Msg::SetTrackedDone(tracked, r)),
                        )
                        .send();
                }

                Ok(false)
            }
            Msg::SetTrackedDone(tracked, result) => {
                result.context(Message::UntrackingShow)?;
                if let ShowState::Loaded(show) = &mut self.show {
                    show.tracked = tracked;
                }
                Ok(true)
            }
            Msg::ConfirmRemove => {
                self.confirm_remove = true;
                Ok(true)
            }
            Msg::CancelRemove => {
                self.confirm_remove = false;
                Ok(true)
            }
            Msg::RemoveShow => {
                let id = props.show_id;

                if self.channel.id() != ws::ChannelId::NONE {
                    self._remove_req = self
                        .channel
                        .request()
                        .body(api::RemoveShowRequest { id })
                        .on_packet(ctx.link().callback(Msg::RemoveDone))
                        .send();
                }

                Ok(false)
            }
            Msg::RemoveDone(result) => {
                result.context(Message::RemovingShow)?;
                self.router.push(Route::Media(MediaQuery::default()));
                Ok(false)
            }
            Msg::SyncShow => {
                let id = props.show_id;

                if self.channel.id() != ws::ChannelId::NONE {
                    self._sync_req = self
                        .channel
                        .request()
                        .body(api::SyncShowRequest { id })
                        .on_packet(ctx.link().callback(Msg::SyncDone))
                        .send();
                }

                Ok(true)
            }
            Msg::SyncDone(result) => {
                result.context(Message::SyncingShow)?;
                Ok(false)
            }
            Msg::SyncEpisode(episode_id) => {
                self.episode_menu = None;
                let show_id = props.show_id;

                if self.channel.id() != ws::ChannelId::NONE {
                    self._sync_episode_req = self
                        .channel
                        .request()
                        .body(api::SyncEpisodeRequest {
                            show_id,
                            episode_id,
                        })
                        .on_packet(ctx.link().callback(Msg::SyncEpisodeDone))
                        .send();
                }

                Ok(true)
            }
            Msg::SyncEpisodeDone(result) => {
                result.context(Message::SyncingEpisode)?;
                Ok(false)
            }
            Msg::ToggleEpisodeDetails(id) => {
                if !self.expanded_episodes.insert(id) {
                    self.expanded_episodes.remove(&id);
                }

                Ok(true)
            }
            Msg::ToggleHistory(id) => {
                self.episode_menu = None;
                if !self.history_expanded.insert(id) {
                    self.history_expanded.remove(&id);
                }

                Ok(true)
            }
            Msg::WatchedLoaded(result) => {
                let watched = result
                    .context(Message::LoadingWatched)?
                    .decode()
                    .context(Message::LoadingWatched)?
                    .watched;

                self.watched_by_episode.clear();

                for watched in watched {
                    self.watched_by_episode
                        .entry(watched.episode_id)
                        .or_default()
                        .push(WatchedState {
                            watched,
                            context_anchor: NodeRef::default(),
                        });
                }

                self.history_expanded.retain(|episode_id| {
                    self.watched_by_episode
                        .get(episode_id)
                        .is_some_and(|watched| !watched.is_empty())
                });

                Ok(true)
            }
            Msg::OnWatchNext(episode_id, mark_time) => {
                self.episode_menu = None;

                let show_id = props.show_id;

                if self.channel.id() != ws::ChannelId::NONE {
                    self._set_next_req = self
                        .channel
                        .request()
                        .body(api::AddPendingRequest {
                            kind: api::PendingKind::Episode {
                                show: show_id,
                                episode: episode_id,
                            },
                            mark_time,
                        })
                        .on_packet(ctx.link().callback(Msg::AddPendingDone))
                        .send();
                }

                Ok(true)
            }
            Msg::AddPendingDone(result) => {
                result
                    .context(Message::AddingPending)?
                    .decode()
                    .context(Message::AddingPending)?;
                self.load_episodes(ctx);
                self.load_orphaned(ctx);
                Ok(false)
            }
            Msg::OnRemoveNext(episode_id) => {
                self.episode_menu = None;
                let show_id = props.show_id;

                if self.channel.id() != ws::ChannelId::NONE {
                    self._set_next_req = self
                        .channel
                        .request()
                        .body(api::RemovePendingRequest {
                            kind: api::PendingKind::Episode {
                                show: show_id,
                                episode: episode_id,
                            },
                        })
                        .on_packet(ctx.link().callback(Msg::RemovePendingDone))
                        .send();
                }

                Ok(false)
            }
            Msg::RemovePendingDone(result) => {
                result
                    .context(Message::RemovingPending)?
                    .decode()
                    .context(Message::RemovingPending)?;
                self.load_episodes(ctx);
                Ok(false)
            }
            Msg::SelectImage(kind, id) => {
                if let Some(images) = self.graphics.get_mut(&kind) {
                    for image in images {
                        image.selected = image.id == id;
                    }
                }

                if self.channel.id() != ws::ChannelId::NONE {
                    self._select_image_req = self
                        .channel
                        .request()
                        .body(api::SelectImageRequest { id })
                        .on_packet(ctx.link().callback(Msg::SelectImageDone))
                        .send();
                }

                Ok(true)
            }
            Msg::ClearSelectedImage(kind) => {
                if let Some(images) = self.graphics.get_mut(&kind) {
                    for image in images {
                        image.selected = false;
                    }
                }

                if self.channel.id() != ws::ChannelId::NONE {
                    self._clear_image_req = self
                        .channel
                        .request()
                        .body(api::ClearSelectedImageRequest {
                            owner: api::ImageOwner::Show(props.show_id),
                            kind,
                        })
                        .on_packet(ctx.link().callback(Msg::ClearSelectedImageDone))
                        .send();
                }

                Ok(false)
            }
            Msg::SelectImageDone(result) => {
                result.context(Message::SelectingImage)?;
                self.load_show(ctx);
                Ok(true)
            }
            Msg::ClearSelectedImageDone(result) => {
                result.context(Message::ClearingImage)?;
                self.image_modal = false;
                self.load_show(ctx);
                Ok(true)
            }
            Msg::PickBestImage(kind) => {
                if self.channel.id() != ws::ChannelId::NONE {
                    self._pick_best_image_req = self
                        .channel
                        .request()
                        .body(api::PickBestImagesRequest {
                            owner: api::ImageOwner::Show(props.show_id),
                            kind,
                        })
                        .on_packet(ctx.link().callback(Msg::PickBestImageDone))
                        .send();
                }

                Ok(false)
            }
            Msg::ResetImageSelection(kind) => {
                if self.channel.id() != ws::ChannelId::NONE {
                    self._reset_image_req = self
                        .channel
                        .request()
                        .body(api::ResetImageSelectionRequest {
                            owner: api::ImageOwner::Show(props.show_id),
                            kind,
                        })
                        .on_packet(ctx.link().callback(Msg::ResetImageSelectionDone))
                        .send();
                }

                Ok(false)
            }
            Msg::PickBestImageDone(result) => {
                result.context(Message::SelectingImage)?;
                self.load_show(ctx);
                Ok(true)
            }
            Msg::ResetImageSelectionDone(result) => {
                result.context(Message::SelectingImage)?;
                self.load_show(ctx);
                Ok(true)
            }
            Msg::ToggleGraphicsSource(source) => {
                if !self.graphics_hidden_sources.remove(&source) {
                    self.graphics_hidden_sources.insert(source);
                }

                Ok(true)
            }
            Msg::SetRemoteEnabled(remote_id, enabled) => {
                if let ShowState::Loaded(show) = &mut self.show
                    && let Some(entry) = show.remotes.iter_mut().find(|e| e.id == remote_id)
                {
                    entry.enabled = enabled;
                }

                let id = props.show_id;

                if self.channel.id() != ws::ChannelId::NONE {
                    self._set_remote_enabled_req = self
                        .channel
                        .request()
                        .body(api::SetShowRemoteEnabledRequest {
                            id,
                            remote_id,
                            enabled,
                        })
                        .on_packet(ctx.link().callback(Msg::SetRemoteEnabledDone))
                        .send();
                }

                Ok(true)
            }
            Msg::SetRemoteEnabledDone(result) => {
                result.context(Message::SettingRemoteEnabled)?;
                Ok(true)
            }
            Msg::SetRemoteSyncKinds(remote_id, sync_kinds) => {
                if let ShowState::Loaded(show) = &mut self.show
                    && let Some(entry) = show.remotes.iter_mut().find(|e| e.id == remote_id)
                {
                    entry.sync_kinds = sync_kinds;
                }

                let id = props.show_id;

                if self.channel.id() != ws::ChannelId::NONE {
                    self._set_remote_sync_kinds_req = self
                        .channel
                        .request()
                        .body(api::SetShowRemoteSyncKindsRequest {
                            id,
                            remote_id,
                            sync_kinds,
                        })
                        .on_packet(ctx.link().callback(Msg::SetRemoteSyncKindsDone))
                        .send();
                }

                Ok(true)
            }
            Msg::SetRemoteSyncKindsDone(result) => {
                result.context(Message::SettingRemoteSyncKinds)?;
                Ok(true)
            }
            Msg::ConfigLoaded(result) => {
                self.global_sync_kinds = result
                    .context(Message::LoadingConfig)?
                    .decode()
                    .context(Message::LoadingConfig)?
                    .config
                    .sync_kinds;
                Ok(true)
            }
            Msg::ReorderRemotes(remote_ids) => {
                if let ShowState::Loaded(show) = &mut self.show {
                    show.remotes
                        .sort_by_key(|e| remote_ids.iter().position(|id| *id == e.id));
                }

                let id = props.show_id;

                if self.channel.id() != ws::ChannelId::NONE {
                    self._reorder_remotes_req = self
                        .channel
                        .request()
                        .body(api::ReorderShowRemotesRequest { id, remote_ids })
                        .on_packet(ctx.link().callback(Msg::ReorderRemotesDone))
                        .send();
                }

                Ok(true)
            }
            Msg::ReorderRemotesDone(result) => {
                result.context(Message::ReorderingRemotes)?;
                Ok(true)
            }
            Msg::OpenImageModal => {
                self.image_modal = true;
                self.settings_modal = false;
                Ok(true)
            }
            Msg::CloseImageModal => {
                self.image_modal = false;
                Ok(true)
            }
            Msg::OpenSeasonImageModal => {
                if let Some(id) = self.selected().map(|s| s.id) {
                    self.season_image_modal = true;
                    self.load_season_images(ctx, id);
                }

                Ok(true)
            }
            Msg::CloseSeasonImageModal => {
                self.season_image_modal = false;
                Ok(true)
            }
            Msg::SeasonImagesLoaded(result) => {
                let packet = result
                    .context(Message::LoadingSeasonImages)?
                    .decode()
                    .context(Message::LoadingSeasonImages)?;

                self.season_graphics.clear();

                for i in &packet.images {
                    let selected = self
                        .selected()
                        .map(|s| s.is_selected(i.kind, i.image.key()))
                        .unwrap_or(false);

                    self.season_graphics
                        .entry(i.kind)
                        .or_default()
                        .push(ImageItem {
                            selected,
                            id: i.id,
                            kind: i.kind,
                            source: i.source,
                            image: i.image.clone(),
                            score: i.score,
                        });
                }

                Ok(true)
            }
            Msg::SelectSeasonImage(kind, id) => {
                if let Some(items) = self.season_graphics.get_mut(&kind) {
                    for item in items {
                        item.selected = item.id == id;
                    }
                }

                if self.channel.id() != ws::ChannelId::NONE {
                    self._select_season_image_req = self
                        .channel
                        .request()
                        .body(api::SelectImageRequest { id })
                        .on_packet(ctx.link().callback(Msg::SelectSeasonImageDone))
                        .send();
                }

                Ok(true)
            }
            Msg::SelectSeasonImageDone(result) => {
                result.context(Message::SelectingImage)?;
                Ok(true)
            }
            Msg::ClearSelectedSeasonImage(kind) => {
                if let Some(items) = self.season_graphics.get_mut(&kind) {
                    for item in items {
                        item.selected = false;
                    }
                }

                if let Some(season) = self.selected()
                    && self.channel.id() != ws::ChannelId::NONE
                {
                    self._clear_season_image_req = self
                        .channel
                        .request()
                        .body(api::ClearSelectedImageRequest {
                            owner: api::ImageOwner::Season(season.id),
                            kind,
                        })
                        .on_packet(ctx.link().callback(Msg::ClearSelectedSeasonImageDone))
                        .send();
                }

                Ok(true)
            }
            Msg::ClearSelectedSeasonImageDone(result) => {
                result.context(Message::ClearingImage)?;
                self.season_image_modal = false;
                Ok(true)
            }
            Msg::OpenSettingsModal => {
                self.settings_modal = true;
                self.actions_expanded = false;
                Ok(true)
            }
            Msg::CloseSettingsModal => {
                self.settings_modal = false;
                Ok(true)
            }
            Msg::OpenShowTranslations => {
                self.show_translations_modal = true;
                self.actions_expanded = false;
                Ok(true)
            }
            Msg::CloseShowTranslations => {
                self.show_translations_modal = false;
                Ok(true)
            }
            Msg::OpenSeasonTranslations => {
                self.season_translations_modal = true;
                Ok(true)
            }
            Msg::CloseSeasonTranslations => {
                self.season_translations_modal = false;
                Ok(true)
            }
            Msg::OpenEpisodeTranslations(episode_id) => {
                self.episode_menu = None;
                self.episode_translations = Some(episode_id);
                Ok(true)
            }
            Msg::CloseEpisodeTranslations => {
                self.episode_translations = None;
                Ok(true)
            }
            Msg::OpenEpisodeReleases(episode_id) => {
                self.episode_menu = None;
                self.episode_releases_modal = Some(episode_id);
                Ok(true)
            }
            Msg::CloseEpisodeReleases => {
                self.episode_releases_modal = None;
                Ok(true)
            }
            Msg::OpenEpisodeCache(episode_id) => {
                self.episode_menu = None;
                self.episode_cache_modal = Some(episode_id);
                Ok(true)
            }
            Msg::CloseEpisodeCache => {
                self.episode_cache_modal = None;
                Ok(true)
            }
            Msg::OpenRemoteEditor => {
                self.remote_editor = true;
                self.settings_modal = false;
                Ok(true)
            }
            Msg::CloseRemoteEditor => {
                self.remote_editor = false;
                Ok(true)
            }
            Msg::AddRemote(slug, remote) => {
                let id = props.show_id;

                if self.channel.id() != ws::ChannelId::NONE {
                    self._remote_req = self
                        .channel
                        .request()
                        .body(api::AddShowRemoteRequest { id, slug, remote })
                        .on_packet(ctx.link().callback(
                            |r: Result<ws::Packet<api::AddShowRemote>, ws::Error>| {
                                Msg::RemoteDone(r.map(|_| ()))
                            },
                        ))
                        .send();
                }

                Ok(false)
            }
            Msg::EditRemote(remote_id, slug, remote) => {
                let id = props.show_id;

                if self.channel.id() != ws::ChannelId::NONE {
                    self._remote_req = self
                        .channel
                        .request()
                        .body(api::UpdateShowRemoteRequest {
                            id,
                            remote_id,
                            slug,
                            remote,
                        })
                        .on_packet(ctx.link().callback(
                            |r: Result<ws::Packet<api::UpdateShowRemote>, ws::Error>| {
                                Msg::RemoteDone(r.map(|_| ()))
                            },
                        ))
                        .send();
                }

                Ok(false)
            }
            Msg::RemoveRemote(remote_id) => {
                let id = props.show_id;

                if self.channel.id() != ws::ChannelId::NONE {
                    self._remote_req = self
                        .channel
                        .request()
                        .body(api::RemoveShowRemoteRequest { id, remote_id })
                        .on_packet(ctx.link().callback(
                            |r: Result<ws::Packet<api::RemoveShowRemote>, ws::Error>| {
                                Msg::RemoteDone(r.map(|_| ()))
                            },
                        ))
                        .send();
                }

                Ok(false)
            }
            Msg::PurgeRemoteCache(remote_id) => {
                let id = props.show_id;

                if self.channel.id() != ws::ChannelId::NONE {
                    self._remote_req = self
                        .channel
                        .request()
                        .body(api::PurgeShowRemoteCacheRequest { id, remote_id })
                        .on_packet(ctx.link().callback(
                            |r: Result<ws::Packet<api::PurgeShowRemoteCache>, ws::Error>| {
                                Msg::RemoteDone(r.map(|_| ()))
                            },
                        ))
                        .send();
                }

                Ok(false)
            }
            Msg::RemoteDone(result) => {
                result.context(Message::EditingRemotes)?;
                self.load_show(ctx);
                Ok(false)
            }
            Msg::SetTime(time) => {
                self.time = time;
                Ok(true)
            }
            Msg::FixWatched(id) => {
                self.fixing_watched = Some(id);
                self.confirm_remove_watch = None;
                Ok(true)
            }
            Msg::CancelFixWatched => {
                self.fixing_watched = None;
                Ok(true)
            }
            Msg::MoveWatched(id, season, episode) => {
                self.fixing_watched = None;
                let show_id = props.show_id;

                if self.channel.id() != ws::ChannelId::NONE {
                    self._move_req = self
                        .channel
                        .request()
                        .body(api::MoveWatchedEpisodeRequest {
                            id,
                            show_id,
                            season,
                            episode,
                        })
                        .on_packet(ctx.link().callback(Msg::MoveWatchedDone))
                        .send();
                }

                Ok(false)
            }
            Msg::MoveWatchedDone(result) => {
                result
                    .context(Message::MovingWatched)?
                    .decode()
                    .context(Message::MovingWatched)?;

                // Refresh season counts so the progress bars reflect the move.
                self.load_episodes(ctx);
                self.load_history(ctx);
                self.load_seasons(ctx);
                self.load_orphaned(ctx);
                Ok(false)
            }
            Msg::OrphanedLoaded(result) => {
                let orphaned = result
                    .context(Message::LoadingWatched)?
                    .decode()
                    .context(Message::LoadingWatched)?
                    .watched;

                self.orphaned.clear();

                for watched in orphaned {
                    self.orphaned.push(OrphanedWatchedState {
                        watched,
                        context_anchor: NodeRef::default(),
                    });
                }

                Ok(true)
            }
            Msg::ToggleActionsExpanded => {
                self.actions_expanded = !self.actions_expanded;
                Ok(true)
            }
            Msg::ToggleEpisodeMenu(episode_id) => {
                self.episode_menu = (self.episode_menu != Some(episode_id)).then_some(episode_id);
                Ok(true)
            }
            Msg::ToggleSeasonActionsExpanded(season) => {
                if !self.season_actions_expanded.insert(season) {
                    self.season_actions_expanded.remove(&season);
                }

                Ok(true)
            }
            Msg::ToggleOrphaned => {
                self.router.push(Route::ShowDetail(
                    props.show_id,
                    ShowDetailQuery {
                        season: props.season,
                        episode: None,
                        orphaned: !props.orphaned,
                    },
                ));

                Ok(true)
            }
        }
    }

    fn selected(&self) -> Option<&api::Season> {
        self.selected.and_then(|i| self.seasons.get(i))
    }

    fn load_show(&mut self, ctx: &Context<Self>) {
        if self.channel.id() == ws::ChannelId::NONE {
            return;
        }

        let show_id = ctx.props().show_id;

        self._show_req = self
            .channel
            .request()
            .body(api::GetShowRequest { id: show_id })
            .on_packet(ctx.link().callback(Msg::ShowLoaded))
            .send();
    }

    /// Cast grid plus a compact key-crew line. Cast is capped until expanded.
    fn view_credits(&self, ctx: &Context<Self>) -> Html {
        if self.credits.is_empty() {
            return html! {};
        }

        let shown = if self.credits_expanded {
            self.credits.as_slice()
        } else {
            self.credits
                .get(..CAP.min(self.credits.len()))
                .unwrap_or_default()
        };

        html! {
            <section class="credits">
                <h2>{"Cast & crew"}</h2>

                if !shown.is_empty() {
                    <div class="cast-grid">
                        { for shown.iter().map(|c| self.view_credit_card(c, c.character.character().or(c.job.as_deref()))) }
                    </div>
                }

                if self.credits.len() > CAP {
                    <Button icon={if self.credits_expanded { "chevron-up" } else { "chevron-down" }} label={if self.credits_expanded { "Show fewer" } else { "Show all cast" }} title={if self.credits_expanded { "Show fewer cast" } else { "Show all cast" }} class="credits-toggle" onclick={ctx.link().callback(|_| Msg::ToggleCreditsExpanded)} />
                }
            </section>
        }
    }

    /// A clickable credit card - photo, name and a subtitle (the character for cast,
    /// the job for crew) - that navigates to the person's page.
    fn view_credit_card(&self, credit: &api::Credit, subtitle: Option<&str>) -> Html {
        let name = credit.name.title().unwrap_or("Unknown").to_owned();
        let subtitle = subtitle.map(str::to_owned);

        let router = self.router.clone();
        let person_id = credit.person_id;
        let onclick = Callback::from(move |_| router.push(Route::PersonDetail(person_id)));

        html! {
            <div class="cast-card clickable" {onclick}>
                <Image class="cast-photo" placeholder={true} src={credit.profile.clone()} alt={name.clone()} />

                <div class="cast-info">
                    <div class="cast-name">{ name }</div>

                    if let Some(subtitle) = subtitle {
                        <div class="cast-character">{ subtitle }</div>
                    }
                </div>
            </div>
        }
    }

    fn update_graphics(&mut self) {
        self.graphics.clear();
        self.present.clear();

        if let ShowState::Loaded(show) = &self.show {
            for i in &show.images {
                self.graphics.entry(i.kind).or_default().push(ImageItem {
                    selected: show.is_selected(i.kind, i.image.key()),
                    id: i.id,
                    kind: i.kind,
                    source: i.source,
                    image: i.image.clone(),
                    score: i.score,
                });

                self.present.insert(i.source);
            }
        }
    }

    fn load_seasons(&mut self, ctx: &Context<Self>) {
        if self.channel.id() == ws::ChannelId::NONE {
            return;
        }

        let show_id = ctx.props().show_id;

        self._seasons_req = self
            .channel
            .request()
            .body(api::ListSeasonsRequest { show_id })
            .on_packet(ctx.link().callback(Msg::SeasonsLoaded))
            .send();
    }

    fn load_credits(&mut self, ctx: &Context<Self>) {
        if self.channel.id() == ws::ChannelId::NONE {
            return;
        }

        let show_id = ctx.props().show_id;

        self._credits_req = self
            .channel
            .request()
            .body(api::ListCreditsRequest {
                owner: api::CreditOwner::Show(show_id),
            })
            .on_packet(ctx.link().callback(Msg::CreditsLoaded))
            .send();
    }

    fn load_season_images(&mut self, ctx: &Context<Self>, season_id: api::SeasonId) {
        if self.channel.id() == ws::ChannelId::NONE {
            return;
        }

        self._season_images_req = self
            .channel
            .request()
            .body(api::GetSeasonImagesRequest { season_id })
            .on_packet(ctx.link().callback(Msg::SeasonImagesLoaded))
            .send();
    }

    fn load_episodes(&mut self, ctx: &Context<Self>) {
        if self.channel.id() == ws::ChannelId::NONE {
            return;
        }

        let show_id = ctx.props().show_id;
        let season = ctx.props().season;

        self._episodes_req = self
            .channel
            .request()
            .body(api::ListEpisodesRequest { show_id, season })
            .on_packet(ctx.link().callback(Msg::EpisodesLoaded))
            .send();
    }

    /// Sync the shared outline rail with the currently loaded episodes. Pushes
    /// the entries through the existing handle, or attaches a new one; clears
    /// the outline (by dropping the handle) when there are no episodes.
    fn update_outline(&mut self) {
        if self.episodes.is_empty() {
            self._outline = None;
            return;
        }

        let entries = self
            .episodes
            .iter()
            .map(|e| {
                let code = AttrValue::from(e.code().to_string());

                OutlineEntry {
                    code: code.clone(),
                    label: code,
                    seen: e.watched_count > 0,
                    pending: e.pending.is_some(),
                }
            })
            .collect();

        match &self._outline {
            Some(handle) => handle.set(entries),
            None => self._outline = Some(self.outline.attach(entries)),
        }
    }

    fn load_history(&mut self, ctx: &Context<Self>) {
        if self.channel.id() == ws::ChannelId::NONE {
            return;
        }

        let show_id = ctx.props().show_id;

        self._watched_req = self
            .channel
            .request()
            .body(api::ListEpisodesWatchedRequest { show_id })
            .on_packet(ctx.link().callback(Msg::WatchedLoaded))
            .send();
    }

    fn load_orphaned(&mut self, ctx: &Context<Self>) {
        if self.channel.id() == ws::ChannelId::NONE {
            return;
        }

        let show_id = ctx.props().show_id;

        self._orphaned_req = self
            .channel
            .request()
            .body(api::ListOrphanedWatchedRequest { show_id })
            .on_packet(ctx.link().callback(Msg::OrphanedLoaded))
            .send();
    }

    /// Load the global config for the per-source sync-kind defaults shown (as
    /// inherited values) in the remote editor.
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

    fn view_sidebar(&self, ctx: &Context<Self>, show: &api::Show, season: &api::Season) -> Html {
        let poster = season.poster.as_ref().or(show.poster.as_ref());

        html! {
            <div class="detail-sidebar">
                <Image class="poster desktop-only" src={poster.cloned()} />

                <div class="table">
                    { for self.seasons.iter().map(|s| self.view_season(ctx, s, self.seasons.len())) }
                </div>
            </div>
        }
    }

    fn view_image_modal(&self, ctx: &Context<Self>) -> Html {
        let link = ctx.link();

        let user_selected = |kind: api::ImageKind| match &self.show {
            ShowState::Loaded(show) => show.is_user_selected(kind),
            _ => false,
        };

        let hidden = self.graphics_hidden_sources.clone();

        html! {
            <Modal icon="photo" title="Graphics" on_close={link.callback(|_| Msg::CloseImageModal)}>
                <div class="row desktop-align-end">
                    <GraphicsSourceFilter present={self.present.clone()} hidden={hidden.clone()} on_toggle={link.callback(Msg::ToggleGraphicsSource)} />
                    <Button icon="sparkles" variant={Variant::Primary} title="Pick the best graphic for every kind" text="Pick best (all)" onclick={link.callback(|_| Msg::PickBestImage(None))} />
                </div>
                {for self.graphics.iter().filter_map(|(&kind, items)| {
                    let items: Vec<ImageItem> = items
                        .iter()
                        .filter(|item| !hidden.contains(&item.source))
                        .cloned()
                        .collect();

                    if items.is_empty() {
                        return None;
                    }

                    Some(html! {
                        <ImageGallery
                            {items}
                            {kind}
                            user_selected={user_selected(kind)}
                            on_select={link.callback(move |id| Msg::SelectImage(kind, id))}
                            on_clear={link.callback(move |_| Msg::ClearSelectedImage(kind))}
                            on_pick_best={Some(link.callback(move |_| Msg::PickBestImage(Some(kind))))}
                            on_reset={Some(link.callback(move |_| Msg::ResetImageSelection(kind)))}
                        />
                    })
                })}
            </Modal>
        }
    }

    fn view_season_image_modal(&self, ctx: &Context<Self>) -> Html {
        let link = ctx.link();

        html! {
            <Modal icon="photo" title="Season Graphics" on_close={link.callback(|_| Msg::CloseSeasonImageModal)}>
                {for self.season_graphics.iter().map(|(&kind, items)| {
                    html! {
                        <ImageGallery
                            items={items.clone()}
                            {kind}
                            on_select={link.callback(move |id| Msg::SelectSeasonImage(kind, id))}
                            on_clear={link.callback(move |_| Msg::ClearSelectedSeasonImage(kind))}
                        />
                    }
                })}
            </Modal>
        }
    }

    fn view_season(&self, ctx: &Context<Self>, s: &api::Season, total: usize) -> Html {
        let season = s.season;
        let active = self.selected().as_ref().map(|s| s.id) == Some(s.id);
        let clickable = total > 1;

        let onclick = if !clickable {
            Callback::noop()
        } else if active {
            ctx.link().callback(move |_| Msg::ToggleExpandSeasons)
        } else {
            ctx.link().callback(move |_| Msg::SelectSeason(season))
        };

        let f = (s.watched_count.min(s.total_count) as f64 * 100.0) / s.total_count.max(1) as f64;
        let style = format!("width: {f:.0}%");

        html! {
            <div class={classes!("column", clickable.then_some("clickable"), active.then_some("active"), (!active && !self.expanded_seasons).then_some("desktop-only"))} {onclick}>
                <div class="row-split fill">
                    <span>
                        if let Some(name) = s.strings.title() {
                            {name}
                        } else {
                            {s.season.long().to_string()}
                        }
                    </span>

                    <div class="row">
                        if s.total_count > 0 {
                            <span class="text-muted" title="Episodes watched">{format!("{}/{}", s.watched_count.min(s.total_count), s.total_count)}</span>
                        }

                        if let Some(ts) = s.air_date {
                            <span class="text-muted">{ts.date(self.time.clone()).year().to_string()}</span>
                        }

                        // Every season is listed on wide screens, so the active one
                        // needs no marker there; on phones it opens the list.
                        if clickable && active {
                            <span class="item-inline mobile-only">
                                <span class="icon chevron-up-down" />
                            </span>
                        } else if clickable {
                            <span class="item-inline">
                                <span class="icon chevron-right" />
                            </span>
                        }
                    </div>
                </div>

                if s.total_count > 0 {
                    <div class="percentage-container">
                        <span class="percentage-fill" {style} />
                    </div>
                }
            </div>
        }
    }

    fn view_episodes(&self, ctx: &Context<Self>, season: &api::Season) -> Html {
        let link = ctx.link();

        let season_number = season.season;

        let watched_count = self
            .episodes
            .iter()
            .filter(|ep| {
                self.watched_by_episode
                    .get(&ep.id)
                    .map(Vec::len)
                    .unwrap_or_default()
                    > 0
            })
            .count();

        let total = self.episodes.len();

        let next_unwatched = self.next_unwatched;

        let pending_episode = self.pending_episode.as_ref().map(|&(label, episode_id)| {
            let callback = link.callback(move |_| Msg::OnRemoveNext(episode_id));
            (label, callback)
        });

        let season_expanded = self.season_actions_expanded.contains(&season_number);
        let toggle_menu =
            link.callback(move |_: MouseEvent| Msg::ToggleSeasonActionsExpanded(season_number));

        let next_episode_preset = next_unwatched.map(|(label, _)| {
            TimePreset::when_aired("calendar", "Aired", format!("When {label} aired"))
        });

        let remaining_description = format!(
            "When each individual episode in {} aired",
            season.season.long()
        );
        let remaining_preset = TimePreset::when_aired("calendar", "Aired", remaining_description);
        let watch_remaining = link.callback({
            let season = season.season;
            move |mark_time| Msg::WatchRemaining(season, mark_time)
        });

        html! {
            <div class="detail-content">
                { self.view_credits(ctx) }

                <div class="column">
                    <div class="toolbar">
                        <h2>
                            if let Some(name) = season.strings.title() {
                                {name}
                            } else {
                                {season.season.long().to_string()}
                            }
                        </h2>

                        <div class="toolbar-toggle">
                            <div class="input-group">
                                if let Some((ref label, _)) = pending_episode {
                                    <a class="button primary" href={format!("#{label}")} title="Jump to pending episode">
                                        <span class="icon chevron-down" />
                                    </a>
                                }

                                <Button icon={if season_expanded { "ellipsis-horizontal" } else { "bars-2" }} title="Season actions" onclick={link.callback(move |_| Msg::ToggleSeasonActionsExpanded(season_number))} />
                            </div>
                        </div>

                        <div class={classes!("toolbar-dropdown", "desktop-input-group", (!season_expanded).then_some("desktop-only"))}>
                            <Button icon="language" title="Season Translations" text="Translations" onclick={link.callback(|_| Msg::OpenSeasonTranslations)} />

                            <Button icon="photo" title="Season Graphics" text="Graphics" onclick={link.callback(|_| Msg::OpenSeasonImageModal)} />

                            if let Some((label, on_remove_next)) = pending_episode {
                                <a class="button primary" href={format!("#{label}")} onclick={toggle_menu} title="Jump to pending episode">
                                    <span class="icon chevron-down" />
                                    <span class="mobile-only">{format!("Jump to next episode {label}")}</span>
                                </a>

                                <Button icon="bookmark" variant={Variant::Danger} title="Remove pending" text={format!("Clear next episode {label}")} onclick={on_remove_next} />
                            } else if let Some((label, episode_id)) = next_unwatched {
                                <MarkTimeMenu class="mobile-has-text" title="Make next episode" prompt={format!("Pending {label} since when?")} preset={next_episode_preset.clone()} on_confirm={link.callback(move |mark_time| Msg::OnWatchNext(episode_id, mark_time))}>
                                    <span class="icon bookmark-slash" />
                                    <span class="mobile-only">{label}</span>
                                </MarkTimeMenu>
                            }

                            if watched_count < total {
                                <MarkTimeMenu class="success mobile-has-text" title="Mark remaining episodes as watched" prompt="When did you watch the remaining episodes?" preset={Some(remaining_preset.clone())} on_confirm={watch_remaining}>
                                    <span class="icon check" />
                                    <span class="mobile-only">{"Remaining"}</span>
                                </MarkTimeMenu>
                            }
                        </div>
                    </div>

                    <TranslatedText strings={season.strings.clone()} />

                    if total > 0 {
                        <h4>{format!("{watched_count} / {total} watched")}</h4>
                    }
                </div>

                if self.episodes.is_empty() && self.selected.is_some() {
                    <div class="text-muted">{"No episodes."}</div>
                }

                <div class="episodes">
                    { for self.episodes.iter().map(|ep| self.view_episode(ctx, ep)) }
                </div>
            </div>
        }
    }

    fn view_episode(&self, ctx: &Context<Self>, episode: &api::Episode) -> Html {
        let link = ctx.link();
        let props = ctx.props();

        let show_id = props.show_id;
        let episode_id = episode.id;

        let watched = self
            .watched_by_episode
            .get(&episode_id)
            .map(Vec::as_slice)
            .unwrap_or_default();

        let history_expanded = self.history_expanded.contains(&episode_id);
        let on_toggle_history =
            (!watched.is_empty()).then(|| link.callback(move |_| Msg::ToggleHistory(episode_id)));

        let menu_open = self.episode_menu == Some(episode_id);
        let on_toggle_menu = link.callback(move |_: MouseEvent| Msg::ToggleEpisodeMenu(episode_id));
        let syncing = self.syncing_episodes.contains(&episode_id);

        let on_remove_next = link.callback(move |_| Msg::OnRemoveNext(episode_id));
        let on_next_episode =
            link.callback(move |mark_time| Msg::OnWatchNext(episode_id, mark_time));

        let on_mark_confirm =
            link.callback(move |mark_time| Msg::MarkWatched(show_id, episode_id, mark_time));

        let preset = episode
            .aired
            .map(|timestamp| TimePreset::at("calendar", "Air date", timestamp));

        // A watched episode that isn't up next is a compact row until expanded.
        let collapsible = !watched.is_empty() && episode.pending.is_none();
        let compact = collapsible && !self.expanded_episodes.contains(&episode_id);

        html! {
            <div class={classes!("episode", (!watched.is_empty()).then_some("watched"), compact.then_some("compact"))} id={episode.code()}>
                <div class="column">
                    <div class="toolbar">
                        <div class="column">
                            <div class="row align-top">
                                if collapsible {
                                    <Button icon={if compact { "chevron-right" } else { "chevron-down" }} class="disclosure" title={if compact { "Show details" } else { "Hide details" }} onclick={link.callback(move |_| Msg::ToggleEpisodeDetails(episode_id))} />
                                }

                                <a class="episode-code" href={format!("#{}", episode.code())}>
                                    <span class="item-inline-xs">
                                        <span class="icon link" />
                                    </span>

                                    <span>{episode.code()}</span>
                                </a>

                                if let Some(name) = episode.strings.title() {
                                    <h4>{name}</h4>
                                }
                            </div>
                        </div>

                        <div class="input-group">
                            <MarkTimeMenu quick=true class="success" icon="check" title="Mark watched" prompt={format!("When did you watch {}?", episode.code())} preset={preset.clone()} on_confirm={on_mark_confirm} />

                            if episode.pending.is_some() {
                                <Button icon="bookmark" variant={Variant::Primary} title="Clear next episode" onclick={on_remove_next} />
                            } else {
                                <MarkTimeMenu icon="bookmark" title="Mark next" prompt={format!("When do you want to queue {}?", episode.code())} preset={preset.clone()} on_confirm={on_next_episode}>
                                    <span class="icon bookmark-slash" />
                                </MarkTimeMenu>
                            }

                            <Button node_ref={if menu_open { self.episode_menu_anchor.clone() } else { NodeRef::default() }} icon="ellipsis-horizontal" class={classes!(menu_open.then_some("selected"))} title="More actions" onclick={on_toggle_menu.clone()} />
                        </div>

                        if menu_open {
                            <ContextMenu anchor={self.episode_menu_anchor.clone()} on_close={link.callback(move |()| Msg::ToggleEpisodeMenu(episode_id))}>
                                <div class="menu-list">
                                    <Button icon="arrow-path" spin={syncing} label="Sync episode" title="Sync episode" onclick={link.callback(move |_| Msg::SyncEpisode(episode_id))} />
                                    <Button icon="language" label="Translations" title="Translations" onclick={link.callback(move |_| Msg::OpenEpisodeTranslations(episode_id))} />
                                    <Button icon="calendar" label="Air dates" title="Air dates" onclick={link.callback(move |_| Msg::OpenEpisodeReleases(episode_id))} />
                                    <Button icon="circle-stack" label="Cache" title="Cache" onclick={link.callback(move |_| Msg::OpenEpisodeCache(episode_id))} />

                                    if let Some(on_toggle) = on_toggle_history {
                                        <Button icon="clock" label={if history_expanded { "Hide watch history" } else { "Watch history" }} title="Watch history" onclick={on_toggle} />
                                    }
                                </div>
                            </ContextMenu>
                        }
                    </div>

                    if !compact {
                        <indicator title="Air date">
                            <span class="item-inline">
                                <span class={classes!("icon", if episode.aired().is_some() { "clock" } else { "exclamation-circle" })} />
                            </span>

                            <content>
                                if let Some(aired) = episode.human_date_time(self.time.clone()) {
                                    <span>{if aired.is_past() { "Aired" } else { "Airs" }}</span>
                                    {aired.lower().view()}
                                } else {
                                    <span class="text-muted">{"No air date"}</span>
                                }
                            </content>
                        </indicator>
                    }

                    <indicator title="Watch status">
                        if episode.pending.is_some() {
                            <span class="item-inline" title="Next episode">
                                <span class="icon primary exclamation-circle" />
                            </span>
                        } else if !watched.is_empty() {
                            <span class="item-inline" title="Watched">
                                <span class="icon primary check-circle" />
                            </span>
                        } else {
                            <span class="item-inline" title="Never watched">
                                <span class="icon secondary x-circle" />
                            </span>
                        }

                        <content>
                            if let Some(ts) = episode.pending {
                                <span>{"Episode scheduled for"}</span>
                                {ts.human_date_time(self.time.clone()).lower().view()}
                            } else {
                                {match watched {
                                    [] => html!(<span class="special">{"Never watched"}</span>),
                                    [w] => html! {
                                        <>
                                            <span>{"Watched once"}</span>
                                            {w.watched.timestamp.human_date_time(self.time.clone()).lower().view()}
                                        </>
                                    },
                                    [w, ..] => html! {
                                        <>
                                            {format!("Watched {} times, first", watched.len())}
                                            {w.watched.timestamp.human_date_time(self.time.clone()).view()}
                                        </>
                                    }
                                }}
                            }
                        </content>
                    </indicator>
                </div>

                if !compact {
                    <div class="desktop-row mobile-column align-top">
                        <Image class="screenshot" src={episode.screenshot.clone()} />

                        <div class="column desktop-fill">
                            <TranslatedText strings={episode.strings.clone()} />
                        </div>
                    </div>
                }

                if history_expanded {
                    <Modal icon="clock" title={format!("Watch history for {}", episode.code())} on_close={link.callback(move |_| Msg::ToggleHistory(episode_id))}>
                        <div key="history" class="column fill">
                            { for watched.iter().map(|w| {
                                let wid = w.watched.id;
                                let kind = api::WatchedKind::Episode { show: show_id, episode: episode_id };

                                html! {
                                    <div class="row-split">
                                        {w.watched.timestamp.human_date_time(self.time.clone()).view()}

                                        <div class="row">
                                            <div class="input-group" ref={w.context_anchor.clone()}>
                                                <Button icon="pencil-square" label="Move" title="Move to different episode" onclick={link.callback(move |_| Msg::FixWatched(wid))} />

                                                if self.fixing_watched == Some(wid) {
                                                    <ContextMenu prompt="Where do you want to move watch at" label={w.watched.timestamp.human_date_time(self.time.clone())} anchor={w.context_anchor.clone()} on_close={link.callback(|_| Msg::CancelFixWatched)}>
                                                        <EpisodePicker
                                                            show_id={show_id}
                                                            season={episode.season}
                                                            episode={episode.episode}
                                                            on_confirm={link.callback(move |(season, ep)| Msg::MoveWatched(wid, season, ep))}
                                                            on_cancel={link.callback(|_| Msg::CancelFixWatched)}
                                                        />
                                                    </ContextMenu>
                                                }

                                                <Button icon="trash" variant={Variant::Danger} title="Remove" text="Remove" onclick={link.callback(move |_| Msg::ConfirmRemoveWatch(wid))} />

                                                if self.confirm_remove_watch == Some(wid) {
                                                    <ContextMenu prompt="Remove watch at" label={w.watched.timestamp.human_date_time(self.time.clone())} anchor={w.context_anchor.clone()} on_close={link.callback(|_| Msg::CancelRemoveWatch)}>
                                                        <ConfirmDanger
                                                            on_confirm={link.callback(move |_| Msg::RemoveWatched(wid, kind))}
                                                            on_cancel={link.callback(|_| Msg::CancelRemoveWatch)}
                                                        />
                                                    </ContextMenu>
                                                }
                                            </div>
                                        </div>
                                    </div>
                                }
                            }) }
                        </div>
                    </Modal>
                }

                if self.episode_releases_modal == Some(episode_id) {
                    <ReleaseModal
                        target={ReleaseTarget::Episode(episode_id)}
                        title={format!("Air dates for {}", episode.code())}
                        on_close={link.callback(|_| Msg::CloseEpisodeReleases)}
                    />
                }

                if self.episode_cache_modal == Some(episode_id) {
                    <EpisodeCacheModal
                        episode_id={episode_id}
                        title={format!("Cache for {}", episode.code())}
                        on_close={link.callback(|_| Msg::CloseEpisodeCache)}
                    />
                }
            </div>
        }
    }

    fn view_orphaned(&self, ctx: &Context<Self>) -> Html {
        let props = ctx.props();

        if self.orphaned.is_empty() {
            return html! {};
        }

        let link = ctx.link();
        let show_id = props.show_id;

        html! {
            <div class="detail-content">
                <div class="table">
                    { for self.orphaned.iter().map(|w| {
                        let id = w.watched.id;
                        let kind = api::WatchedKind::Episode { show: show_id, episode: api::EpisodeId::new(0) };

                        html! {
                            <div class="row-split">
                                <div class="row">
                                    <span class="text-muted">{w.watched.code()}</span>
                                    <span>{w.watched.timestamp.human_date_time(self.time.clone())}</span>
                                </div>

                                <div ref={w.context_anchor.clone()} class="input-group">
                                    <Button icon="pencil-square" title="Move to episode" onclick={link.callback(move |_| Msg::FixWatched(id))} />

                                    <Button icon="trash" variant={Variant::Danger} title="Remove" text="Remove" onclick={link.callback(move |_| Msg::ConfirmRemoveWatch(id))} />
                                </div>

                                if self.fixing_watched == Some(id) {
                                    <ContextMenu anchor={w.context_anchor.clone()} prompt="Where do you want to move orphaned watch at" label={w.watched.timestamp.human_date_time(self.time.clone())} on_close={ctx.link().callback(|_| Msg::CancelFixWatched)}>
                                        <EpisodePicker
                                            {show_id}
                                            season={w.watched.season}
                                            episode={w.watched.episode}
                                            timestamp={w.watched.timestamp}
                                            on_confirm={link.callback(move |(season, ep)| Msg::MoveWatched(id, season, ep))}
                                            on_cancel={link.callback(|_| Msg::CancelFixWatched)}
                                        />
                                    </ContextMenu>
                                }

                                if self.confirm_remove_watch == Some(id) {
                                    <ContextMenu anchor={w.context_anchor.clone()} prompt="Remove orphaned watch at" label={w.watched.timestamp.human_date_time(self.time.clone())} on_close={ctx.link().callback(|_| Msg::CancelRemoveWatch)}>
                                        <ConfirmDanger
                                            on_confirm={link.callback(move |_| Msg::RemoveWatched(id, kind))}
                                            on_cancel={link.callback(|_| Msg::CancelRemoveWatch)}
                                        />
                                    </ContextMenu>
                                }
                            </div>
                        }
                    }) }
                </div>
            </div>
        }
    }
}
