use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};
use std::rc::Rc;

use musli_web::web03::prelude::*;
use wasm_bindgen::JsCast as _;
use yew::prelude::*;

use api::{TimeInfo, Timed};

use super::detail::{Graphics, ImageMsg, ImageUpdate, RemoteMsg, RemoteUpdate, Remotes};
use crate::SetupChannel;
use crate::active_tasks::{ActiveTasks, SyncTarget};
use crate::background::Background;
use crate::error::{CustomContext, Error, Message};
use crate::router::{MediaQuery, Route, Router, ShowDetailQuery};
use crate::ui::{
    AlsoKnownAs, Button, ConfirmDanger, ContextMenu, DetailHero, DetailSkeleton, EpisodeCacheModal,
    EpisodePicker, Image, ImageGallery, ImageItem, MarkTimeMenu, MediaSettingsModal, Modal,
    NumberingEditor, OutlineControl, OutlineEntry, OutlineHandle, ReleaseModal, ReleaseTarget,
    RemoteEditor, RemoteSourceKind, SettingsTarget, SyncButton, TimePreset, Tracked,
    TranslatedText, TranslationsModal, Variant,
};
use crate::ui::{CastModal, cast_card};

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
    graphics: Graphics,
    remotes: Remotes,
    season_graphics: BTreeMap<api::ImageKind, Vec<ImageItem>>,
    seasons: Vec<api::Season>,
    credits: Rc<Vec<api::Credit>>,
    cast_modal: bool,
    selected: Option<usize>,
    /// The phone season chips, and the season they were last scrolled to.
    season_chips: NodeRef,
    chips_scrolled: Option<usize>,
    episodes: Vec<api::Episode>,
    pending_episode: Option<(api::Code, api::EpisodeId)>,
    next_unwatched: Option<(api::Code, api::EpisodeId)>,
    confirm_remove: bool,
    remove_anchor: NodeRef,
    /// Queued and running syncs, for the episode menu's sync spinner.
    active_tasks: ActiveTasks,
    _active_tasks_handle: ContextHandle<ActiveTasks>,
    /// Episodes picked for a bulk action, from the season shown only.
    picked: BTreeSet<api::EpisodeId>,
    /// The episode picked last, where a shift-click range starts.
    pick_anchor: Option<api::EpisodeId>,
    /// Clears the picked episodes on Escape while any are picked.
    _pick_escape: Option<gloo::events::EventListener>,
    /// Bulk requests in flight, and how many have yet to answer.
    _bulk_reqs: Vec<ws::Request>,
    bulk_pending: usize,
    actions_expanded: bool,
    /// The episode whose overflow menu is open, anchored to its trigger.
    episode_menu: Option<api::EpisodeId>,
    episode_menu_anchor: NodeRef,
    season_actions_expanded: HashSet<api::SeasonNumber>,
    confirm_remove_watch: Option<api::WatchedId>,
    watched_by_episode: HashMap<api::EpisodeId, Vec<WatchedState>>,
    history_expanded: HashSet<api::EpisodeId>,
    orphaned: Vec<OrphanedWatchedState>,
    fixing_watched: Option<api::WatchedId>,
    image_modal: bool,
    season_image_modal: bool,
    settings_modal: bool,
    remote_editor: bool,
    /// The numbering editor is open, on manual ranges when `true`.
    numbering_editor: Option<bool>,
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
    _season_images_req: ws::Request,
    _select_season_image_req: ws::Request,
    _clear_season_image_req: ws::Request,
    _config_req: ws::Request,
    _orphaned_req: ws::Request,
    _move_req: ws::Request,
}

pub(crate) enum Msg {
    Channel(Result<ws::Channel, ws::Error>),
    AppBroadcast(Result<ws::Packet<api::AppBroadcast>, ws::Error>),
    SetTime(TimeInfo),
    ActiveTasks(ActiveTasks),
    Image(ImageMsg),
    Remote(RemoteMsg),
    Load(LoadMsg),
    Watch(WatchMsg),
    Pick(PickMsg),
    Action(ActionMsg),
    SeasonImage(SeasonImageMsg),
    Ui(UiMsg),
}

/// Responses loading the page's data.
pub(crate) enum LoadMsg {
    Show(Result<ws::Packet<api::GetShow>, ws::Error>),
    Seasons(Result<ws::Packet<api::ListSeasons>, ws::Error>),
    Credits(Result<ws::Packet<api::ListCredits>, ws::Error>),
    Episodes(Result<ws::Packet<api::ListEpisodes>, ws::Error>),
    Watched(Result<ws::Packet<api::ListEpisodesWatched>, ws::Error>),
    Config(Result<ws::Packet<api::GetPreferences>, ws::Error>),
    Orphaned(Result<ws::Packet<api::ListOrphanedWatched>, ws::Error>),
}

/// Marking episodes watched, fixing orphaned watches, and the pending (watch next) episode.
pub(crate) enum WatchMsg {
    MarkWatched(api::ShowId, api::EpisodeId, api::MarkTime),
    MarkWatchedDone(Result<ws::Packet<api::MarkWatched>, ws::Error>),
    RemoveWatched(api::WatchedId, api::WatchedKind),
    RemoveWatchedDone(Result<ws::Packet<api::RemoveWatched>, ws::Error>),
    ConfirmRemoveWatch(api::WatchedId),
    CancelRemoveWatch,
    WatchRemaining(api::SeasonNumber, api::MarkTime),
    WatchRemainingDone(Result<ws::Packet<api::MarkWatched>, ws::Error>),
    OnWatchNext(api::EpisodeId, api::MarkTime),
    AddPendingDone(Result<ws::Packet<api::AddPending>, ws::Error>),
    OnRemoveNext(api::EpisodeId),
    RemovePendingDone(Result<ws::Packet<api::RemovePending>, ws::Error>),
    FixWatched(api::WatchedId),
    CancelFixWatched,
    MoveWatched(api::WatchedId, api::SeasonNumber, u32),
    MoveWatchedDone(Result<ws::Packet<api::MoveWatchedEpisode>, ws::Error>),
}

/// Picking episodes and acting on them in bulk.
pub(crate) enum PickMsg {
    /// Pick or unpick an episode; with shift, pick the range from the last one.
    TogglePick(api::EpisodeId, bool),
    ClearPicked,
    BulkMark(api::MarkTime),
    BulkMarkDone(Result<ws::Packet<api::MarkWatched>, ws::Error>),
    BulkSync,
    BulkSyncDone(Result<ws::Packet<api::SyncEpisode>, ws::Error>),
}

/// Tracking, syncing and removing the show.
pub(crate) enum ActionMsg {
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
}

/// The selected season's graphics.
pub(crate) enum SeasonImageMsg {
    OpenSeasonImageModal,
    CloseSeasonImageModal,
    SeasonImagesLoaded(Result<ws::Packet<api::GetSeasonImages>, ws::Error>),
    SelectSeasonImage(api::ImageKind, api::ImageId),
    SelectSeasonImageDone(Result<ws::Packet<api::SelectImage>, ws::Error>),
    ClearSelectedSeasonImage(api::ImageKind),
    ClearSelectedSeasonImageDone(Result<ws::Packet<api::ClearSelectedImage>, ws::Error>),
}

/// Navigation, modals and expandable sections.
pub(crate) enum UiMsg {
    OpenCastModal,
    CloseCastModal,
    SelectSeason(api::SeasonNumber),
    ToggleHistory(api::EpisodeId),
    OpenImageModal,
    CloseImageModal,
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
    OpenNumberingEditor(bool),
    CloseNumberingEditor,
    ToggleActionsExpanded,
    ToggleEpisodeMenu(api::EpisodeId),
    ToggleSeasonActionsExpanded(api::SeasonNumber),
    ToggleOrphaned,
}

impl From<ImageMsg> for Msg {
    #[inline]
    fn from(msg: ImageMsg) -> Self {
        Msg::Image(msg)
    }
}

impl From<RemoteMsg> for Msg {
    #[inline]
    fn from(msg: RemoteMsg) -> Self {
        Msg::Remote(msg)
    }
}

impl From<LoadMsg> for Msg {
    #[inline]
    fn from(msg: LoadMsg) -> Self {
        Msg::Load(msg)
    }
}

impl From<WatchMsg> for Msg {
    #[inline]
    fn from(msg: WatchMsg) -> Self {
        Msg::Watch(msg)
    }
}

impl From<PickMsg> for Msg {
    #[inline]
    fn from(msg: PickMsg) -> Self {
        Msg::Pick(msg)
    }
}

impl From<ActionMsg> for Msg {
    #[inline]
    fn from(msg: ActionMsg) -> Self {
        Msg::Action(msg)
    }
}

impl From<SeasonImageMsg> for Msg {
    #[inline]
    fn from(msg: SeasonImageMsg) -> Self {
        Msg::SeasonImage(msg)
    }
}

impl From<UiMsg> for Msg {
    #[inline]
    fn from(msg: UiMsg) -> Self {
        Msg::Ui(msg)
    }
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

        let (active_tasks, _active_tasks_handle) = ctx
            .link()
            .context::<ActiveTasks>(ctx.link().callback(Msg::ActiveTasks))
            .expect("Expected active tasks in context");

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
            graphics: Graphics::default(),
            remotes: Remotes::default(),
            season_graphics: BTreeMap::new(),
            seasons: Vec::new(),
            credits: Rc::default(),
            cast_modal: false,
            selected: None,
            season_chips: NodeRef::default(),
            chips_scrolled: None,
            episodes: Vec::new(),
            pending_episode: None,
            next_unwatched: None,
            confirm_remove: false,
            remove_anchor: NodeRef::default(),
            active_tasks,
            _active_tasks_handle,
            picked: BTreeSet::new(),
            pick_anchor: None,
            _pick_escape: None,
            _bulk_reqs: Vec::new(),
            bulk_pending: 0,
            actions_expanded: false,
            episode_menu: None,
            episode_menu_anchor: NodeRef::default(),
            season_actions_expanded: HashSet::new(),
            confirm_remove_watch: None,
            watched_by_episode: HashMap::new(),
            history_expanded: HashSet::new(),
            orphaned: Vec::new(),
            fixing_watched: None,
            image_modal: false,
            season_image_modal: false,
            settings_modal: false,
            remote_editor: false,
            numbering_editor: None,
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
            _season_images_req: ws::Request::default(),
            _select_season_image_req: ws::Request::default(),
            _clear_season_image_req: ws::Request::default(),
            _config_req: ws::Request::default(),
            _orphaned_req: ws::Request::default(),
            _move_req: ws::Request::default(),
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
        // Bring the current season's chip into view sideways, never moving the
        // page itself.
        if self.chips_scrolled != self.selected
            && let Some(chips) = self.season_chips.cast::<web_sys::HtmlElement>()
            && let Ok(Some(chip)) = chips.query_selector(".selected")
        {
            self.chips_scrolled = self.selected;
            let (strip, chip) = (
                chips.get_bounding_client_rect(),
                chip.get_bounding_client_rect(),
            );
            let offset = chip.left() - strip.left() - (strip.width() - chip.width()) / 2.0;
            chips.set_scroll_left(chips.scroll_left() + offset as i32);
        }

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
                        <span class="icon exclamation-triangle" aria-hidden="true" />
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
                >
                    <AlsoKnownAs names={show.alt_names.clone()} />
                </DetailHero>

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
                        <Button icon={if self.actions_expanded { "ellipsis-horizontal" } else { "bars-2" }} title="Actions" expanded={Some(self.actions_expanded)} onclick={link.callback(|_| UiMsg::ToggleActionsExpanded)} />
                    </div>

                    <div class={classes!("toolbar-dropdown", "desktop-input-group", (!self.actions_expanded).then_some("desktop-only"))}>
                        if !self.orphaned.is_empty() || props.orphaned {
                            <Button
                                icon={if props.orphaned { "ellipsis-horizontal" } else { "exclamation-triangle" }}
                                variant={Variant::Danger}
                                title={if props.orphaned { "Showing orphaned watches" } else { "Hiding orphaned watches" }}
                                pressed={Some(props.orphaned)}
                                text="Orphaned watches"
                                onclick={link.callback(|_| UiMsg::ToggleOrphaned)}
                            />
                        }

                        <Tracked kind="show" tracked={show.tracked} ontoggle={link.callback(ActionMsg::SetTracked)} />

                        if !show.remotes.is_empty() {
                            <SyncButton target={SyncTarget::Show(show.id)} onclick={link.callback(|_| ActionMsg::SyncShow)} text="Sync" />
                        }

                        <Button icon="language" title="Translations" text="Translations" onclick={link.callback(|_| UiMsg::OpenShowTranslations)} />

                        <Button icon="cog-6-tooth" title="Settings" text="Settings" onclick={link.callback(|_| UiMsg::OpenSettingsModal)} />

                        if crate::is_admin(ctx) {
                            <Button node_ref={self.remove_anchor.clone()} icon="trash" variant={Variant::Danger} class="detached" title="Remove show" text="Remove" expanded={Some(self.confirm_remove)} haspopup="dialog" onclick={link.callback(|_| ActionMsg::ConfirmRemove)} />

                            if self.confirm_remove {
                                <ContextMenu prompt="Remove show" label={show.strings.title().map(str::to_owned)} anchor={self.remove_anchor.clone()} on_close={ctx.link().callback(|_| ActionMsg::CancelRemove)}>
                                    <ConfirmDanger on_confirm={link.callback(|_| ActionMsg::RemoveShow)} on_cancel={link.callback(|_| ActionMsg::CancelRemove)} />
                                </ContextMenu>
                            }
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
                        on_edit_graphics={link.callback(|_| UiMsg::OpenImageModal)}
                        on_edit_remotes={link.callback(|_| UiMsg::OpenRemoteEditor)}
                        on_edit_numbering={link.callback(UiMsg::OpenNumberingEditor)}
                        on_close={link.callback(|_| UiMsg::CloseSettingsModal)}
                    />
                }

                if let Some(manual) = self.numbering_editor {
                    <NumberingEditor
                        show_id={show.id}
                        numbering={show.numbering.clone()}
                        {manual}
                        on_close={link.callback(|_| UiMsg::CloseNumberingEditor)}
                    />
                }

                if self.remote_editor {
                    <RemoteEditor
                        title={show.strings.title().unwrap_or("Untitled Show").to_owned()}
                        kind={RemoteSourceKind::Show}
                        remotes={show.remotes.clone()}
                        on_add={link.callback(|(slug, remote)| RemoteMsg::AddRemote(slug, remote))}
                        on_edit={link.callback(|(id, slug, remote)| RemoteMsg::EditRemote(id, slug, remote))}
                        on_remove={link.callback(RemoteMsg::RemoveRemote)}
                        on_purge_cache={link.callback(RemoteMsg::PurgeRemoteCache)}
                        on_set_enabled={link.callback(|(id, enabled)| RemoteMsg::SetRemoteEnabled(id, enabled))}
                        on_reorder={link.callback(RemoteMsg::ReorderRemotes)}
                        on_set_sync_kinds={link.callback(|(id, kinds)| RemoteMsg::SetRemoteSyncKinds(id, kinds))}
                        global_sync_kinds={self.global_sync_kinds.clone()}
                        on_close={link.callback(|_| UiMsg::CloseRemoteEditor)}
                    />
                }

                if self.show_translations_modal {
                    <TranslationsModal
                        target={api::TranslationTarget::Show(show.id)}
                        on_close={link.callback(|_| UiMsg::CloseShowTranslations)}
                    />
                }

                if self.season_translations_modal {
                    if let Some(season) = self.selected() {
                        <TranslationsModal
                            target={api::TranslationTarget::Season(season.id)}
                            on_close={link.callback(|_| UiMsg::CloseSeasonTranslations)}
                        />
                    }
                }

                if let Some(episode_id) = self.episode_translations {
                    <TranslationsModal
                        target={api::TranslationTarget::Episode(episode_id)}
                        on_close={link.callback(|_| UiMsg::CloseEpisodeTranslations)}
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
            self.episodes.clear();
            self.pending_episode = None;
            self.next_unwatched = None;
            self.confirm_remove = false;
            self.confirm_remove_watch = None;
            self.watched_by_episode.clear();
            self.clear_picked();

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
            self.clear_picked();

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
            Msg::AppBroadcast(packet) => self.on_broadcast(ctx, packet),
            Msg::SetTime(time) => {
                self.time = time;
                Ok(true)
            }
            Msg::ActiveTasks(tasks) => {
                let render = tasks.changed(|target| {
                    matches!(target, SyncTarget::Episode(id) if self.episode_menu == Some(id))
                });
                self.active_tasks = tasks;
                Ok(render)
            }
            Msg::Image(msg) => self.update_image(ctx, msg),
            Msg::Remote(msg) => self.update_remote(ctx, msg),
            Msg::Load(msg) => self.update_load(ctx, msg),
            Msg::Watch(msg) => self.update_watch(ctx, msg),
            Msg::Pick(msg) => self.update_pick(ctx, msg),
            Msg::Action(msg) => self.update_action(ctx, msg),
            Msg::SeasonImage(msg) => self.update_season_image(ctx, msg),
            Msg::Ui(msg) => self.update_ui(ctx, msg),
        }
    }

    fn on_broadcast(
        &mut self,
        ctx: &Context<Self>,
        packet: Result<ws::Packet<api::AppBroadcast>, ws::Error>,
    ) -> Result<bool, Error> {
        let props = ctx.props();

        let event = packet?.decode_event()?;
        if event.channel == self.channel.id() {
            return Ok(false);
        }
        match &event.kind {
            api::AppEventKind::ShowChanged { show } | api::AppEventKind::ShowCreated { show }
                if show.id == props.show_id =>
            {
                self.background
                    .background(show.backdrop.as_ref().map(|i| i.proxy_url()));

                // Other numberings and season names follow the numbering.
                if let ShowState::Loaded(old) = &self.show
                    && old.numbering != show.numbering
                    && self.channel.id() != ws::ChannelId::NONE
                {
                    self.load_seasons(ctx);
                    self.load_episodes(ctx);
                }

                self.show = ShowState::Loaded(Box::new(show.clone()));
                self.update_graphics();
                Ok(true)
            }
            api::AppEventKind::SeasonsChanged { show_id, .. } if *show_id == props.show_id => {
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
            api::AppEventKind::EpisodesChanged { show_id, season } if *show_id == props.show_id => {
                if props.season == *season {
                    self.load_episodes(ctx);
                }

                self.load_orphaned(ctx);
                Ok(false)
            }
            api::AppEventKind::PendingChanged | api::AppEventKind::PendingEntryChanged { .. } => {
                self.load_episodes(ctx);
                self.load_orphaned(ctx);
                Ok(false)
            }
            api::AppEventKind::TaskCompleted { task } => {
                if matches!(&task.kind, api::TaskKind::SyncShow { show_id, .. } if *show_id == props.show_id)
                {
                    self.load_episodes(ctx);
                    self.load_show(ctx);
                    self.load_seasons(ctx);
                    self.load_orphaned(ctx);
                }

                Ok(false)
            }
            api::AppEventKind::WatchedChanged { event: kind } => {
                let relevant = match kind {
                    api::WatchedEvent::Episode { show, .. } => *show == props.show_id,
                    api::WatchedEvent::RemainingSeason { show, .. } => *show == props.show_id,
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

    fn update_image(&mut self, ctx: &Context<Self>, msg: ImageMsg) -> Result<bool, Error> {
        let owner = api::ImageOwner::Show(ctx.props().show_id);

        match self
            .graphics
            .update(ctx.link(), &self.channel, owner, msg)?
        {
            ImageUpdate::Render(render) => Ok(render),
            ImageUpdate::Reload => {
                self.load_show(ctx);
                Ok(true)
            }
            ImageUpdate::Cleared => {
                self.image_modal = false;
                self.load_show(ctx);
                Ok(true)
            }
        }
    }

    fn update_remote(&mut self, ctx: &Context<Self>, msg: RemoteMsg) -> Result<bool, Error> {
        let remotes = match &mut self.show {
            ShowState::Loaded(show) => Some(&mut show.remotes),
            _ => None,
        };

        match self
            .remotes
            .update(ctx.link(), &self.channel, ctx.props().show_id, remotes, msg)?
        {
            RemoteUpdate::Render(render) => Ok(render),
            RemoteUpdate::Reload => {
                self.load_show(ctx);
                Ok(false)
            }
        }
    }

    fn update_load(&mut self, ctx: &Context<Self>, msg: LoadMsg) -> Result<bool, Error> {
        let props = ctx.props();

        match msg {
            LoadMsg::Show(result) => {
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
            LoadMsg::Seasons(result) => {
                self.seasons = result
                    .context(Message::LoadingSeasons)?
                    .decode()
                    .context(Message::LoadingSeasons)?
                    .seasons;

                // Specials are extras: list them after the numbered seasons.
                self.seasons
                    .sort_by_key(|s| matches!(s.season, api::SeasonNumber::Specials));

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
            LoadMsg::Credits(result) => {
                self.credits = Rc::new(
                    result
                        .context(Message::LoadingCredits)?
                        .decode()
                        .context(Message::LoadingCredits)?
                        .credits,
                );

                Ok(true)
            }
            LoadMsg::Episodes(result) => {
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
            LoadMsg::Watched(result) => {
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
            LoadMsg::Config(result) => {
                self.global_sync_kinds = result
                    .context(Message::LoadingConfig)?
                    .decode()
                    .context(Message::LoadingConfig)?
                    .site
                    .sync_kinds;
                Ok(true)
            }
            LoadMsg::Orphaned(result) => {
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
        }
    }

    fn update_watch(&mut self, ctx: &Context<Self>, msg: WatchMsg) -> Result<bool, Error> {
        let props = ctx.props();

        match msg {
            WatchMsg::MarkWatched(show, episode, mark_time) => {
                self.episode_menu = None;

                if self.channel.id() != ws::ChannelId::NONE {
                    self._mark_req = self
                        .channel
                        .request()
                        .body(api::MarkWatchedRequest {
                            kind: api::WatchedKind::Episode { show, episode },
                            mark_time,
                        })
                        .on_packet(ctx.link().callback(WatchMsg::MarkWatchedDone))
                        .send();
                }

                Ok(true)
            }
            WatchMsg::MarkWatchedDone(result) => {
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
            WatchMsg::RemoveWatched(id, kind) => {
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
                        .on_packet(ctx.link().callback(WatchMsg::RemoveWatchedDone))
                        .send();
                }

                Ok(false)
            }
            WatchMsg::RemoveWatchedDone(result) => {
                result.context(Message::RemovingWatched)?;
                self.confirm_remove_watch = None;

                // Refresh season counts so the progress bars reflect the change.
                self.load_episodes(ctx);
                self.load_seasons(ctx);
                self.load_history(ctx);
                self.load_orphaned(ctx);
                Ok(false)
            }
            WatchMsg::ConfirmRemoveWatch(episode_id) => {
                self.confirm_remove_watch = Some(episode_id);
                Ok(true)
            }
            WatchMsg::CancelRemoveWatch => {
                self.confirm_remove_watch = None;
                Ok(true)
            }
            WatchMsg::WatchRemaining(season, mark_time) => {
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
                        .on_packet(ctx.link().callback(WatchMsg::WatchRemainingDone))
                        .send();
                }

                Ok(false)
            }
            WatchMsg::WatchRemainingDone(result) => {
                result.context(Message::MarkingWatched)?;

                // Refresh season counts so the progress bars reflect the marks.
                self.load_episodes(ctx);
                self.load_seasons(ctx);
                self.load_history(ctx);
                self.load_orphaned(ctx);
                Ok(false)
            }
            WatchMsg::OnWatchNext(episode_id, mark_time) => {
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
                        .on_packet(ctx.link().callback(WatchMsg::AddPendingDone))
                        .send();
                }

                Ok(true)
            }
            WatchMsg::AddPendingDone(result) => {
                result
                    .context(Message::AddingPending)?
                    .decode()
                    .context(Message::AddingPending)?;
                self.load_episodes(ctx);
                self.load_orphaned(ctx);
                Ok(false)
            }
            WatchMsg::OnRemoveNext(episode_id) => {
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
                        .on_packet(ctx.link().callback(WatchMsg::RemovePendingDone))
                        .send();
                }

                Ok(false)
            }
            WatchMsg::RemovePendingDone(result) => {
                result
                    .context(Message::RemovingPending)?
                    .decode()
                    .context(Message::RemovingPending)?;
                self.load_episodes(ctx);
                Ok(false)
            }
            WatchMsg::FixWatched(id) => {
                self.fixing_watched = Some(id);
                self.confirm_remove_watch = None;
                Ok(true)
            }
            WatchMsg::CancelFixWatched => {
                self.fixing_watched = None;
                Ok(true)
            }
            WatchMsg::MoveWatched(id, season, episode) => {
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
                        .on_packet(ctx.link().callback(WatchMsg::MoveWatchedDone))
                        .send();
                }

                Ok(false)
            }
            WatchMsg::MoveWatchedDone(result) => {
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
        }
    }

    fn update_pick(&mut self, ctx: &Context<Self>, msg: PickMsg) -> Result<bool, Error> {
        let props = ctx.props();

        match msg {
            PickMsg::TogglePick(episode_id, range) => {
                let position = |id| self.episodes.iter().position(|e| e.id == id);

                match (
                    range,
                    self.pick_anchor.and_then(position),
                    position(episode_id),
                ) {
                    (true, Some(from), Some(to)) => {
                        let (from, to) = (from.min(to), from.max(to));
                        self.picked
                            .extend(self.episodes[from..=to].iter().map(|e| e.id));
                    }
                    _ => {
                        if !self.picked.remove(&episode_id) {
                            self.picked.insert(episode_id);
                        }
                    }
                }

                self.pick_anchor = Some(episode_id);

                if self.picked.is_empty() {
                    self.clear_picked();
                } else if self._pick_escape.is_none() {
                    let clear = ctx.link().callback(|()| PickMsg::ClearPicked);

                    self._pick_escape = web_sys::window().map(|window| {
                        gloo::events::EventListener::new(&window, "keydown", move |e| {
                            if let Some(e) = e.dyn_ref::<web_sys::KeyboardEvent>()
                                && e.key() == "Escape"
                            {
                                clear.emit(());
                            }
                        })
                    });
                }

                Ok(true)
            }
            PickMsg::ClearPicked => {
                self.clear_picked();
                Ok(true)
            }
            PickMsg::BulkMark(mark_time) => {
                let show = props.show_id;

                if self.channel.id() != ws::ChannelId::NONE {
                    self._bulk_reqs = self
                        .picked
                        .iter()
                        .map(|&episode| {
                            self.channel
                                .request()
                                .body(api::MarkWatchedRequest {
                                    kind: api::WatchedKind::Episode { show, episode },
                                    mark_time,
                                })
                                .on_packet(ctx.link().callback(PickMsg::BulkMarkDone))
                                .send()
                        })
                        .collect();
                    self.bulk_pending = self._bulk_reqs.len();
                }

                self.clear_picked();
                Ok(true)
            }
            PickMsg::BulkMarkDone(result) => {
                self.bulk_pending = self.bulk_pending.saturating_sub(1);
                result
                    .context(Message::MarkingWatched)?
                    .decode()
                    .context(Message::MarkingWatched)?;

                // Refresh once every mark has answered.
                if self.bulk_pending == 0 {
                    self.load_episodes(ctx);
                    self.load_seasons(ctx);
                    self.load_history(ctx);
                    self.load_orphaned(ctx);
                }

                Ok(false)
            }
            PickMsg::BulkSync => {
                let show_id = props.show_id;

                if self.channel.id() != ws::ChannelId::NONE {
                    self._bulk_reqs = self
                        .picked
                        .iter()
                        .map(|&episode_id| {
                            self.channel
                                .request()
                                .body(api::SyncEpisodeRequest {
                                    show_id,
                                    episode_id,
                                })
                                .on_packet(ctx.link().callback(PickMsg::BulkSyncDone))
                                .send()
                        })
                        .collect();
                }

                self.clear_picked();
                Ok(true)
            }
            PickMsg::BulkSyncDone(result) => {
                result.context(Message::SyncingEpisode)?;
                Ok(false)
            }
        }
    }

    fn update_action(&mut self, ctx: &Context<Self>, msg: ActionMsg) -> Result<bool, Error> {
        let props = ctx.props();

        match msg {
            ActionMsg::SetTracked(tracked) => {
                self.actions_expanded = false;

                let id = props.show_id;

                if self.channel.id() != ws::ChannelId::NONE {
                    self._untrack_req = self
                        .channel
                        .request()
                        .body(api::UntrackShowRequest { id, tracked })
                        .on_packet(
                            ctx.link()
                                .callback(move |r| ActionMsg::SetTrackedDone(tracked, r)),
                        )
                        .send();
                }

                Ok(false)
            }
            ActionMsg::SetTrackedDone(tracked, result) => {
                result.context(Message::UntrackingShow)?;
                if let ShowState::Loaded(show) = &mut self.show {
                    show.tracked = tracked;
                }
                Ok(true)
            }
            ActionMsg::ConfirmRemove => {
                self.confirm_remove = true;
                Ok(true)
            }
            ActionMsg::CancelRemove => {
                self.confirm_remove = false;
                Ok(true)
            }
            ActionMsg::RemoveShow => {
                let id = props.show_id;

                if self.channel.id() != ws::ChannelId::NONE {
                    self._remove_req = self
                        .channel
                        .request()
                        .body(api::RemoveShowRequest { id })
                        .on_packet(ctx.link().callback(ActionMsg::RemoveDone))
                        .send();
                }

                Ok(false)
            }
            ActionMsg::RemoveDone(result) => {
                result.context(Message::RemovingShow)?;
                self.router.push(Route::Media(MediaQuery::default()));
                Ok(false)
            }
            ActionMsg::SyncShow => {
                let id = props.show_id;

                if self.channel.id() != ws::ChannelId::NONE {
                    self._sync_req = self
                        .channel
                        .request()
                        .body(api::SyncShowRequest { id })
                        .on_packet(ctx.link().callback(ActionMsg::SyncDone))
                        .send();
                }

                Ok(true)
            }
            ActionMsg::SyncDone(result) => {
                result.context(Message::SyncingShow)?;
                Ok(false)
            }
            ActionMsg::SyncEpisode(episode_id) => {
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
                        .on_packet(ctx.link().callback(ActionMsg::SyncEpisodeDone))
                        .send();
                }

                Ok(true)
            }
            ActionMsg::SyncEpisodeDone(result) => {
                result.context(Message::SyncingEpisode)?;
                Ok(false)
            }
        }
    }

    fn update_season_image(
        &mut self,
        ctx: &Context<Self>,
        msg: SeasonImageMsg,
    ) -> Result<bool, Error> {
        match msg {
            SeasonImageMsg::OpenSeasonImageModal => {
                if let Some(id) = self.selected().map(|s| s.id) {
                    self.season_image_modal = true;
                    self.load_season_images(ctx, id);
                }

                Ok(true)
            }
            SeasonImageMsg::CloseSeasonImageModal => {
                self.season_image_modal = false;
                Ok(true)
            }
            SeasonImageMsg::SeasonImagesLoaded(result) => {
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
                        });
                }

                Ok(true)
            }
            SeasonImageMsg::SelectSeasonImage(kind, id) => {
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
                        .on_packet(ctx.link().callback(SeasonImageMsg::SelectSeasonImageDone))
                        .send();
                }

                Ok(true)
            }
            SeasonImageMsg::SelectSeasonImageDone(result) => {
                result.context(Message::SelectingImage)?;
                Ok(true)
            }
            SeasonImageMsg::ClearSelectedSeasonImage(kind) => {
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
                        .on_packet(
                            ctx.link()
                                .callback(SeasonImageMsg::ClearSelectedSeasonImageDone),
                        )
                        .send();
                }

                Ok(true)
            }
            SeasonImageMsg::ClearSelectedSeasonImageDone(result) => {
                result.context(Message::ClearingImage)?;
                self.season_image_modal = false;
                Ok(true)
            }
        }
    }

    fn update_ui(&mut self, ctx: &Context<Self>, msg: UiMsg) -> Result<bool, Error> {
        let props = ctx.props();

        match msg {
            UiMsg::OpenCastModal => {
                self.cast_modal = true;
                Ok(true)
            }
            UiMsg::CloseCastModal => {
                self.cast_modal = false;
                Ok(true)
            }
            UiMsg::SelectSeason(season) => {
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

                Ok(false)
            }
            UiMsg::ToggleHistory(id) => {
                self.episode_menu = None;
                if !self.history_expanded.insert(id) {
                    self.history_expanded.remove(&id);
                }

                Ok(true)
            }
            UiMsg::OpenImageModal => {
                self.image_modal = true;
                self.settings_modal = false;
                Ok(true)
            }
            UiMsg::CloseImageModal => {
                self.image_modal = false;
                // Opened from Settings, so closing goes back there.
                self.settings_modal = true;
                Ok(true)
            }
            UiMsg::OpenSettingsModal => {
                self.settings_modal = true;
                self.actions_expanded = false;
                Ok(true)
            }
            UiMsg::CloseSettingsModal => {
                self.settings_modal = false;
                Ok(true)
            }
            UiMsg::OpenShowTranslations => {
                self.show_translations_modal = true;
                self.actions_expanded = false;
                Ok(true)
            }
            UiMsg::CloseShowTranslations => {
                self.show_translations_modal = false;
                Ok(true)
            }
            UiMsg::OpenSeasonTranslations => {
                self.season_translations_modal = true;
                Ok(true)
            }
            UiMsg::CloseSeasonTranslations => {
                self.season_translations_modal = false;
                Ok(true)
            }
            UiMsg::OpenEpisodeTranslations(episode_id) => {
                self.episode_menu = None;
                self.episode_translations = Some(episode_id);
                Ok(true)
            }
            UiMsg::CloseEpisodeTranslations => {
                self.episode_translations = None;
                Ok(true)
            }
            UiMsg::OpenEpisodeReleases(episode_id) => {
                self.episode_menu = None;
                self.episode_releases_modal = Some(episode_id);
                Ok(true)
            }
            UiMsg::CloseEpisodeReleases => {
                self.episode_releases_modal = None;
                Ok(true)
            }
            UiMsg::OpenEpisodeCache(episode_id) => {
                self.episode_menu = None;
                self.episode_cache_modal = Some(episode_id);
                Ok(true)
            }
            UiMsg::CloseEpisodeCache => {
                self.episode_cache_modal = None;
                Ok(true)
            }
            UiMsg::OpenRemoteEditor => {
                self.remote_editor = true;
                self.settings_modal = false;
                Ok(true)
            }
            UiMsg::CloseRemoteEditor => {
                self.remote_editor = false;
                // Opened from Settings, so closing goes back there.
                self.settings_modal = true;
                Ok(true)
            }
            UiMsg::OpenNumberingEditor(manual) => {
                self.numbering_editor = Some(manual);
                self.settings_modal = false;
                Ok(true)
            }
            UiMsg::CloseNumberingEditor => {
                self.numbering_editor = None;
                self.settings_modal = true;
                Ok(true)
            }
            UiMsg::ToggleActionsExpanded => {
                self.actions_expanded = !self.actions_expanded;
                Ok(true)
            }
            UiMsg::ToggleEpisodeMenu(episode_id) => {
                self.episode_menu = (self.episode_menu != Some(episode_id)).then_some(episode_id);
                Ok(true)
            }
            UiMsg::ToggleSeasonActionsExpanded(season) => {
                if !self.season_actions_expanded.insert(season) {
                    self.season_actions_expanded.remove(&season);
                }

                Ok(true)
            }
            UiMsg::ToggleOrphaned => {
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

    fn clear_picked(&mut self) {
        self.picked.clear();
        self.pick_anchor = None;
        self._pick_escape = None;
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
            .on_packet(ctx.link().callback(LoadMsg::Show))
            .send();
    }

    /// Cast grid plus a compact key-crew line, capped; the full cast opens in
    /// a searchable modal.
    fn view_credits(&self, ctx: &Context<Self>) -> Html {
        if self.credits.is_empty() {
            return html! {};
        }

        let shown = self
            .credits
            .get(..CAP.min(self.credits.len()))
            .unwrap_or_default();

        html! {
            <section class="credits">
                <h2>{"Cast & crew"}</h2>

                if !shown.is_empty() {
                    <div class="cast-grid">
                        { for shown.iter().map(cast_card) }
                    </div>
                }

                if self.credits.len() > CAP {
                    <Button icon="user-group" label="Show all cast" title="Show all cast" class="credits-toggle" onclick={ctx.link().callback(|_| UiMsg::OpenCastModal)} />
                }

                if self.cast_modal {
                    <CastModal credits={self.credits.clone()} on_close={ctx.link().callback(|_| UiMsg::CloseCastModal)} />
                }
            </section>
        }
    }

    fn update_graphics(&mut self) {
        match &self.show {
            ShowState::Loaded(show) => self
                .graphics
                .set(&show.images, |kind, key| show.is_selected(kind, key)),
            _ => self.graphics.clear(),
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
            .on_packet(ctx.link().callback(LoadMsg::Seasons))
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
            .on_packet(ctx.link().callback(LoadMsg::Credits))
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
            .on_packet(ctx.link().callback(SeasonImageMsg::SeasonImagesLoaded))
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
            .on_packet(ctx.link().callback(LoadMsg::Episodes))
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
            .on_packet(ctx.link().callback(LoadMsg::Watched))
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
            .on_packet(ctx.link().callback(LoadMsg::Orphaned))
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
            .body(api::GetPreferencesRequest)
            .on_packet(ctx.link().callback(LoadMsg::Config))
            .send();
    }

    fn view_sidebar(&self, ctx: &Context<Self>, show: &api::Show, season: &api::Season) -> Html {
        let poster = season.poster.as_ref().or(show.poster.as_ref());

        html! {
            <div class="detail-sidebar">
                <Image class="poster desktop-only artwork" src={poster.cloned()} />

                <nav class="season-list desktop-only" aria-label="Seasons">
                    { for self.seasons.iter().map(|s| self.view_season(ctx, s)) }
                </nav>
            </div>
        }
    }

    fn view_image_modal(&self, ctx: &Context<Self>) -> Html {
        let user_selected = |kind: api::ImageKind| match &self.show {
            ShowState::Loaded(show) => show.is_user_selected(kind),
            _ => false,
        };

        self.graphics.view_modal(
            ctx.link(),
            user_selected,
            ctx.link().callback(|_| UiMsg::CloseImageModal),
        )
    }

    fn view_season_image_modal(&self, ctx: &Context<Self>) -> Html {
        let link = ctx.link();

        html! {
            <Modal icon="photo" title="Season Graphics" on_close={link.callback(|_| SeasonImageMsg::CloseSeasonImageModal)}>
                {for self.season_graphics.iter().map(|(&kind, items)| {
                    html! {
                        <ImageGallery
                            items={items.clone()}
                            {kind}
                            on_select={link.callback(move |id| SeasonImageMsg::SelectSeasonImage(kind, id))}
                            on_clear={link.callback(move |_| SeasonImageMsg::ClearSelectedSeasonImage(kind))}
                        />
                    }
                })}
            </Modal>
        }
    }

    /// A season in the wide sidebar list: its name, how much of it is watched
    /// and its year, over a bar showing that progress.
    fn view_season(&self, ctx: &Context<Self>, s: &api::Season) -> Html {
        let season = s.season;
        let current = self.selected().map(|s| s.id) == Some(s.id);
        let watched = s.watched_count.min(s.total_count);
        let finished = s.total_count > 0 && watched == s.total_count;
        let name = season_name(s);

        let body = html! {
            <>
                <span class="season-name">{name.clone()}</span>

                if !s.alt_names.is_empty() {
                    <span class="season-alt-names">{season_alt_names(s)}</span>
                }

                <span class="season-count" title="Episodes watched">
                    if finished {
                        <span class="icon sm check" aria-hidden="true" />
                    }

                    if s.total_count > 0 {
                        {format!("{watched}/{}", s.total_count)}
                    }
                </span>

                <span class="season-year">
                    if let Some(ts) = s.air_date {
                        {ts.date(self.time.clone()).year().to_string()}
                    }
                </span>

                if s.total_count > 0 {
                    <span class="season-progress">
                        <span style={format!("width: {:.0}%", watched as f64 * 100.0 / s.total_count as f64)} />
                    </span>
                }
            </>
        };

        let class = classes!(
            "season-row",
            current.then_some("current"),
            finished.then_some("finished"),
            (!s.alt_names.is_empty()).then_some("has-alt")
        );

        if self.seasons.len() > 1 {
            html! {
                <Button {class} title={format!("Show {name}")} {current} onclick={ctx.link().callback(move |_| UiMsg::SelectSeason(season))}>
                    {body}
                </Button>
            }
        } else {
            html! {
                <div {class}>{body}</div>
            }
        }
    }

    /// The seasons as a sideways-scrolling row of chips, shown on phones right
    /// above the episodes they switch between.
    fn view_season_chips(&self, ctx: &Context<Self>) -> Html {
        if self.seasons.len() < 2 {
            return html! {};
        }

        let current = self.selected().map(|s| s.id);

        html! {
            <nav class="season-chips mobile-only" aria-label="Seasons" ref={self.season_chips.clone()}>
                { for self.seasons.iter().map(|s| {
                    let season = s.season;
                    let is_current = current == Some(s.id);
                    let watched = s.watched_count.min(s.total_count);
                    let finished = s.total_count > 0 && watched == s.total_count;
                    let name = season_name(s);

                    html! {
                        <Button class={classes!("chip", is_current.then_some("selected"))} title={format!("Show {name}")} current={is_current} onclick={ctx.link().callback(move |_| UiMsg::SelectSeason(season))}>
                            <span>{name}</span>

                            if finished {
                                <span class="icon sm check" aria-hidden="true" />
                            } else if s.total_count > 0 {
                                <span class="chip-count">{format!("{watched}/{}", s.total_count)}</span>
                            }
                        </Button>
                    }
                }) }
            </nav>
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

        // A season linked to several seasons of another numbering marks where
        // each starts.
        let mut previous = None;

        let mut starts = self
            .episodes
            .iter()
            .map(|e| {
                let link = e.link.as_ref()?;
                let start = (previous != Some(link)).then_some(link);
                previous = Some(link);
                start
            })
            .collect::<Vec<_>>();

        if starts.iter().flatten().count() < 2 {
            starts.clear();
        }

        let pending_episode = self.pending_episode.as_ref().map(|&(label, episode_id)| {
            let callback = link.callback(move |_| WatchMsg::OnRemoveNext(episode_id));
            (label, callback)
        });

        let season_expanded = self.season_actions_expanded.contains(&season_number);
        let toggle_menu =
            link.callback(move |_: MouseEvent| UiMsg::ToggleSeasonActionsExpanded(season_number));

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
            move |mark_time| WatchMsg::WatchRemaining(season, mark_time)
        });

        html! {
            <div class="detail-content">
                { self.view_credits(ctx) }

                { self.view_season_chips(ctx) }

                // Keyed so a season switch replaces the column: patching it,
                // Yew panics inserting the season names before an empty
                // TranslatedText while removing the watched count after it.
                <div class="column" key={season.id.to_string()}>
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
                                        <span class="icon chevron-down" aria-hidden="true" />
                                    </a>
                                }

                                <Button icon={if season_expanded { "ellipsis-horizontal" } else { "bars-2" }} title="Season actions" expanded={Some(season_expanded)} onclick={link.callback(move |_| UiMsg::ToggleSeasonActionsExpanded(season_number))} />
                            </div>
                        </div>

                        <div class={classes!("toolbar-dropdown", "desktop-input-group", (!season_expanded).then_some("desktop-only"))}>
                            <Button icon="language" title="Season Translations" text="Translations" onclick={link.callback(|_| UiMsg::OpenSeasonTranslations)} />

                            if crate::is_admin(ctx) {
                                <Button icon="photo" title="Season Graphics" text="Graphics" onclick={link.callback(|_| SeasonImageMsg::OpenSeasonImageModal)} />
                            }

                            if let Some((label, on_remove_next)) = pending_episode {
                                <a class="button primary mobile-has-text" href={format!("#{label}")} onclick={toggle_menu} title="Jump to pending episode">
                                    <span class="icon chevron-down" aria-hidden="true" />
                                    <span class="mobile-only">{format!("Jump to next episode {label}")}</span>
                                </a>

                                <Button icon="bookmark" variant={Variant::Danger} title="Remove pending" text={format!("Clear next episode {label}")} onclick={on_remove_next} />
                            } else if let Some((label, episode_id)) = next_unwatched {
                                <MarkTimeMenu class="mobile-has-text" title="Make next episode" prompt={format!("Pending {label} since when?")} preset={next_episode_preset.clone()} on_confirm={link.callback(move |mark_time| WatchMsg::OnWatchNext(episode_id, mark_time))}>
                                    <span class="icon bookmark-slash" aria-hidden="true" />
                                    <span class="mobile-only">{label}</span>
                                </MarkTimeMenu>
                            }

                            if watched_count < total {
                                <MarkTimeMenu class="primary mobile-has-text" title="Mark remaining episodes as watched" prompt="When did you watch the remaining episodes?" preset={Some(remaining_preset.clone())} on_confirm={watch_remaining}>
                                    <span class="icon check" aria-hidden="true" />
                                    <span class="mobile-only">{"Remaining"}</span>
                                </MarkTimeMenu>
                            }
                        </div>
                    </div>

                    if !season.alt_names.is_empty() {
                        { view_season_names(season) }
                    }

                    <TranslatedText strings={season.strings.clone()} />

                    if total > 0 {
                        <span class="text-muted">{format!("{watched_count} of {total} watched")}</span>
                    }
                </div>

                if self.episodes.is_empty() && self.selected.is_some() {
                    <div class="text-muted">{"No episodes."}</div>
                }

                <div class="episodes">
                    { for self.episodes.iter().enumerate().map(|(i, ep)| html! {
                        <>
                            if let Some(Some(link)) = starts.get(i) {
                                { view_link_divider(season, link) }
                            }

                            { self.view_episode(ctx, ep) }
                        </>
                    }) }
                </div>

                if !self.picked.is_empty() {
                    { self.view_selection_bar(ctx) }
                }
            </div>
        }
    }

    /// What can be done with the picked episodes, kept in view while any are
    /// picked.
    fn view_selection_bar(&self, ctx: &Context<Self>) -> Html {
        let link = ctx.link();
        let count = self.picked.len();

        let aired = TimePreset::when_aired(
            "calendar",
            "Aired",
            if count == 1 {
                "When the episode aired"
            } else {
                "When each episode aired"
            },
        );

        html! {
            <div class="selection-bar" role="region" aria-label="Selected episodes">
                <span class="selection-count">
                    {if count == 1 { "1 episode selected".to_owned() } else { format!("{count} episodes selected") }}
                </span>

                <div class="selection-actions">
                    <MarkTimeMenu class="primary has-text" icon="check" title="Mark the selected episodes watched" prompt="When did you watch them?" preset={aired} on_confirm={link.callback(PickMsg::BulkMark)}>
                        <span class="icon check" aria-hidden="true" />
                        <span>{"Mark watched"}</span>
                    </MarkTimeMenu>

                    <Button icon="arrow-path" label="Sync" title="Sync the selected episodes" onclick={link.callback(|_| PickMsg::BulkSync)} />
                    <Button icon="x-mark" label="Clear" title="Clear the selection" onclick={link.callback(|_| PickMsg::ClearPicked)} />
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
            (!watched.is_empty()).then(|| link.callback(move |_| UiMsg::ToggleHistory(episode_id)));

        let menu_open = self.episode_menu == Some(episode_id);
        let picked = self.picked.contains(&episode_id);
        let on_toggle_menu =
            link.callback(move |_: MouseEvent| UiMsg::ToggleEpisodeMenu(episode_id));
        let syncing = self
            .active_tasks
            .task(SyncTarget::Episode(episode_id))
            .is_some();

        let on_remove_next = link.callback(move |_| WatchMsg::OnRemoveNext(episode_id));
        let on_next_episode =
            link.callback(move |mark_time| WatchMsg::OnWatchNext(episode_id, mark_time));

        let on_mark_confirm =
            link.callback(move |mark_time| WatchMsg::MarkWatched(show_id, episode_id, mark_time));

        let preset = episode
            .aired
            .map(|timestamp| TimePreset::at("calendar", "Aired", timestamp));

        html! {
            <div class={classes!("episode", (!watched.is_empty()).then_some("watched"))} id={episode.code()}>
                <Button class={classes!("episode-pick", picked.then_some("picked"))} title={format!("Select {}", episode.code())} pressed={Some(picked)} onclick={link.callback(move |e: MouseEvent| PickMsg::TogglePick(episode_id, e.shift_key()))}>
                    <Image class="screenshot artwork" placeholder={true} placeholder_icon="photo" src={episode.screenshot.clone()} />
                    <span class="episode-pick-mark" aria-hidden="true">
                        <span class="icon sm check" />
                    </span>
                </Button>

                <div class="episode-body">
                    <div class="episode-head">
                        <div class="episode-heading">
                            <a class="episode-code" href={format!("#{}", episode.code())} title="Link to this episode">{episode.code()}</a>

                            if let Some(name) = episode.strings.title() {
                                <h4>{name}</h4>
                            }
                        </div>

                        <div class="row episode-actions">
                            <div class="input-group">
                                <MarkTimeMenu quick=true class="primary" icon="check" title="Mark watched" prompt={format!("When did you watch {}?", episode.code())} preset={preset.clone()} on_confirm={on_mark_confirm} />
                            </div>

                            <div class="input-group">
                                if episode.pending.is_some() {
                                    <Button icon="bookmark" variant={Variant::Primary} title="Next episode" pressed={Some(true)} onclick={on_remove_next} />
                                } else {
                                    <MarkTimeMenu icon="bookmark" title="Not next episode" prompt={format!("When do you want to queue {}?", episode.code())} preset={preset.clone()} on_confirm={on_next_episode}>
                                        <span class="icon bookmark" aria-hidden="true" />
                                    </MarkTimeMenu>
                                }

                                <Button node_ref={if menu_open { self.episode_menu_anchor.clone() } else { NodeRef::default() }} icon="ellipsis-horizontal" class={classes!(menu_open.then_some("selected"))} title="More actions" expanded={Some(menu_open)} haspopup="menu" onclick={on_toggle_menu.clone()} />
                            </div>
                        </div>

                        if menu_open {
                            <ContextMenu anchor={self.episode_menu_anchor.clone()} on_close={link.callback(move |()| UiMsg::ToggleEpisodeMenu(episode_id))}>
                                <div class="menu-list" role="menu" aria-label="Episode actions">
                                    <Button role="menuitem" icon="arrow-path" spin={syncing} label="Sync episode" title="Sync episode" onclick={link.callback(move |_| ActionMsg::SyncEpisode(episode_id))} />
                                    <Button role="menuitem" icon="language" label="Translations" title="Translations" onclick={link.callback(move |_| UiMsg::OpenEpisodeTranslations(episode_id))} />
                                    <Button role="menuitem" icon="calendar" label="Air dates" title="Air dates" onclick={link.callback(move |_| UiMsg::OpenEpisodeReleases(episode_id))} />
                                    <Button role="menuitem" icon="circle-stack" label="Cache" title="Cache" onclick={link.callback(move |_| UiMsg::OpenEpisodeCache(episode_id))} />

                                    if let Some(on_toggle) = on_toggle_history {
                                        <Button role="menuitem" icon="clock" label={if history_expanded { "Hide watch history" } else { "Watch history" }} title="Watch history" onclick={on_toggle} />
                                    }
                                </div>
                            </ContextMenu>
                        }
                    </div>

                    if !episode.numberings.is_empty() {
                        <div class="episode-numberings">
                            { for episode.numberings.iter().map(view_numbering) }
                        </div>
                    }

                    <div class="episode-meta">
                        <indicator title="Air date">
                            <span class="item-inline">
                                <span class={classes!("icon", if episode.aired().is_some() { "clock" } else { "exclamation-circle" })} />
                            </span>

                            <content>
                                if let Some(aired) = episode.human_date_time(self.time.clone()) {
                                    <span>{if aired.is_past() { "Aired" } else { "Airs" }}</span>
                                    {aired.lower().view()}
                                } else {
                                    <span>{"No air date"}</span>
                                }
                            </content>
                        </indicator>

                        <indicator title="Watch status">
                        if episode.pending.is_some() {
                            <span class="item-inline" title="Next episode">
                                <span class="icon primary exclamation-circle" aria-hidden="true" />
                            </span>
                        } else if !watched.is_empty() {
                            <span class="item-inline" title="Watched">
                                <span class="icon primary check-circle" aria-hidden="true" />
                            </span>
                        } else {
                            <span class="item-inline" title="Never watched">
                                <span class="icon secondary x-circle" aria-hidden="true" />
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

                    <TranslatedText strings={episode.strings.clone()} />
                </div>

                if history_expanded {
                    <Modal icon="clock" title={format!("Watch history for {}", episode.code())} on_close={link.callback(move |_| UiMsg::ToggleHistory(episode_id))}>
                        if let Some(moving) = watched.iter().find(|w| self.fixing_watched == Some(w.watched.id)) {
                            <p class="watch-move-lead">
                                {"Move the watch from "}
                                {moving.watched.timestamp.human_date_time(self.time.clone()).lower().view()}
                                {" to another episode."}
                            </p>

                            <EpisodePicker
                                show_id={show_id}
                                season={episode.season}
                                episode={episode.episode}
                                on_confirm={link.callback({
                                    let wid = moving.watched.id;
                                    move |(season, ep)| WatchMsg::MoveWatched(wid, season, ep)
                                })}
                                on_cancel={link.callback(|_| WatchMsg::CancelFixWatched)}
                            />
                        } else {
                            <div key="history" class="watch-history">
                                { for watched.iter().map(|w| {
                                    let wid = w.watched.id;
                                    let kind = api::WatchedKind::Episode { show: show_id, episode: episode_id };

                                    html! {
                                        <div class="watch-row">
                                            <span class="watch-when">{w.watched.timestamp.human_date_time(self.time.clone()).view()}</span>
                                            <span class="watch-age">{w.watched.timestamp.relative_to(self.time.now())}</span>

                                            <div class="watch-actions" ref={w.context_anchor.clone()}>
                                                <Button icon="arrow-uturn-right" label="Move" title="Move to another episode" onclick={link.callback(move |_| WatchMsg::FixWatched(wid))} />

                                                <Button icon="trash" label="Remove" title="Remove" expanded={Some(self.confirm_remove_watch == Some(wid))} haspopup="dialog" onclick={link.callback(move |_| WatchMsg::ConfirmRemoveWatch(wid))} />

                                                if self.confirm_remove_watch == Some(wid) {
                                                    <ContextMenu prompt="Remove watch at" label={w.watched.timestamp.human_date_time(self.time.clone())} anchor={w.context_anchor.clone()} on_close={link.callback(|_| WatchMsg::CancelRemoveWatch)}>
                                                        <ConfirmDanger
                                                            on_confirm={link.callback(move |_| WatchMsg::RemoveWatched(wid, kind))}
                                                            on_cancel={link.callback(|_| WatchMsg::CancelRemoveWatch)}
                                                        />
                                                    </ContextMenu>
                                                }
                                            </div>
                                        </div>
                                    }
                                }) }
                            </div>
                        }
                    </Modal>
                }

                if self.episode_releases_modal == Some(episode_id) {
                    <ReleaseModal
                        target={ReleaseTarget::Episode(episode_id)}
                        title={format!("Air dates for {}", episode.code())}
                        on_close={link.callback(|_| UiMsg::CloseEpisodeReleases)}
                    />
                }

                if self.episode_cache_modal == Some(episode_id) {
                    <EpisodeCacheModal
                        episode_id={episode_id}
                        title={format!("Cache for {}", episode.code())}
                        on_close={link.callback(|_| UiMsg::CloseEpisodeCache)}
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
                                    <Button icon="pencil-square" title="Move to episode" expanded={Some(self.fixing_watched == Some(id))} haspopup="dialog" onclick={link.callback(move |_| WatchMsg::FixWatched(id))} />

                                    <Button icon="trash" variant={Variant::Danger} title="Remove" text="Remove" expanded={Some(self.confirm_remove_watch == Some(id))} haspopup="dialog" onclick={link.callback(move |_| WatchMsg::ConfirmRemoveWatch(id))} />
                                </div>

                                if self.fixing_watched == Some(id) {
                                    <ContextMenu anchor={w.context_anchor.clone()} prompt="Where do you want to move orphaned watch at" label={w.watched.timestamp.human_date_time(self.time.clone())} on_close={ctx.link().callback(|_| WatchMsg::CancelFixWatched)}>
                                        <EpisodePicker
                                            {show_id}
                                            season={w.watched.season}
                                            episode={w.watched.episode}
                                            timestamp={w.watched.timestamp}
                                            on_confirm={link.callback(move |(season, ep)| WatchMsg::MoveWatched(id, season, ep))}
                                            on_cancel={link.callback(|_| WatchMsg::CancelFixWatched)}
                                        />
                                    </ContextMenu>
                                }

                                if self.confirm_remove_watch == Some(id) {
                                    <ContextMenu anchor={w.context_anchor.clone()} prompt="Remove orphaned watch at" label={w.watched.timestamp.human_date_time(self.time.clone())} on_close={ctx.link().callback(|_| WatchMsg::CancelRemoveWatch)}>
                                        <ConfirmDanger
                                            on_confirm={link.callback(move |_| WatchMsg::RemoveWatched(id, kind))}
                                            on_cancel={link.callback(|_| WatchMsg::CancelRemoveWatch)}
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

/// A season's own title, else its number spelled out ("Season 2", "Specials").
fn season_name(s: &api::Season) -> String {
    match s.strings.title() {
        Some(name) => name.to_owned(),
        None => s.season.long().to_string(),
    }
}

/// One of an episode's other numberings: the system's plate and the code,
/// with the absolute number in the tooltip.
fn view_numbering(n: &api::AltNumbering) -> Html {
    let label = api::xem_system_label(&n.system);
    let code = n.code();

    let title = match n.absolute {
        Some(absolute) => format!("{label} {code}, absolute {absolute}"),
        None => format!("{label} {code}"),
    };

    html! {
        <span class="numbering-chip" {title}>
            <span class={classes!("logo", n.system.clone())} aria-hidden="true" />
            <span class="numbering-chip-code">{code}</span>
        </span>
    }
}

/// The names of the seasons `s` covers, on one line.
fn season_alt_names(s: &api::Season) -> String {
    let mut names = Vec::new();

    for group in &s.alt_names {
        for n in &group.names {
            names.push(n.name.as_str());
        }
    }

    names.join(" · ")
}

/// A linked season's label, such as `TheTVDB S2`.
fn linked_label(l: &api::LinkedSeason) -> String {
    format!("{} S{}", api::xem_system_label(&l.system), l.season)
}

/// The names of the seasons a season covers under its heading, each with the
/// season of the other numbering it names.
fn view_season_names(s: &api::Season) -> Html {
    html! {
        <p class="season-names">
            <span class="season-names-label">{"Season names"}</span>

            { for s.alt_names.iter().map(|group| html! {
                <span class="season-names-group">
                    { for group.names.iter().map(|n| html! {
                        <span class="alt-name">{n.name.clone()}</span>
                    }) }

                    <span class="text-muted">{format!("({})", linked_label(&group.target))}</span>
                </span>
            }) }
        </p>
    }
}

/// The line before the first episode linked to `link`, naming that season
/// and its XEM names.
fn view_link_divider(season: &api::Season, link: &api::LinkedSeason) -> Html {
    let names = season
        .alt_names
        .iter()
        .find(|g| g.target == *link)
        .map(|g| {
            g.names
                .iter()
                .map(|n| n.name.as_str())
                .collect::<Vec<_>>()
                .join(" · ")
        });

    html! {
        <div class="numbering-divider" role="separator" aria-label={linked_label(link)}>
            <span class={classes!("logo", link.system.clone())} title={api::xem_system_label(&link.system).to_owned()} />
            <span class="numbering-divider-season">{format!("Season {}", link.season)}</span>

            if let Some(names) = names {
                <span class="text-muted">{format!("· {names}")}</span>
            }
        </div>
    }
}
