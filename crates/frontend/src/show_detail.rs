use std::collections::{BTreeMap, HashMap, HashSet};

use musli_web::web03::prelude::*;
use yew::prelude::*;

use api::{HasAired, TimeZone};

use crate::background::Background;
use crate::error::{CustomContext, Error, Message};
use crate::router::{MediaQuery, Route, ShowDetailQuery};
use crate::ui::{
    ConfirmDanger, EpisodePicker, Loading, MDASH, MarkWatchedPicker, MediaSettingsModal,
    RemoteEditor, RemoteSourceKind, Tracked,
};
use crate::{Image, ImageGallery, ImageItem, Modal, SetupChannel};

pub(super) struct ShowDetail {
    channel: ws::Channel,
    show: Option<api::Show>,
    graphics: BTreeMap<api::ImageKind, Vec<ImageItem>>,
    season_graphics: BTreeMap<api::ImageKind, Vec<ImageItem>>,
    seasons: Vec<api::Season>,
    selected: Option<usize>,
    expanded_seasons: bool,
    episodes: Vec<api::Episode>,
    pending_episode: Option<(api::Code, api::EpisodeId)>,
    next_unwatched: Option<(api::Code, api::EpisodeId)>,
    view_orphaned: bool,
    confirm_remove: bool,
    syncing: bool,
    actions_expanded: bool,
    episode_actions_expanded: HashSet<api::EpisodeId>,
    confirm_remove_watch: Option<api::WatchedId>,
    confirming_mark_watch: Option<api::EpisodeId>,
    confirming_pending: Option<api::EpisodeId>,
    confirming_pending_header: Option<(api::Code, api::EpisodeId)>,
    select_mark_remaining: bool,
    watched_by_episode: HashMap<api::EpisodeId, Vec<api::WatchedEpisode>>,
    history_expanded: HashSet<api::EpisodeId>,
    orphaned: Vec<api::OrphanedWatched>,
    fixing_watched: Option<api::WatchedId>,
    image_modal: bool,
    season_image_modal: bool,
    settings_modal: bool,
    remote_editor: bool,
    global_sync_kinds: Vec<api::SourceSyncKinds>,
    background: Background,
    tz: TimeZone,
    _tz_handle: ContextHandle<TimeZone>,
    _setup: SetupChannel,
    _broadcast: ws::Listener,
    _show_req: ws::Request,
    _seasons_req: ws::Request,
    _episodes_req: ws::Request,
    _mark_req: ws::Request,
    _remove_watch_req: ws::Request,
    _untrack_req: ws::Request,
    _remove_req: ws::Request,
    _sync_req: ws::Request,
    _watch_remaining_reqs: ws::Request,
    _watched_req: ws::Request,
    _set_next_req: ws::Request,
    _select_image_req: ws::Request,
    _clear_image_req: ws::Request,
    _season_images_req: ws::Request,
    _select_season_image_req: ws::Request,
    _clear_season_image_req: ws::Request,
    _set_remote_enabled_req: ws::Request,
    _reorder_remotes_req: ws::Request,
    _set_remote_sync_kinds_req: ws::Request,
    _set_language_req: ws::Request,
    _set_include_specials_req: ws::Request,
    _set_air_date_filters_req: ws::Request,
    _config_req: ws::Request,
    _orphaned_req: ws::Request,
    _move_req: ws::Request,
    _remote_req: ws::Request,
}

pub(super) enum Msg {
    Channel(Result<ws::Channel, ws::Error>),
    AppBroadcast(Result<ws::Packet<api::AppBroadcast>, ws::Error>),
    ShowLoaded(Result<ws::Packet<api::GetShow>, ws::Error>),
    SeasonsLoaded(Result<ws::Packet<api::ListSeasons>, ws::Error>),
    SelectSeason(api::SeasonNumber),
    ToggleExpandSeasons,
    EpisodesLoaded(Result<ws::Packet<api::ListEpisodes>, ws::Error>),
    AskMarkWatched(api::EpisodeId),
    MarkWatched(api::ShowId, api::EpisodeId, api::MarkTime),
    MarkWatchedDone(Result<ws::Packet<api::MarkWatched>, ws::Error>),
    CancelMarkWatch(api::EpisodeId),
    MarkRemainingWatch,
    CancelMarkRemainingWatch,
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
    ToggleHistory(api::EpisodeId),
    WatchedLoaded(Result<ws::Packet<api::ListEpisodesWatched>, ws::Error>),
    AskWatchNext(api::EpisodeId),
    AskWatchNextHeader(api::Code, api::EpisodeId),
    CancelWatchNext(api::EpisodeId),
    OnWatchNext(api::EpisodeId, api::MarkTime),
    AddPendingDone(Result<ws::Packet<api::AddPending>, ws::Error>),
    OnRemoveNext(api::EpisodeId),
    RemovePendingDone(Result<ws::Packet<api::RemovePending>, ws::Error>),
    SelectImage(api::ImageKind, api::ImageId),
    ClearSelectedImage(api::ImageKind),
    SelectImageDone(Result<ws::Packet<api::SelectImage>, ws::Error>),
    ClearSelectedImageDone(Result<ws::Packet<api::ClearSelectedImage>, ws::Error>),
    SetRemoteEnabled(api::RemoteId, bool),
    SetRemoteEnabledDone(Result<ws::Packet<api::SetShowRemoteEnabled>, ws::Error>),
    SetRemoteSyncKinds(api::RemoteId, Option<api::SyncKindSet>),
    SetRemoteSyncKindsDone(Result<ws::Packet<api::SetShowRemoteSyncKinds>, ws::Error>),
    ReorderRemotes(Vec<api::RemoteId>),
    ReorderRemotesDone(Result<ws::Packet<api::ReorderShowRemotes>, ws::Error>),
    ConfigLoaded(Result<ws::Packet<api::GetConfig>, ws::Error>),
    SetLanguage(Option<String>),
    SetLanguageDone(
        Option<String>,
        Result<ws::Packet<api::SetShowLanguage>, ws::Error>,
    ),
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
    SetIncludeSpecials(Option<bool>),
    SetIncludeSpecialsDone(
        Option<bool>,
        Result<ws::Packet<api::SetShowIncludeSpecials>, ws::Error>,
    ),
    SetAirDateFilters(Option<Vec<api::AirDateFilter>>),
    SetAirDateFiltersDone(Result<ws::Packet<api::SetShowAirDateFilters>, ws::Error>),
    OpenRemoteEditor,
    CloseRemoteEditor,
    AddRemote(Option<String>, api::Remote),
    EditRemote(api::RemoteId, Option<String>, api::Remote),
    RemoveRemote(api::RemoteId),
    RemoteDone(Result<(), ws::Error>),
    SetTz(TimeZone),
    FixWatched(api::WatchedId),
    CancelFixWatched,
    MoveWatched(api::WatchedId, api::SeasonNumber, u32),
    MoveWatchedDone(Result<ws::Packet<api::MoveWatchedEpisode>, ws::Error>),
    OrphanedLoaded(Result<ws::Packet<api::ListOrphanedWatched>, ws::Error>),
    ToggleActionsExpanded,
    ToggleEpisodeActionsExpanded(api::EpisodeId),
    ToggleOrphaned,
}

#[derive(Properties, PartialEq)]
pub(super) struct Props {
    pub(super) onerror: Callback<Option<Error>>,
    pub(super) show_id: api::ShowId,
    #[prop_or_default]
    pub(super) initial_season: Option<api::SeasonNumber>,
    pub(super) on_navigate: Callback<Route>,
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
            show: None,
            graphics: BTreeMap::new(),
            season_graphics: BTreeMap::new(),
            seasons: Vec::new(),
            selected: None,
            expanded_seasons: false,
            episodes: Vec::new(),
            pending_episode: None,
            next_unwatched: None,
            view_orphaned: false,
            confirm_remove: false,
            syncing: false,
            actions_expanded: false,
            episode_actions_expanded: HashSet::new(),
            confirm_remove_watch: None,
            confirming_mark_watch: None,
            confirming_pending: None,
            confirming_pending_header: None,
            select_mark_remaining: false,
            watched_by_episode: HashMap::new(),
            history_expanded: HashSet::new(),
            orphaned: Vec::new(),
            fixing_watched: None,
            image_modal: false,
            season_image_modal: false,
            settings_modal: false,
            remote_editor: false,
            global_sync_kinds: Vec::new(),
            background,
            tz,
            _tz_handle,
            _setup,
            _broadcast,
            _show_req: ws::Request::default(),
            _seasons_req: ws::Request::default(),
            _episodes_req: ws::Request::default(),
            _mark_req: ws::Request::default(),
            _remove_watch_req: ws::Request::default(),
            _untrack_req: ws::Request::default(),
            _remove_req: ws::Request::default(),
            _sync_req: ws::Request::default(),
            _watch_remaining_reqs: ws::Request::default(),
            _watched_req: ws::Request::default(),
            _set_next_req: ws::Request::default(),
            _select_image_req: ws::Request::default(),
            _clear_image_req: ws::Request::default(),
            _season_images_req: ws::Request::default(),
            _select_season_image_req: ws::Request::default(),
            _clear_season_image_req: ws::Request::default(),
            _set_remote_enabled_req: ws::Request::default(),
            _reorder_remotes_req: ws::Request::default(),
            _set_remote_sync_kinds_req: ws::Request::default(),
            _set_language_req: ws::Request::default(),
            _set_include_specials_req: ws::Request::default(),
            _set_air_date_filters_req: ws::Request::default(),
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
                ctx.props().onerror.emit(Some(e));
                false
            }
        }
    }

    fn destroy(&mut self, _ctx: &Context<Self>) {
        self.background.title(None);
    }

    fn view(&self, ctx: &Context<Self>) -> Html {
        let (Some(show), Some(season)) = (&self.show, self.selected()) else {
            return html!(<Loading />);
        };

        let link = ctx.link();

        let actions = 'actions: {
            if self.confirm_remove {
                break 'actions html! {
                    <ConfirmDanger prompt="Remove show" label={show.title.clone()} on_confirm={link.callback(|_| Msg::RemoveShow)} on_cancel={link.callback(|_| Msg::CancelRemove)} />
                };
            }

            html! {
                <>
                    if !show.remotes.is_empty() {
                        <div class="desktop-row mobile-column fill start">
                            <div class="row justify-around">
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
                        </div>
                    }

                    <div class="desktop-row mobile-column desktop-input-group end">
                        <Tracked tracked={show.tracked} ontoggle={link.callback(Msg::SetTracked)} />

                        <button class="btn-danger" onclick={link.callback(|_| Msg::ConfirmRemove)} title="Remove show">
                            <span class="icon trash" />
                            <span class="hide-desktop">{"Remove"}</span>
                        </button>

                        if !show.remotes.is_empty() {
                            <button class="btn" onclick={link.callback(|_| Msg::SyncShow)} title="Sync now">
                                <span class={classes!("icon", "arrow-path", self.syncing.then_some("spin"))} />
                                <span class="hide-desktop">{"Sync"}</span>
                            </button>
                        }

                        <button class="btn" onclick={link.callback(|_| Msg::OpenSettingsModal)} title="Show settings">
                            <span class="icon cog-6-tooth" />
                            <span class="hide-desktop">{"Show settings"}</span>
                        </button>
                    </div>
                </>
            }
        };

        html! {
            <>
                <div class="row-fill align-top">
                    <div class="column desktop-center fill">
                        <h1>{show.title.as_deref().unwrap_or("Untitled Show")}</h1>

                        if let Some(date) = show.first_air_date {
                            <span class="text-muted">{date.date(self.tz.clone()).year()}</span>
                        }
                    </div>

                    <div class="hide-desktop row end">
                        <button class="btn" onclick={link.callback(|_| Msg::ToggleActionsExpanded)}>
                            <span class={classes!("icon", if self.actions_expanded { "ellipsis-horizontal" } else { "bars-2" })} />
                        </button>
                    </div>
                </div>

                <div class={classes!("desktop-row-fill", "mobile-column", "actions", (!self.actions_expanded).then_some("hide-mobile"))}>
                    {actions}
                </div>

                if let Some(ref overview) = show.overview {
                    <p class="overview">{overview}</p>
                }

                <div class="detail-layout">
                    <div class="hide-desktop">
                        if let Some(ref banner) = show.banner {
                            <Image class="banner" src={banner.clone()} />
                        } else if let Some(ref backdrop) = show.backdrop {
                            <Image class="backdrop" src={backdrop.clone()} />
                        }
                    </div>

                    { self.view_sidebar(ctx, show, season) }

                    { self.view_episodes(ctx, season) }
                </div>

                if self.image_modal {
                    { self.view_image_modal(ctx) }
                }

                if self.season_image_modal {
                    { self.view_season_image_modal(ctx) }
                }

                if self.settings_modal {
                    <MediaSettingsModal
                        title="Show settings"
                        language={show.language.clone()}
                        include_specials={show.include_specials}
                        has_images={!show.images.is_empty()}
                        has_remotes={!show.remotes.is_empty()}
                        last_synced={show.last_synced_at.map(|ts| AttrValue::from(ts.display(self.tz.clone())))}
                        syncing={self.syncing}
                        on_sync={link.callback(|_| Msg::SyncShow)}
                        on_language_change={link.callback(Msg::SetLanguage)}
                        on_include_specials_change={Some(link.callback(Msg::SetIncludeSpecials))}
                        air_date_filters={show.air_date_filters.clone()}
                        on_air_date_filters_change={Some(link.callback(Msg::SetAirDateFilters))}
                        on_edit_graphics={link.callback(|_| Msg::OpenImageModal)}
                        on_edit_remotes={link.callback(|_| Msg::OpenRemoteEditor)}
                        on_close={link.callback(|_| Msg::CloseSettingsModal)}
                    />
                }

                if self.remote_editor {
                    <RemoteEditor
                        title={show.title.as_deref().unwrap_or("Untitled Show").to_owned()}
                        kind={RemoteSourceKind::Show}
                        remotes={show.remotes.clone()}
                        on_add={link.callback(|(slug, remote)| Msg::AddRemote(slug, remote))}
                        on_edit={link.callback(|(id, slug, remote)| Msg::EditRemote(id, slug, remote))}
                        on_remove={link.callback(Msg::RemoveRemote)}
                        on_set_enabled={link.callback(|(id, enabled)| Msg::SetRemoteEnabled(id, enabled))}
                        on_reorder={link.callback(Msg::ReorderRemotes)}
                        on_set_sync_kinds={link.callback(|(id, kinds)| Msg::SetRemoteSyncKinds(id, kinds))}
                        global_sync_kinds={self.global_sync_kinds.clone()}
                        on_close={link.callback(|_| Msg::CloseRemoteEditor)}
                    />
                }
            </>
        }
    }

    fn changed(&mut self, ctx: &Context<Self>, old_props: &Props) -> bool {
        if ctx.props().show_id != old_props.show_id {
            self.show = None;
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
                self.load_history(ctx);
            }
        } else if ctx.props().initial_season != old_props.initial_season
            && let Some(season) = ctx.props().initial_season
            && self.selected().map(|s| s.season) != Some(season)
        {
            self.selected = self
                .seasons
                .iter()
                .enumerate()
                .find(|(_, s)| s.season == season)
                .map(|(i, _)| i);
            self.episodes.clear();
            self.pending_episode = None;
            self.next_unwatched = None;
            self.confirm_remove_watch = None;
            self.watched_by_episode.clear();

            if self.channel.id() != ws::ChannelId::NONE {
                self.load_episodes(ctx, season);
                self.load_orphaned(ctx);
                self.load_history(ctx);
            }
        }
        true
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
                    self.load_orphaned(ctx);
                    self.load_config(ctx);
                } else {
                    self.show = None;
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
                    api::AppEventKind::ShowChanged { show } if show.id == ctx.props().show_id => {
                        self.background
                            .background(show.backdrop.as_ref().map(|i| i.proxy_url()));
                        self.show = Some(show.clone());
                        self.update_graphics();
                        Ok(true)
                    }
                    api::AppEventKind::SeasonsChanged { show_id, .. }
                        if *show_id == ctx.props().show_id =>
                    {
                        if self.channel.id() != ws::ChannelId::NONE {
                            self.load_seasons(ctx);
                        }

                        Ok(false)
                    }
                    api::AppEventKind::EpisodesChanged { show_id, season }
                        if *show_id == ctx.props().show_id =>
                    {
                        if self.selected().map(|s| s.season) == Some(*season) {
                            self.load_episodes(ctx, *season);
                        }

                        self.load_orphaned(ctx);
                        Ok(false)
                    }
                    api::AppEventKind::PendingChanged => {
                        if let Some(season) = self.selected() {
                            self.load_episodes(ctx, season.season);
                        }

                        self.load_orphaned(ctx);
                        Ok(false)
                    }
                    api::AppEventKind::TaskAdded { task }
                    | api::AppEventKind::TaskStarted { task } => {
                        if matches!(&task.kind, api::TaskKind::SyncShow { show_id, .. } if *show_id == ctx.props().show_id)
                        {
                            self.syncing = true;
                            return Ok(true);
                        }
                        Ok(false)
                    }
                    api::AppEventKind::TaskCompleted { task } => {
                        if matches!(&task.kind, api::TaskKind::SyncShow { show_id, .. } if *show_id == ctx.props().show_id)
                        {
                            self.syncing = false;

                            if let Some(season) = self.selected() {
                                self.load_episodes(ctx, season.season);
                            }

                            self.load_show(ctx);
                            self.load_seasons(ctx);
                            self.load_orphaned(ctx);
                            return Ok(true);
                        }
                        Ok(false)
                    }
                    api::AppEventKind::WatchedChanged { event: kind } => {
                        let relevant = match kind {
                            api::WatchedEvent::Episode { show, .. } => *show == ctx.props().show_id,
                            api::WatchedEvent::RemainingSeason { show, .. } => {
                                *show == ctx.props().show_id
                            }
                            api::WatchedEvent::Movie { .. } => false,
                        };

                        if relevant {
                            if let Some(season) = self.selected() {
                                self.load_episodes(ctx, season.season);
                            }

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

                self.background
                    .background(show.backdrop.as_ref().map(|i| i.proxy_url()));
                self.background.title(show.title.clone());
                self.show = Some(show);
                self.update_graphics();
                Ok(true)
            }
            Msg::SeasonsLoaded(result) => {
                self.seasons = result
                    .context(Message::LoadingSeasons)?
                    .decode()
                    .context(Message::LoadingSeasons)?
                    .seasons;

                if self.selected.is_none() {
                    let initial = ctx
                        .props()
                        .initial_season
                        .and_then(|n| self.seasons.iter().enumerate().find(|(_, s)| s.season == n))
                        .map(|(i, _)| i);

                    self.selected = initial.or_else(|| {
                        self.seasons
                            .iter()
                            .enumerate()
                            .find(|(_, s)| !s.season.is_special())
                            .map(|(i, _)| i)
                    });

                    if let Some(season) = self.selected() {
                        self.load_episodes(ctx, season.season);
                    }

                    self.load_history(ctx);
                    self.load_orphaned(ctx);
                }

                Ok(true)
            }
            Msg::SelectSeason(season) => {
                if self.selected().map(|s| s.season) != Some(season) {
                    let id = ctx.props().show_id;

                    ctx.props().on_navigate.emit(Route::ShowDetail(
                        id,
                        ShowDetailQuery {
                            season: Some(season),
                        },
                    ));
                }

                self.expanded_seasons = false;
                self.view_orphaned = false;
                Ok(false)
            }
            Msg::ToggleExpandSeasons => {
                self.expanded_seasons = !self.expanded_seasons;
                self.view_orphaned = false;
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
                    .find(|e| e.pending)
                    .map(|e| (e.code(), e.id));

                self.next_unwatched = self
                    .episodes
                    .iter()
                    .find(|e| e.watched_count == 0)
                    .map(|e| (e.code(), e.id));

                for w in result.watched {
                    self.watched_by_episode
                        .entry(w.episode_id)
                        .or_default()
                        .push(w);
                }

                self.history_expanded.retain(|episode_id| {
                    self.watched_by_episode
                        .get(episode_id)
                        .is_some_and(|watched| !watched.is_empty())
                });

                Ok(true)
            }
            Msg::AskMarkWatched(episode_id) => {
                self.confirming_mark_watch = Some(episode_id);
                self.confirm_remove_watch = None;
                Ok(true)
            }
            Msg::CancelMarkWatch(episode_id) => {
                self.episode_actions_expanded.remove(&episode_id);
                self.confirming_mark_watch = None;
                Ok(true)
            }
            Msg::MarkRemainingWatch => {
                self.select_mark_remaining = true;
                Ok(true)
            }
            Msg::CancelMarkRemainingWatch => {
                self.select_mark_remaining = false;
                Ok(true)
            }
            Msg::MarkWatched(show, episode, mark_time) => {
                self.episode_actions_expanded.remove(&episode);
                self.confirming_mark_watch = None;
                self._mark_req = self
                    .channel
                    .request()
                    .body(api::MarkWatchedRequest {
                        kind: api::WatchedKind::Episode { show, episode },
                        mark_time,
                    })
                    .on_packet(ctx.link().callback(Msg::MarkWatchedDone))
                    .send();
                Ok(true)
            }
            Msg::MarkWatchedDone(result) => {
                result.context(Message::MarkingWatched)?;

                if let Some(season) = self.selected() {
                    self.load_episodes(ctx, season.season);
                }

                // Refresh season counts so the progress bars reflect the mark.
                self.load_seasons(ctx);
                self.load_history(ctx);
                self.load_orphaned(ctx);
                Ok(false)
            }
            Msg::RemoveWatched(id, kind) => {
                self.orphaned.retain(|w| w.id != id);

                self.actions_expanded = false;

                if let api::WatchedKind::Episode { episode, .. } = kind {
                    self.episode_actions_expanded.remove(&episode);
                }

                if self.orphaned.is_empty() {
                    self.view_orphaned = false;
                }

                self._remove_watch_req = self
                    .channel
                    .request()
                    .body(api::RemoveWatchedRequest { id, kind })
                    .on_packet(ctx.link().callback(Msg::RemoveWatchedDone))
                    .send();

                Ok(false)
            }
            Msg::RemoveWatchedDone(result) => {
                result.context(Message::RemovingWatched)?;
                self.confirm_remove_watch = None;

                if let Some(season) = self.selected() {
                    self.load_episodes(ctx, season.season);
                }

                // Refresh season counts so the progress bars reflect the change.
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
                self.select_mark_remaining = false;

                let show_id = ctx.props().show_id;

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
                Ok(false)
            }
            Msg::WatchRemainingDone(result) => {
                result.context(Message::MarkingWatched)?;

                if let Some(season) = self.selected() {
                    self.load_episodes(ctx, season.season);
                }

                // Refresh season counts so the progress bars reflect the marks.
                self.load_seasons(ctx);
                self.load_history(ctx);
                self.load_orphaned(ctx);
                Ok(false)
            }
            Msg::SetTracked(tracked) => {
                self.actions_expanded = false;

                let id = ctx.props().show_id;

                self._untrack_req = self
                    .channel
                    .request()
                    .body(api::UntrackShowRequest { id, tracked })
                    .on_packet(
                        ctx.link()
                            .callback(move |r| Msg::SetTrackedDone(tracked, r)),
                    )
                    .send();

                Ok(false)
            }
            Msg::SetTrackedDone(tracked, result) => {
                result.context(Message::UntrackingShow)?;
                if let Some(ref mut show) = self.show {
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
                let id = ctx.props().show_id;
                self._remove_req = self
                    .channel
                    .request()
                    .body(api::RemoveShowRequest { id })
                    .on_packet(ctx.link().callback(Msg::RemoveDone))
                    .send();
                Ok(false)
            }
            Msg::RemoveDone(result) => {
                result.context(Message::RemovingShow)?;
                ctx.props()
                    .on_navigate
                    .emit(Route::Media(MediaQuery::default()));
                Ok(false)
            }
            Msg::SyncShow => {
                let id = ctx.props().show_id;

                self._sync_req = self
                    .channel
                    .request()
                    .body(api::SyncShowRequest { id })
                    .on_packet(ctx.link().callback(Msg::SyncDone))
                    .send();

                Ok(true)
            }
            Msg::SyncDone(result) => {
                result.context(Message::SyncingShow)?;
                Ok(false)
            }
            Msg::ToggleHistory(id) => {
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

                for w in watched {
                    self.watched_by_episode
                        .entry(w.episode_id)
                        .or_default()
                        .push(w);
                }

                self.history_expanded.retain(|episode_id| {
                    self.watched_by_episode
                        .get(episode_id)
                        .is_some_and(|watched| !watched.is_empty())
                });

                Ok(true)
            }
            Msg::AskWatchNext(episode_id) => {
                self.confirming_pending = Some(episode_id);
                self.confirming_mark_watch = None;
                Ok(true)
            }
            Msg::AskWatchNextHeader(label, episode_id) => {
                self.confirming_pending_header = Some((label, episode_id));
                self.confirming_mark_watch = None;
                Ok(true)
            }
            Msg::CancelWatchNext(episode_id) => {
                self.episode_actions_expanded.remove(&episode_id);
                self.confirming_pending = None;
                self.confirming_pending_header = None;
                Ok(true)
            }
            Msg::OnWatchNext(episode_id, mark_time) => {
                self.episode_actions_expanded.remove(&episode_id);
                self.confirming_pending = None;
                self.confirming_pending_header = None;

                let show_id = ctx.props().show_id;

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

                Ok(true)
            }
            Msg::AddPendingDone(result) => {
                result.context(Message::SyncingShow)?;

                if let Some(season) = self.selected() {
                    self.load_episodes(ctx, season.season);
                }

                self.load_orphaned(ctx);
                Ok(false)
            }
            Msg::OnRemoveNext(episode_id) => {
                self.episode_actions_expanded.remove(&episode_id);
                let show_id = ctx.props().show_id;

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

                Ok(false)
            }
            Msg::RemovePendingDone(result) => {
                result.context(Message::SyncingShow)?;

                if let Some(season) = self.selected() {
                    self.load_episodes(ctx, season.season);
                }

                Ok(false)
            }
            Msg::SelectImage(kind, id) => {
                if let Some(images) = self.graphics.get_mut(&kind) {
                    for image in images {
                        image.selected = image.id == id;
                    }
                }

                self._select_image_req = self
                    .channel
                    .request()
                    .body(api::SelectImageRequest { id })
                    .on_packet(ctx.link().callback(Msg::SelectImageDone))
                    .send();

                Ok(true)
            }
            Msg::ClearSelectedImage(kind) => {
                if let Some(images) = self.graphics.get_mut(&kind) {
                    for image in images {
                        image.selected = false;
                    }
                }

                self._clear_image_req = self
                    .channel
                    .request()
                    .body(api::ClearSelectedImageRequest {
                        owner: api::ImageOwner::Show(ctx.props().show_id),
                        kind,
                    })
                    .on_packet(ctx.link().callback(Msg::ClearSelectedImageDone))
                    .send();

                Ok(false)
            }
            Msg::SelectImageDone(result) => {
                result.context(Message::SyncingShow)?;
                self.image_modal = false;
                self.load_show(ctx);
                Ok(true)
            }
            Msg::ClearSelectedImageDone(result) => {
                result.context(Message::SyncingShow)?;
                self.image_modal = false;
                self.load_show(ctx);
                Ok(true)
            }
            Msg::SetRemoteEnabled(remote_id, enabled) => {
                if let Some(ref mut show) = self.show
                    && let Some(entry) = show.remotes.iter_mut().find(|e| e.id == remote_id)
                {
                    entry.enabled = enabled;
                }

                let id = ctx.props().show_id;

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

                Ok(true)
            }
            Msg::SetRemoteEnabledDone(result) => {
                result.context(Message::SettingSyncSource)?;
                Ok(true)
            }
            Msg::SetRemoteSyncKinds(remote_id, sync_kinds) => {
                if let Some(ref mut show) = self.show
                    && let Some(entry) = show.remotes.iter_mut().find(|e| e.id == remote_id)
                {
                    entry.sync_kinds = sync_kinds;
                }

                let id = ctx.props().show_id;

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

                Ok(true)
            }
            Msg::SetRemoteSyncKindsDone(result) => {
                result.context(Message::SettingSyncSource)?;
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
                if let Some(ref mut show) = self.show {
                    show.remotes
                        .sort_by_key(|e| remote_ids.iter().position(|id| *id == e.id));
                }

                let id = ctx.props().show_id;

                self._reorder_remotes_req = self
                    .channel
                    .request()
                    .body(api::ReorderShowRemotesRequest { id, remote_ids })
                    .on_packet(ctx.link().callback(Msg::ReorderRemotesDone))
                    .send();

                Ok(true)
            }
            Msg::ReorderRemotesDone(result) => {
                result.context(Message::SettingSyncSource)?;
                Ok(true)
            }
            Msg::SetLanguage(language) => {
                let id = ctx.props().show_id;

                self._set_language_req = self
                    .channel
                    .request()
                    .body(api::SetShowLanguageRequest {
                        id,
                        language: language.clone(),
                    })
                    .on_packet(
                        ctx.link()
                            .callback(move |r| Msg::SetLanguageDone(language.clone(), r)),
                    )
                    .send();

                Ok(false)
            }
            Msg::SetLanguageDone(language, result) => {
                result.context(Message::SettingLanguage)?;
                if let Some(ref mut show) = self.show {
                    show.language = language;
                }
                Ok(true)
            }
            Msg::OpenImageModal => {
                self.image_modal = true;
                self.settings_modal = false;
                self.view_orphaned = false;
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
                    .context(Message::SyncingShow)?
                    .decode()
                    .context(Message::SyncingShow)?;

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
            Msg::SelectSeasonImage(kind, id) => {
                if let Some(items) = self.season_graphics.get_mut(&kind) {
                    for item in items {
                        item.selected = item.id == id;
                    }
                }

                self._select_season_image_req = self
                    .channel
                    .request()
                    .body(api::SelectImageRequest { id })
                    .on_packet(ctx.link().callback(Msg::SelectSeasonImageDone))
                    .send();

                Ok(true)
            }
            Msg::SelectSeasonImageDone(result) => {
                result.context(Message::SyncingShow)?;
                self.season_image_modal = false;
                Ok(true)
            }
            Msg::ClearSelectedSeasonImage(kind) => {
                if let Some(items) = self.season_graphics.get_mut(&kind) {
                    for item in items {
                        item.selected = false;
                    }
                }

                if let Some(season) = self.selected() {
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
                result.context(Message::SyncingShow)?;
                self.season_image_modal = false;
                Ok(true)
            }
            Msg::OpenSettingsModal => {
                self.settings_modal = true;
                self.view_orphaned = false;
                Ok(true)
            }
            Msg::CloseSettingsModal => {
                self.settings_modal = false;
                Ok(true)
            }
            Msg::SetIncludeSpecials(include_specials) => {
                let id = ctx.props().show_id;

                self._set_include_specials_req = self
                    .channel
                    .request()
                    .body(api::SetShowIncludeSpecialsRequest {
                        id,
                        include_specials,
                    })
                    .on_packet(
                        ctx.link()
                            .callback(move |r| Msg::SetIncludeSpecialsDone(include_specials, r)),
                    )
                    .send();

                Ok(false)
            }
            Msg::SetIncludeSpecialsDone(include_specials, result) => {
                result.context(Message::SettingLanguage)?;
                if let Some(ref mut show) = self.show {
                    show.include_specials = include_specials;
                }
                Ok(true)
            }
            Msg::SetAirDateFilters(air_date_filters) => {
                if let Some(ref mut show) = self.show {
                    show.air_date_filters = air_date_filters.clone();
                }

                let id = ctx.props().show_id;

                self._set_air_date_filters_req = self
                    .channel
                    .request()
                    .body(api::SetShowAirDateFiltersRequest {
                        id,
                        air_date_filters,
                    })
                    .on_packet(ctx.link().callback(Msg::SetAirDateFiltersDone))
                    .send();

                Ok(true)
            }
            Msg::SetAirDateFiltersDone(result) => {
                result.context(Message::SettingLanguage)?;
                Ok(false)
            }
            Msg::OpenRemoteEditor => {
                self.remote_editor = true;
                self.settings_modal = false;
                self.view_orphaned = false;
                Ok(true)
            }
            Msg::CloseRemoteEditor => {
                self.remote_editor = false;
                Ok(true)
            }
            Msg::AddRemote(slug, remote) => {
                let id = ctx.props().show_id;

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

                Ok(false)
            }
            Msg::EditRemote(remote_id, slug, remote) => {
                let id = ctx.props().show_id;

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

                Ok(false)
            }
            Msg::RemoveRemote(remote_id) => {
                let id = ctx.props().show_id;

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

                Ok(false)
            }
            Msg::RemoteDone(result) => {
                result.context(Message::EditingRemotes)?;
                self.load_show(ctx);
                Ok(false)
            }
            Msg::SetTz(tz) => {
                self.tz = tz;
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
                let show_id = ctx.props().show_id;
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
                Ok(false)
            }
            Msg::MoveWatchedDone(result) => {
                result.context(Message::MarkingWatched)?;

                if let Some(season) = self.selected() {
                    self.load_episodes(ctx, season.season);
                    self.load_history(ctx);
                }

                // Refresh season counts so the progress bars reflect the move.
                self.load_seasons(ctx);
                self.load_orphaned(ctx);
                Ok(false)
            }
            Msg::OrphanedLoaded(result) => {
                self.orphaned = result
                    .context(Message::LoadingWatched)?
                    .decode()
                    .context(Message::LoadingWatched)?
                    .watched;

                Ok(true)
            }
            Msg::ToggleActionsExpanded => {
                self.actions_expanded = !self.actions_expanded;
                self.view_orphaned = false;
                Ok(true)
            }
            Msg::ToggleEpisodeActionsExpanded(episode_id) => {
                if !self.episode_actions_expanded.insert(episode_id) {
                    self.episode_actions_expanded.remove(&episode_id);
                    self.history_expanded.remove(&episode_id);
                }

                Ok(true)
            }
            Msg::ToggleOrphaned => {
                self.view_orphaned = !self.view_orphaned;
                Ok(true)
            }
        }
    }

    fn selected(&self) -> Option<&api::Season> {
        self.selected.and_then(|i| self.seasons.get(i))
    }

    fn load_show(&mut self, ctx: &Context<Self>) {
        self._show_req = self
            .channel
            .request()
            .body(api::GetShowRequest {
                id: ctx.props().show_id,
            })
            .on_packet(ctx.link().callback(Msg::ShowLoaded))
            .send();
    }

    fn update_graphics(&mut self) {
        self.graphics.clear();

        if let Some(ref show) = self.show {
            for i in &show.images {
                self.graphics.entry(i.kind).or_default().push(ImageItem {
                    selected: show.is_selected(i.kind, i.image.key()),
                    id: i.id,
                    kind: i.kind,
                    source: i.source,
                    image: i.image.clone(),
                });
            }
        }
    }

    fn load_seasons(&mut self, ctx: &Context<Self>) {
        self._seasons_req = self
            .channel
            .request()
            .body(api::ListSeasonsRequest {
                show_id: ctx.props().show_id,
            })
            .on_packet(ctx.link().callback(Msg::SeasonsLoaded))
            .send();
    }

    fn load_season_images(&mut self, ctx: &Context<Self>, season_id: api::SeasonId) {
        self._season_images_req = self
            .channel
            .request()
            .body(api::GetSeasonImagesRequest { season_id })
            .on_packet(ctx.link().callback(Msg::SeasonImagesLoaded))
            .send();
    }

    fn load_episodes(&mut self, ctx: &Context<Self>, season: api::SeasonNumber) {
        self._episodes_req = self
            .channel
            .request()
            .body(api::ListEpisodesRequest {
                show_id: ctx.props().show_id,
                season,
            })
            .on_packet(ctx.link().callback(Msg::EpisodesLoaded))
            .send();
    }

    fn load_history(&mut self, ctx: &Context<Self>) {
        let show_id = ctx.props().show_id;

        self._watched_req = self
            .channel
            .request()
            .body(api::ListEpisodesWatchedRequest { show_id })
            .on_packet(ctx.link().callback(Msg::WatchedLoaded))
            .send();
    }

    fn load_orphaned(&mut self, ctx: &Context<Self>) {
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
                <Image class="poster hide-mobile" src={poster.cloned()} />

                <div class="table">
                    { for self.seasons.iter().map(|s| self.view_season(ctx, s, self.seasons.len())) }
                </div>
            </div>
        }
    }

    fn view_image_modal(&self, ctx: &Context<Self>) -> Html {
        let link = ctx.link();

        html! {
            <Modal title="Show Graphics" on_close={link.callback(|_| Msg::CloseImageModal)}>
                {for self.graphics.iter().map(|(&kind, items)| {
                    html! {
                        <ImageGallery
                            items={items.clone()}
                            {kind}
                            on_select={link.callback(move |id| Msg::SelectImage(kind, id))}
                            on_clear={link.callback(move |_| Msg::ClearSelectedImage(kind))}
                        />
                    }
                })}
            </Modal>
        }
    }

    fn view_season_image_modal(&self, ctx: &Context<Self>) -> Html {
        let link = ctx.link();

        html! {
            <Modal title="Season Graphics" on_close={link.callback(|_| Msg::CloseSeasonImageModal)}>
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

        let style = if s.total_count > 0 {
            let frac = (s.watched_count.min(s.total_count) as f64 * 100.0) / s.total_count as f64;

            Some(format!("width: {frac:.0}%"))
        } else {
            None
        };

        html! {
            <div class={classes!("table-entry", "column", clickable.then_some("clickable"), active.then_some("active"), (!active && !self.expanded_seasons).then_some("hide-mobile"))} {onclick}>
                <div class="row-fill fill">
                    <span>
                        if let Some(ref name) = s.name {
                            {name}
                        } else {
                            {s.season.long().to_string()}
                        }
                    </span>

                    <div class="row">
                        if let Some(ts) = s.air_date {
                            <span class="text-muted">{ts.date(self.tz.clone()).year().to_string()}</span>
                        }

                        if clickable {
                            <span class="item-inline">
                                <span class={classes!("icon", if active { "ellipsis-horizontal" } else { "chevron-right" })} />
                            </span>
                        }
                    </div>
                </div>

                <div class="percentage-container">
                    <span class="percentage-fill" {style} />
                </div>
            </div>
        }
    }

    fn view_episodes(&self, ctx: &Context<Self>, season: &api::Season) -> Html {
        let link = ctx.link();

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

        let header = 'header: {
            if let Some((ref label, episode_id)) = self.confirming_pending_header {
                break 'header html! {
                    <MarkWatchedPicker
                        prompt={format!("Pending {label} since when?")}
                        on_confirm={link.callback(move |mark_time| Msg::OnWatchNext(episode_id, mark_time))}
                        on_cancel={link.callback(move |_| Msg::CancelWatchNext(episode_id))}
                    />
                };
            }

            let next_unwatched = self.next_unwatched.as_ref().map(|&(label, episode_id)| {
                let callback = link.callback({
                    let label = label.clone();
                    move |_| Msg::AskWatchNextHeader(label, episode_id)
                });

                (label, callback)
            });

            let pending_episode = self.pending_episode.as_ref().map(|&(label, episode_id)| {
                let callback = link.callback(move |_| Msg::OnRemoveNext(episode_id));
                (label, callback)
            });

            html! {
                <>
                    <button class="btn" onclick={link.callback(|_| Msg::OpenSeasonImageModal)} title="Season Graphics">
                        <span class="icon photo" />
                        <span class="hide-mobile">{"Graphics"}</span>
                    </button>

                    if !self.orphaned.is_empty() {
                        <button class="btn-danger" onclick={link.callback(|_| Msg::ToggleOrphaned)} title="View orphaned watched episodes">
                            <span class={classes!("icon", if self.view_orphaned { "ellipsis-horizontal" } else { "exclamation-triangle" })} />

                            if !self.view_orphaned {
                                <span class="hide-mobile">{"Show orphaned watches"}</span>
                            }
                        </button>
                    }

                    if let Some((label, on_remove_next)) = pending_episode {
                        <a class="btn-primary" href={format!("#{label}")} title="Jump to pending episode">
                            <span class="icon bookmark" />
                            <span class="icon chevron-down" />
                        </a>

                        <button class="btn-danger" onclick={on_remove_next} title="Remove pending">
                            <span class="icon bookmark" />
                            <span>{label}</span>
                        </button>
                    } else if let Some((label, onclick)) = next_unwatched {
                        <button class="btn" title="Make next episode" {onclick}>
                            <span class="icon bookmark-slash" />
                            <span>{label}</span>
                        </button>
                    }

                    if !self.view_orphaned && watched_count < total {
                        <button class="btn-success" onclick={link.callback(move |_| Msg::MarkRemainingWatch)} title="Mark remaining episodes as watched">
                            <span class="icon check" />
                            <span class="hide-mobile">{"Remaining"}</span>
                        </button>
                    }
                </>
            }
        };

        let actions = 'actions: {
            if self.select_mark_remaining {
                let season = season.season;

                break 'actions html! {
                    <MarkWatchedPicker
                        prompt="Watched when?"
                        icon_class="item-inline-lg"
                        on_confirm={link.callback(move |mark_time| Msg::WatchRemaining(season, mark_time))}
                        on_cancel={link.callback(|_| Msg::CancelMarkRemainingWatch)}
                    />
                };
            }

            html! {
                <div class="column fill">
                    if self.view_orphaned {
                        <h2>{format!("{} orphaned episodes", self.orphaned.len())}</h2>
                    } else {
                        <h2 class="hide-mobile">
                            if let Some(ref name) = season.name {
                                {name}
                            } else {
                                {season.season.long().to_string()}
                            }
                        </h2>
                    }

                    if let Some(ref overview) = season.overview {
                        <p class="overview">{overview}</p>
                    }

                    <div class="row-fill">
                        if total > 0 {
                            <h4>{format!("{watched_count} / {total} watched")}</h4>
                        }

                        if self.view_orphaned || (!self.orphaned.is_empty() || watched_count < total) {
                            <div class="row end">
                                <div class="input-group">
                                    {header}
                                </div>
                            </div>
                        }
                    </div>
                </div>
            }
        };

        html! {
            <div class="detail-content">
                <div class="row actions">
                    {actions}
                </div>

                if self.episodes.is_empty() && self.selected.is_some() {
                    <div class="text-muted">{"No episodes."}</div>
                }

                if self.view_orphaned {
                    { self.view_orphaned(ctx) }
                } else {
                    <div class="episodes">
                        { for self.episodes.iter().map(|ep| self.view_episode(ctx, ep)) }
                    </div>
                }
            </div>
        }
    }

    fn view_episode(&self, ctx: &Context<Self>, episode: &api::Episode) -> Html {
        let link = ctx.link();

        let show_id = ctx.props().show_id;
        let episode_id = episode.id;

        let watched = self
            .watched_by_episode
            .get(&episode_id)
            .map(Vec::as_slice)
            .unwrap_or_default();

        let history_expanded = self.history_expanded.contains(&episode_id);
        let confirming_mark = self.confirming_mark_watch == Some(episode_id);
        let confirming_pending = self.confirming_pending == Some(episode_id);
        let on_ask_mark = link.callback(move |_| Msg::AskMarkWatched(episode_id));
        let on_toggle_history =
            (!watched.is_empty()).then(|| link.callback(move |_| Msg::ToggleHistory(episode_id)));

        // If the episode has already aired, let the user pick whether the
        // pending slot is dated now or at the air date; otherwise just set it.
        let now = api::Timestamp::now();

        let aired_in_past = episode.aired.is_some_and(|a| a <= now);

        let actions_expanded = self.episode_actions_expanded.contains(&episode_id);

        let toggle_pending = move |mobile: bool| {
            let on_remove_next = link.callback(move |_| Msg::OnRemoveNext(episode_id));

            let on_watch_next = if aired_in_past {
                link.callback(move |_| Msg::AskWatchNext(episode_id))
            } else {
                link.callback(move |_| Msg::OnWatchNext(episode_id, api::MarkTime::WhenAired))
            };

            html! {
                if episode.pending {
                    <button class="btn-primary" onclick={on_remove_next} title="Next episode">
                        <span class="icon bookmark" />
                        <span class={classes!(mobile.then_some("hide-mobile"), "hide-desktop")}>{"Next episode"}</span>
                    </button>
                } else {
                    <button class="btn" onclick={on_watch_next} title="Not next episode">
                        <span class="icon bookmark-slash" />
                        <span class={classes!(mobile.then_some("hide-mobile"), "hide-desktop")}>{"Not next episode"}</span>
                    </button>
                }
            }
        };

        let main_actions = html! {
            <>
                if !history_expanded {
                    <button class="btn-success" onclick={on_ask_mark.clone()} title="Mark watched">
                        <span class="icon check" />
                        <span class="hide-desktop">{"Mark watched"}</span>
                    </button>
                }

                {toggle_pending(false)}

                if let Some(on_toggle) = on_toggle_history {
                    <button class="btn" onclick={on_toggle} title={if history_expanded { "Hide watch history" } else { "Show watch history" }}>
                        <span class={classes!("icon", if history_expanded { "ellipsis-horizontal" } else { "clock" })} />
                        <span class="hide-desktop">{if history_expanded { "History" } else { "Show history" }}</span>
                    </button>
                }
            </>
        };

        let actions = 'actions: {
            if confirming_mark {
                break 'actions html! {
                    <MarkWatchedPicker
                        prompt={format!("When did you watch {}?", episode.code())}
                        icon_class="item-inline-lg"
                        on_confirm={link.callback(move |mark_time| Msg::MarkWatched(show_id, episode_id, mark_time))}
                        on_cancel={link.callback(move |_| Msg::CancelMarkWatch(episode_id))}
                    />
                };
            }

            if confirming_pending {
                break 'actions html! {
                    <MarkWatchedPicker
                        prompt="Pending since when?"
                        icon_class="item-inline-lg"
                        on_confirm={link.callback(move |mark_time| Msg::OnWatchNext(episode_id, mark_time))}
                        on_cancel={link.callback(move |_| Msg::CancelWatchNext(episode_id))}
                    />
                };
            }

            html! {
                <div class="actions row-fill">
                    <div class="column fill">
                        <div class="row-fill">
                            <a class="episode-code">{episode.code()}</a>

                            <div class="row">
                                if episode.pending {
                                    <span class="item-inline-lg" title="Next episode"><span class="icon primary exclamation-circle" /></span>
                                } else if !watched.is_empty() {
                                    <span class="item-inline-lg" title="Watched"><span class="icon primary check-circle" /></span>
                                } else {
                                    <span class="item-inline-lg" title="Never watched"><span class="icon secondary x-circle" /></span>
                                }

                                if episode.pending {
                                    <span class="text-muted">{"Next episode"}</span>
                                } else {
                                    <span class="text-muted">
                                        {match watched {
                                            [] => "Never watched".to_string(),
                                            [w] => format!("Watched at {}", w.timestamp.display(self.tz.clone())),
                                            [first, ..] => format!("Watched {} times, first at {}", watched.len(), first.timestamp.display(self.tz.clone())),
                                        }}
                                    </span>
                                }
                            </div>

                            <div class="row end">
                                <div class="hide-desktop">
                                    <div class="input-group">
                                        {toggle_pending(true)}

                                        <button class="btn" onclick={link.callback(move |_| Msg::ToggleEpisodeActionsExpanded(episode_id))}>
                                            <span class={classes!("icon", if actions_expanded { "ellipsis-horizontal" } else { "bars-2" })} />
                                        </button>
                                    </div>
                                </div>

                                <div class="hide-mobile">
                                    <div class="input-group">
                                        {main_actions.clone()}
                                    </div>
                                </div>
                            </div>
                        </div>

                        if !history_expanded {
                            <div class={classes!("column", "hide-desktop", (!actions_expanded).then_some("hide-mobile"))}>
                                {main_actions.clone()}
                            </div>
                        }
                    </div>
                </div>
            }
        };

        html! {
            <div class={classes!("episode", (!watched.is_empty()).then_some("watched"))} id={episode.code()}>
                {actions}

                <div class="desktop-row mobile-column">
                    <Image class="screenshot" src={episode.screenshot.clone()} />

                    <div class="column top desktop-fill">
                        if !history_expanded {
                            <h3>{ episode.name.as_deref().unwrap_or(MDASH) }</h3>

                            if let Some(aired) = episode.display_at(self.tz.clone()) {
                                <span class="text-muted">{aired}</span>
                            }

                            if let Some(ref overview) = episode.overview {
                                <p class="overview">{overview}</p>
                            }
                        }

                        if history_expanded {
                            <div class="column">
                                <h3>{"Watch history"}</h3>

                                <div class="column">
                                    { for watched.iter().map(|w| {
                                        let wid = w.id;
                                        let kind = api::WatchedKind::Episode { show: show_id, episode: episode_id };

                                        if self.confirm_remove_watch == Some(wid) {
                                            html! {
                                                <ConfirmDanger
                                                    prompt="Remove watch at"
                                                    label={w.timestamp.display(self.tz.clone())}
                                                    on_confirm={link.callback(move |_| Msg::RemoveWatched(wid, kind))}
                                                    on_cancel={link.callback(|_| Msg::CancelRemoveWatch)}
                                                />
                                            }
                                        } else if self.fixing_watched == Some(wid) {
                                            html! {
                                                <EpisodePicker
                                                    prompt="Move watch at"
                                                    label={w.timestamp.display(self.tz.clone())}
                                                    show_id={show_id}
                                                    seasons={self.seasons.clone()}
                                                    selected_season={episode.season}
                                                    selected_episode={episode.episode}
                                                    on_confirm={link.callback(move |(season, ep)| Msg::MoveWatched(wid, season, ep))}
                                                    on_cancel={link.callback(|_| Msg::CancelFixWatched)}
                                                />
                                            }
                                        } else {
                                            html! {
                                                <div class="row-fill">
                                                    <div class="row">
                                                        <span>{w.timestamp.display(self.tz.clone())}</span>
                                                    </div>

                                                    <div class="row end">
                                                        <div class="input-group">
                                                            <button class="btn" onclick={link.callback(move |_| Msg::FixWatched(wid))} title="Move to different episode">
                                                                <span class="icon pencil-square" />
                                                                <span>{"Move"}</span>
                                                            </button>

                                                            <button class="btn-danger" onclick={link.callback(move |_| Msg::ConfirmRemoveWatch(wid))} title="Remove">
                                                                <span class="icon trash" />
                                                            </button>
                                                        </div>
                                                    </div>
                                                </div>
                                            }
                                        }
                                    }) }
                                </div>
                            </div>
                        }
                    </div>
                </div>
            </div>
        }
    }

    fn view_orphaned(&self, ctx: &Context<Self>) -> Html {
        if self.orphaned.is_empty() {
            return html! {};
        }

        let link = ctx.link();
        let show_id = ctx.props().show_id;

        // The first unwatched episode in the current season is the default
        // selected value.
        let selected_episode = || {
            self.selected().and_then(|season| {
                for e in self.episodes.iter().filter(|ep| ep.season == season.season) {
                    let Some(watched) = self.watched_by_episode.get(&e.id) else {
                        return Some(e.episode);
                    };

                    if watched.is_empty() {
                        return Some(e.episode);
                    }
                }

                None
            })
        };

        html! {
            <div class="column">
                <div class="table">
                    { for self.orphaned.iter().map(|w| {
                        let wid = w.id;

                        let kind = api::WatchedKind::Episode { show: show_id, episode: api::EpisodeId::new(0) };

                        if self.fixing_watched == Some(wid) {
                            html! {
                                <div class="table-entry">
                                    <EpisodePicker
                                        prompt="Move watch at"
                                        label={w.timestamp.display(self.tz.clone())}
                                        {show_id}
                                        seasons={self.seasons.clone()}
                                        selected_season={self.selected().map(|s| s.season)}
                                        selected_episode={selected_episode()}
                                        on_confirm={link.callback(move |(season, ep)| Msg::MoveWatched(wid, season, ep))}
                                        on_cancel={link.callback(|_| Msg::CancelFixWatched)}
                                    />
                                </div>
                            }
                        } else if self.confirm_remove_watch == Some(wid) {
                            html! {
                                <ConfirmDanger
                                    prompt="Remove watch at"
                                    label={w.timestamp.display(self.tz.clone())}
                                    on_confirm={link.callback(move |_| Msg::RemoveWatched(wid, kind))}
                                    on_cancel={link.callback(|_| Msg::CancelRemoveWatch)}
                                />
                            }
                        } else {
                            html! {
                                <div class="row-fill">
                                    <div class="row">
                                        <span class="text-muted">{w.code()}</span>
                                        <span>{w.timestamp.display(self.tz.clone())}</span>
                                    </div>

                                    <div class="end input-group">
                                        <button class="btn" onclick={link.callback(move |_| Msg::FixWatched(wid))} title="Move to episode">
                                            <span class="icon pencil-square" />
                                        </button>

                                        <button class="btn-danger" onclick={link.callback(move |_| Msg::ConfirmRemoveWatch(wid))} title="Remove">
                                            <span class="icon trash" />
                                        </button>
                                    </div>
                                </div>
                            }
                        }
                    }) }
                </div>
            </div>
        }
    }
}
