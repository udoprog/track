use api::TimeInfo;
use musli_web::web03::prelude::*;
use std::collections::{BTreeMap, HashSet};
use yew::prelude::*;

use crate::SetupChannel;
use crate::background::Background;
use crate::error::{CustomContext, Error, Message};
use crate::router::{MediaQuery, Route, Router};
use crate::ui::{
    ConfirmDanger, ContextMenu, Image, ImageGallery, ImageItem, Loading, MarkTimeMenu,
    MediaSettingsModal, Modal, RemoteEditor, RemoteSourceKind, TimePreset, Tracked,
    TranslationsModal,
};

struct WatchedState {
    remove_watch_anchor: NodeRef,
    watched: api::Watched,
}

pub(crate) struct MovieDetail {
    channel: ws::Channel,
    movie: Option<api::Movie>,
    graphics: BTreeMap<api::ImageKind, Vec<ImageItem>>,
    watched: Vec<WatchedState>,
    confirm_remove: bool,
    remove_anchor: NodeRef,
    confirm_remove_watch: Option<api::WatchedId>,
    syncing: bool,
    actions_expanded: bool,
    detailed_expand: bool,
    open_watched: bool,
    open_releases: bool,
    releases_expanded: HashSet<api::ReleaseType>,
    movie_releases: Vec<(api::ReleaseType, Vec<api::MovieRelease>)>,
    default_release_filters: Vec<api::ReleaseFilter>,
    global_sync_kinds: Vec<api::SourceSyncKinds>,
    image_modal: bool,
    settings_modal: bool,
    remote_editor: bool,
    translations_modal: bool,
    background: Background,
    router: Router,
    time: TimeInfo,
    _time_handle: ContextHandle<TimeInfo>,
    _setup: SetupChannel,
    _broadcast: ws::Listener,
    _movie_req: ws::Request,
    _watched_req: ws::Request,
    _mark_req: ws::Request,
    _remove_watch_req: ws::Request,
    _remove_req: ws::Request,
    _sync_req: ws::Request,
    _untrack_req: ws::Request,
    _pending_req: ws::Request,
    _select_image_req: ws::Request,
    _clear_image_req: ws::Request,
    _set_remote_enabled_req: ws::Request,
    _reorder_remotes_req: ws::Request,
    _set_remote_sync_kinds_req: ws::Request,
    _set_language_req: ws::Request,
    _set_release_filters_req: ws::Request,
    _set_auto_sync_req: ws::Request,
    _config_req: ws::Request,
    _remote_req: ws::Request,
}

pub(crate) enum Msg {
    Channel(Result<ws::Channel, ws::Error>),
    AppBroadcast(Result<ws::Packet<api::AppBroadcast>, ws::Error>),
    MovieLoaded(Result<ws::Packet<api::GetMovie>, ws::Error>),
    WatchedLoaded(Result<ws::Packet<api::ListWatched>, ws::Error>),
    MarkWatched(api::MarkTime),
    MarkWatchedDone(Result<ws::Packet<api::MarkWatched>, ws::Error>),
    RemoveWatched(api::WatchedId, api::WatchedKind),
    RemoveWatchedDone(Result<ws::Packet<api::RemoveWatched>, ws::Error>),
    ConfirmRemoveWatch(api::WatchedId),
    CancelRemoveWatch,
    SelectImage(api::ImageKind, api::ImageId),
    ClearSelectedImage(api::ImageKind),
    SelectImageDone(Result<ws::Packet<api::SelectImage>, ws::Error>),
    ClearSelectedImageDone(Result<ws::Packet<api::ClearSelectedImage>, ws::Error>),
    OpenImageModal,
    CloseImageModal,
    OpenSettingsModal,
    CloseSettingsModal,
    OpenTranslations,
    CloseTranslations,
    OpenRemoteEditor,
    CloseRemoteEditor,
    AddRemote(Option<String>, api::Remote),
    EditRemote(api::RemoteId, Option<String>, api::Remote),
    RemoveRemote(api::RemoteId),
    RemoteDone(Result<(), ws::Error>),
    ConfirmRemove,
    CancelRemove,
    RemoveMovie,
    RemoveDone(Result<ws::Packet<api::RemoveMovie>, ws::Error>),
    SyncMovie,
    SyncDone(Result<ws::Packet<api::SyncMovie>, ws::Error>),
    SetRemoteEnabled(api::RemoteId, bool),
    SetRemoteEnabledDone(Result<ws::Packet<api::SetMovieRemoteEnabled>, ws::Error>),
    SetRemoteSyncKinds(api::RemoteId, Option<api::SyncKindSet>),
    SetRemoteSyncKindsDone(Result<ws::Packet<api::SetMovieRemoteSyncKinds>, ws::Error>),
    ReorderRemotes(Vec<api::RemoteId>),
    ReorderRemotesDone(Result<ws::Packet<api::ReorderMovieRemotes>, ws::Error>),
    SetLanguage(api::Locale),
    SetLanguageDone(
        api::Locale,
        Result<ws::Packet<api::SetMovieLanguage>, ws::Error>,
    ),
    ConfigLoaded(Result<ws::Packet<api::GetConfig>, ws::Error>),
    SetReleaseFilters(Option<Vec<api::ReleaseFilter>>),
    SetReleaseFiltersDone(
        Option<Vec<api::ReleaseFilter>>,
        Result<ws::Packet<api::SetMovieReleaseFilters>, ws::Error>,
    ),
    SetAutoSync(bool),
    SetAutoSyncDone(bool, Result<ws::Packet<api::SetMovieAutoSync>, ws::Error>),
    SetTracked(bool),
    SetTrackedDone(bool, Result<ws::Packet<api::UntrackMovie>, ws::Error>),
    OnWatchNext(api::MarkTime),
    AddPendingDone(Result<ws::Packet<api::AddPending>, ws::Error>),
    OnRemoveNext,
    RemovePendingDone(Result<ws::Packet<api::RemovePending>, ws::Error>),
    SetTime(TimeInfo),
    ToggleActionsExpanded,
    ToggleDetailedActionsExpanded,
    ToggleReleaseType(api::ReleaseType),
    ToggleOpenWatched,
    ToggleOpenReleases,
}

#[derive(Properties, PartialEq)]
pub(crate) struct Props {
    pub(crate) onerror: Callback<Error>,
    pub(crate) movie_id: api::MovieId,
}

impl Component for MovieDetail {
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
            movie: None,
            graphics: BTreeMap::new(),
            watched: Vec::new(),
            confirm_remove: false,
            remove_anchor: NodeRef::default(),
            confirm_remove_watch: None,
            syncing: false,
            actions_expanded: false,
            detailed_expand: false,
            open_watched: false,
            open_releases: false,
            releases_expanded: HashSet::new(),
            movie_releases: Vec::new(),
            default_release_filters: api::ReleaseFilter::default_filters(),
            global_sync_kinds: Vec::new(),
            image_modal: false,
            settings_modal: false,
            remote_editor: false,
            translations_modal: false,
            background,
            router,
            time,
            _time_handle,
            _setup,
            _broadcast,
            _movie_req: ws::Request::default(),
            _watched_req: ws::Request::default(),
            _mark_req: ws::Request::default(),
            _remove_watch_req: ws::Request::default(),
            _remove_req: ws::Request::default(),
            _sync_req: ws::Request::default(),
            _untrack_req: ws::Request::default(),
            _pending_req: ws::Request::default(),
            _select_image_req: ws::Request::default(),
            _clear_image_req: ws::Request::default(),
            _set_remote_enabled_req: ws::Request::default(),
            _reorder_remotes_req: ws::Request::default(),
            _set_remote_sync_kinds_req: ws::Request::default(),
            _set_language_req: ws::Request::default(),
            _set_release_filters_req: ws::Request::default(),
            _set_auto_sync_req: ws::Request::default(),
            _config_req: ws::Request::default(),
            _remote_req: ws::Request::default(),
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

    fn destroy(&mut self, _ctx: &Context<Self>) {
        self.background.title(None);
    }

    fn view(&self, ctx: &Context<Self>) -> Html {
        let Some(ref movie) = self.movie else {
            return html!(<Loading />);
        };

        html! {
            <>
                { self.view_header(ctx, movie) }

                { self.view_body(ctx, movie) }
            </>
        }
    }

    fn changed(&mut self, ctx: &Context<Self>, old_props: &Props) -> bool {
        if ctx.props().movie_id != old_props.movie_id {
            self.movie = None;
            self.watched.clear();
            self.confirm_remove = false;
            self.confirm_remove_watch = None;

            if self.channel.id() != ws::ChannelId::NONE {
                self.load_movie(ctx);
                self.load_watched(ctx);
            }
        }

        true
    }
}

impl MovieDetail {
    fn try_update(&mut self, ctx: &Context<Self>, msg: Msg) -> Result<bool, Error> {
        match msg {
            Msg::Channel(result) => {
                self.channel = result?;
                if self.channel.id() != ws::ChannelId::NONE {
                    self.load_movie(ctx);
                    self.load_watched(ctx);
                    self.load_config(ctx);
                } else {
                    self.movie = None;
                    self.movie_releases.clear();
                    self.watched.clear();
                }
                Ok(true)
            }
            Msg::AppBroadcast(packet) => {
                let event = packet?.decode_event()?;

                if event.channel == self.channel.id() {
                    return Ok(false);
                }

                match &event.kind {
                    api::AppEventKind::MovieChanged { movie }
                        if movie.id == ctx.props().movie_id =>
                    {
                        self.set_movie(movie.clone());
                        Ok(true)
                    }
                    api::AppEventKind::MovieDeleted { movie_id }
                        if *movie_id == ctx.props().movie_id =>
                    {
                        self.router.push(Route::Media(MediaQuery::default()));
                        Ok(false)
                    }
                    api::AppEventKind::WatchedChanged { event: kind } => {
                        let relevant = matches!(
                            kind,
                            api::WatchedEvent::Movie { movie } if *movie == ctx.props().movie_id
                        );

                        if relevant && self.channel.id() != ws::ChannelId::NONE {
                            self.load_movie(ctx);
                            self.load_watched(ctx);
                        }

                        Ok(false)
                    }
                    api::AppEventKind::TaskAdded { task }
                    | api::AppEventKind::TaskStarted { task } => {
                        if matches!(&task.kind, api::TaskKind::SyncMovie { movie_id, .. } if *movie_id == ctx.props().movie_id)
                        {
                            self.syncing = true;
                            return Ok(true);
                        }
                        Ok(false)
                    }
                    api::AppEventKind::TaskCompleted { task } => {
                        if matches!(&task.kind, api::TaskKind::SyncMovie { movie_id, .. } if *movie_id == ctx.props().movie_id)
                        {
                            self.syncing = false;
                            self.load_movie(ctx);
                            return Ok(true);
                        }
                        Ok(false)
                    }
                    _ => Ok(false),
                }
            }
            Msg::MovieLoaded(result) => {
                let movie = result
                    .context(Message::LoadingMovies)?
                    .decode()
                    .context(Message::LoadingMovies)?;

                self.set_movie(movie);
                Ok(true)
            }
            Msg::WatchedLoaded(result) => {
                let watched = result
                    .context(Message::LoadingWatched)?
                    .decode()
                    .context(Message::LoadingWatched)?
                    .watched;

                self.watched.clear();

                for w in watched {
                    self.watched.push(WatchedState {
                        remove_watch_anchor: NodeRef::default(),
                        watched: w,
                    });
                }

                Ok(true)
            }
            Msg::MarkWatched(mark_time) => {
                let movie = ctx.props().movie_id;
                self._mark_req = self
                    .channel
                    .request()
                    .body(api::MarkWatchedRequest {
                        kind: api::WatchedKind::Movie { movie },
                        mark_time,
                    })
                    .on_packet(ctx.link().callback(Msg::MarkWatchedDone))
                    .send();
                Ok(true)
            }
            Msg::MarkWatchedDone(result) => {
                result.context(Message::MarkingWatched)?;
                self.load_movie(ctx);
                self.load_watched(ctx);
                Ok(false)
            }
            Msg::RemoveWatched(id, kind) => {
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
                self.load_movie(ctx);
                self.load_watched(ctx);
                Ok(false)
            }
            Msg::ConfirmRemoveWatch(watched_id) => {
                self.confirm_remove_watch = Some(watched_id);
                Ok(true)
            }
            Msg::CancelRemoveWatch => {
                self.confirm_remove_watch = None;
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
            Msg::RemoveMovie => {
                let id = ctx.props().movie_id;
                self._remove_req = self
                    .channel
                    .request()
                    .body(api::RemoveMovieRequest { id })
                    .on_packet(ctx.link().callback(Msg::RemoveDone))
                    .send();
                Ok(false)
            }
            Msg::RemoveDone(result) => {
                result.context(Message::RemovingMovie)?;
                self.router.push(Route::Media(MediaQuery::default()));
                Ok(false)
            }
            Msg::SyncMovie => {
                let id = ctx.props().movie_id;

                self._sync_req = self
                    .channel
                    .request()
                    .body(api::SyncMovieRequest { id })
                    .on_packet(ctx.link().callback(Msg::SyncDone))
                    .send();

                Ok(true)
            }
            Msg::SyncDone(result) => {
                result.context(Message::SyncingMovie)?;
                Ok(false)
            }
            Msg::SetRemoteEnabled(remote_id, enabled) => {
                if let Some(ref mut movie) = self.movie
                    && let Some(entry) = movie.remotes.iter_mut().find(|e| e.id == remote_id)
                {
                    entry.enabled = enabled;
                }

                let id = ctx.props().movie_id;
                self._set_remote_enabled_req = self
                    .channel
                    .request()
                    .body(api::SetMovieRemoteEnabledRequest {
                        id,
                        remote_id,
                        enabled,
                    })
                    .on_packet(ctx.link().callback(Msg::SetRemoteEnabledDone))
                    .send();
                Ok(true)
            }
            Msg::SetRemoteEnabledDone(result) => {
                result.context(Message::SettingRemoteEnabled)?;
                Ok(false)
            }
            Msg::SetRemoteSyncKinds(remote_id, sync_kinds) => {
                if let Some(ref mut movie) = self.movie
                    && let Some(entry) = movie.remotes.iter_mut().find(|e| e.id == remote_id)
                {
                    entry.sync_kinds = sync_kinds;
                }

                let id = ctx.props().movie_id;
                self._set_remote_sync_kinds_req = self
                    .channel
                    .request()
                    .body(api::SetMovieRemoteSyncKindsRequest {
                        id,
                        remote_id,
                        sync_kinds,
                    })
                    .on_packet(ctx.link().callback(Msg::SetRemoteSyncKindsDone))
                    .send();
                Ok(true)
            }
            Msg::SetRemoteSyncKindsDone(result) => {
                result.context(Message::SettingRemoteSyncKinds)?;
                Ok(false)
            }
            Msg::ReorderRemotes(remote_ids) => {
                if let Some(ref mut movie) = self.movie {
                    movie
                        .remotes
                        .sort_by_key(|e| remote_ids.iter().position(|id| *id == e.id));
                }

                let id = ctx.props().movie_id;
                self._reorder_remotes_req = self
                    .channel
                    .request()
                    .body(api::ReorderMovieRemotesRequest { id, remote_ids })
                    .on_packet(ctx.link().callback(Msg::ReorderRemotesDone))
                    .send();
                Ok(true)
            }
            Msg::ReorderRemotesDone(result) => {
                result.context(Message::ReorderingRemotes)?;
                Ok(false)
            }
            Msg::SetLanguage(language) => {
                let id = ctx.props().movie_id;

                self._set_language_req = self
                    .channel
                    .request()
                    .body(api::SetMovieLanguageRequest { id, language })
                    .on_packet(
                        ctx.link()
                            .callback(move |r| Msg::SetLanguageDone(language, r)),
                    )
                    .send();

                Ok(false)
            }
            Msg::SetLanguageDone(language, result) => {
                result.context(Message::SettingLanguage(language))?;

                if let Some(ref mut movie) = self.movie {
                    movie.language = language;
                }

                Ok(true)
            }
            Msg::ConfigLoaded(result) => {
                let config = result
                    .context(Message::LoadingConfig)?
                    .decode()
                    .context(Message::LoadingConfig)?
                    .config;
                self.default_release_filters = config.release_filters;
                self.global_sync_kinds = config.sync_kinds;
                Ok(true)
            }
            Msg::SetReleaseFilters(release_filters) => {
                let id = ctx.props().movie_id;

                self._set_release_filters_req =
                    self.channel
                        .request()
                        .body(api::SetMovieReleaseFiltersRequest {
                            id,
                            release_filters: release_filters.clone(),
                        })
                        .on_packet(ctx.link().callback(move |r| {
                            Msg::SetReleaseFiltersDone(release_filters.clone(), r)
                        }))
                        .send();

                Ok(false)
            }
            Msg::SetReleaseFiltersDone(release_filters, result) => {
                result.context(Message::SettingReleaseFilters)?;

                if let Some(ref mut movie) = self.movie {
                    movie.release_filters = release_filters;
                }

                // The server recomputes the effective release date from the new filters; reload to
                // reflect it (the change broadcast excludes this originating channel).
                self.load_movie(ctx);

                Ok(true)
            }
            Msg::SetAutoSync(auto_sync) => {
                let id = ctx.props().movie_id;

                self._set_auto_sync_req = self
                    .channel
                    .request()
                    .body(api::SetMovieAutoSyncRequest { id, auto_sync })
                    .on_packet(
                        ctx.link()
                            .callback(move |r| Msg::SetAutoSyncDone(auto_sync, r)),
                    )
                    .send();

                Ok(false)
            }
            Msg::SetAutoSyncDone(auto_sync, result) => {
                result.context(Message::SettingAutoSync(auto_sync))?;

                if let Some(ref mut movie) = self.movie {
                    movie.auto_sync = auto_sync;
                }

                Ok(true)
            }
            Msg::SetTracked(tracked) => {
                self.actions_expanded = false;

                let id = ctx.props().movie_id;

                self._untrack_req = self
                    .channel
                    .request()
                    .body(api::UntrackMovieRequest { id, tracked })
                    .on_packet(
                        ctx.link()
                            .callback(move |r| Msg::SetTrackedDone(tracked, r)),
                    )
                    .send();

                Ok(false)
            }
            Msg::SetTrackedDone(tracked, result) => {
                result.context(Message::UntrackingMovie)?;

                if let Some(ref mut movie) = self.movie {
                    movie.tracked = tracked;
                }

                Ok(true)
            }
            Msg::OnWatchNext(mark_time) => {
                let movie = ctx.props().movie_id;
                self._pending_req = self
                    .channel
                    .request()
                    .body(api::AddPendingRequest {
                        kind: api::PendingKind::Movie { movie },
                        mark_time,
                    })
                    .on_packet(ctx.link().callback(Msg::AddPendingDone))
                    .send();
                Ok(true)
            }
            Msg::AddPendingDone(result) => {
                let packet = result
                    .context(Message::AddingPending)?
                    .decode()
                    .context(Message::AddingPending)?;

                if let Some(ref mut movie) = self.movie {
                    movie.pending = packet.pending.map(|p| p.timestamp);
                }

                Ok(true)
            }
            Msg::OnRemoveNext => {
                let movie = ctx.props().movie_id;
                self._pending_req = self
                    .channel
                    .request()
                    .body(api::RemovePendingRequest {
                        kind: api::PendingKind::Movie { movie },
                    })
                    .on_packet(ctx.link().callback(Msg::RemovePendingDone))
                    .send();
                Ok(false)
            }
            Msg::RemovePendingDone(result) => {
                _ = result
                    .context(Message::RemovingPending)?
                    .decode()
                    .context(Message::RemovingPending)?;

                if let Some(ref mut movie) = self.movie {
                    movie.pending = None;
                }

                Ok(true)
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
                        owner: api::ImageOwner::Movie(ctx.props().movie_id),
                        kind,
                    })
                    .on_packet(ctx.link().callback(Msg::ClearSelectedImageDone))
                    .send();

                Ok(false)
            }
            Msg::SelectImageDone(result) => {
                result.context(Message::SelectingImage)?;
                self.image_modal = false;
                self.load_movie(ctx);
                Ok(true)
            }
            Msg::ClearSelectedImageDone(result) => {
                result.context(Message::ClearingImage)?;
                self.image_modal = false;
                self.load_movie(ctx);
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
            Msg::OpenSettingsModal => {
                self.settings_modal = true;
                self.actions_expanded = false;
                Ok(true)
            }
            Msg::CloseSettingsModal => {
                self.settings_modal = false;
                Ok(true)
            }
            Msg::OpenTranslations => {
                self.translations_modal = true;
                self.actions_expanded = false;
                Ok(true)
            }
            Msg::CloseTranslations => {
                self.translations_modal = false;
                Ok(true)
            }
            Msg::OpenRemoteEditor => {
                self.remote_editor = true;
                self.settings_modal = false;
                self.actions_expanded = false;
                Ok(true)
            }
            Msg::CloseRemoteEditor => {
                self.remote_editor = false;
                Ok(true)
            }
            Msg::AddRemote(slug, remote) => {
                let id = ctx.props().movie_id;

                self._remote_req = self
                    .channel
                    .request()
                    .body(api::AddMovieRemoteRequest { id, slug, remote })
                    .on_packet(ctx.link().callback(
                        |r: Result<ws::Packet<api::AddMovieRemote>, ws::Error>| {
                            Msg::RemoteDone(r.map(|_| ()))
                        },
                    ))
                    .send();

                Ok(false)
            }
            Msg::EditRemote(remote_id, slug, remote) => {
                let id = ctx.props().movie_id;

                self._remote_req = self
                    .channel
                    .request()
                    .body(api::UpdateMovieRemoteRequest {
                        id,
                        remote_id,
                        slug,
                        remote,
                    })
                    .on_packet(ctx.link().callback(
                        |r: Result<ws::Packet<api::UpdateMovieRemote>, ws::Error>| {
                            Msg::RemoteDone(r.map(|_| ()))
                        },
                    ))
                    .send();

                Ok(false)
            }
            Msg::RemoveRemote(remote_id) => {
                let id = ctx.props().movie_id;

                self._remote_req = self
                    .channel
                    .request()
                    .body(api::RemoveMovieRemoteRequest { id, remote_id })
                    .on_packet(ctx.link().callback(
                        |r: Result<ws::Packet<api::RemoveMovieRemote>, ws::Error>| {
                            Msg::RemoteDone(r.map(|_| ()))
                        },
                    ))
                    .send();

                Ok(false)
            }
            Msg::RemoteDone(result) => {
                result.context(Message::EditingRemotes)?;
                self.load_movie(ctx);
                Ok(false)
            }
            Msg::SetTime(time) => {
                self.time = time;
                Ok(true)
            }
            Msg::ToggleActionsExpanded => {
                self.actions_expanded = !self.actions_expanded;
                Ok(true)
            }
            Msg::ToggleDetailedActionsExpanded => {
                self.detailed_expand = !self.detailed_expand;
                Ok(true)
            }
            Msg::ToggleReleaseType(ty) => {
                if !self.releases_expanded.remove(&ty) {
                    self.releases_expanded.insert(ty);
                }

                Ok(true)
            }
            Msg::ToggleOpenWatched => {
                self.open_watched = !self.open_watched;
                Ok(true)
            }
            Msg::ToggleOpenReleases => {
                self.open_releases = !self.open_releases;
                Ok(true)
            }
        }
    }

    fn set_movie(&mut self, movie: api::Movie) {
        let mut by_type: BTreeMap<u32, (api::ReleaseType, Vec<api::MovieRelease>)> =
            BTreeMap::new();

        for r in &movie.releases {
            let entry = by_type
                .entry(r.release_type.as_u32())
                .or_insert_with(|| (r.release_type, Vec::new()));

            entry.1.push(r.clone());
        }

        self.movie_releases = by_type.into_values().collect();
        self.background
            .background(movie.backdrop.as_ref().map(|i| i.proxy_url()));
        self.background.title(movie.title.clone());
        self.movie = Some(movie);
        self.update_graphics();
    }

    fn update_graphics(&mut self) {
        self.graphics.clear();

        if let Some(ref movie) = self.movie {
            for i in &movie.images {
                self.graphics.entry(i.kind).or_default().push(ImageItem {
                    selected: movie.is_selected(i.kind, i.image.key()),
                    id: i.id,
                    kind: i.kind,
                    source: i.source,
                    image: i.image.clone(),
                });
            }
        }
    }

    fn load_movie(&mut self, ctx: &Context<Self>) {
        self._movie_req = self
            .channel
            .request()
            .body(api::GetMovieRequest {
                id: ctx.props().movie_id,
            })
            .on_packet(ctx.link().callback(Msg::MovieLoaded))
            .send();
    }

    fn load_watched(&mut self, ctx: &Context<Self>) {
        let movie = ctx.props().movie_id;
        self._watched_req = self
            .channel
            .request()
            .body(api::ListWatchedRequest {
                kind: api::WatchedKind::Movie { movie },
            })
            .on_packet(ctx.link().callback(Msg::WatchedLoaded))
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

    fn view_header(&self, ctx: &Context<Self>, movie: &api::Movie) -> Html {
        let link = ctx.link();

        html! {
            <div class="column">
                <div class="mobile-row desktop-column desktop-center">
                    <h1>{movie.title.as_deref().unwrap_or("Untitled Movie")}</h1>

                    if let Some(ts) = movie.release_date {
                        <span class="text-muted">{ts.date(self.time.clone()).year()}</span>
                    }
                </div>

                <div class="toolbar">
                    <div class="row mobile-justify-around mobile-flex-wrap">
                        {for movie.remotes.iter().filter_map(|r| {
                            let url = r.remote.movie_url()?;
                            let id = r.remote.source().as_id();

                            Some(html! {
                                <a class="item-inline-source" href={url} target="_blank" rel="noopener noreferrer" title={format!("Open on {id}")}>
                                    <span class={classes!("logo", id)} />
                                </a>
                            })
                        })}
                    </div>

                    <div class="toolbar-toggle">
                        <button class="btn" onclick={link.callback(|_| Msg::ToggleActionsExpanded)}>
                            <span class={classes!("icon", if self.actions_expanded { "ellipsis-horizontal" } else { "bars-3" })} />
                        </button>
                    </div>

                    <div class={classes!("toolbar-dropdown", (!self.actions_expanded).then_some("hide-mobile"))}>
                        <div class="desktop-row mobile-column desktop-input-group">
                            <Tracked tracked={movie.tracked} ontoggle={link.callback(Msg::SetTracked)} />

                            if !movie.remotes.is_empty() {
                                <button class="btn" onclick={link.callback(|_| Msg::SyncMovie)} title="Sync now">
                                    <span class={classes!("icon", "arrow-path", self.syncing.then_some("spin"))} />
                                    <span class="hide-desktop">{"Sync"}</span>
                                </button>
                            }

                            <button class="btn" onclick={link.callback(|_| Msg::OpenTranslations)} title="Translations">
                                <span class="icon language" />
                                <span class="hide-desktop">{"Translations"}</span>
                            </button>

                            <button class="btn" onclick={link.callback(|_| Msg::OpenSettingsModal)} title="Settings">
                                <span class="icon cog-6-tooth" />
                                <span class="hide-desktop">{"Settings"}</span>
                            </button>

                            <button ref={self.remove_anchor.clone()} class="btn-danger" onclick={link.callback(|_| Msg::ConfirmRemove)} title="Remove movie">
                                <span class="icon trash" />
                                <span class="hide-desktop">{"Remove"}</span>
                            </button>

                            if self.confirm_remove {
                                <ContextMenu
                                    prompt="Remove movie"
                                    label={movie.title.clone()}
                                    anchor={self.remove_anchor.clone()}
                                    on_close={link.callback(|_| Msg::CancelRemove)}
                                    onerror={ctx.props().onerror.clone()}>
                                    <ConfirmDanger
                                        on_confirm={link.callback(|_| Msg::RemoveMovie)}
                                        on_cancel={link.callback(|_| Msg::CancelRemove)}
                                    />
                                </ContextMenu>
                            }
                        </div>
                    </div>
                </div>

                <indicator title="Release date">
                    <span class="item-inline">
                        <span class={classes!("icon", if movie.release_date.is_some() { "clock" } else { "exclamation-circle" })} />
                    </span>

                    <content>
                        if let Some(ts) = movie.release_date {
                            <span>{if self.time.now() < ts { "Releases" } else { "Released" }}</span>
                            {ts.human_date_time(self.time.clone()).lower().view()}
                        } else {
                            <span class="text-muted">{"No release date"}</span>
                        }
                    </content>
                </indicator>
            </div>
        }
    }

    fn view_body(&self, ctx: &Context<Self>, movie: &api::Movie) -> Html {
        let link = ctx.link();

        // Earliest release, used to pre-fill the "Released" quick option.
        let release_at = movie.releases.iter().map(|r| r.timestamp).min();

        // Mark-watched only offers the release date once the movie is actually out.
        let watched_preset = release_at
            .filter(|&r| r <= self.time.now())
            .map(|ts| TimePreset::at("clock", "Released", ts));

        let release_preset = release_at.map(|ts| TimePreset::at("clock", "When released", ts));

        let on_remove_next = link.callback(move |_| Msg::OnRemoveNext);

        html! {
            <>
            if let Some(ref overview) = movie.overview {
                <p class="overview">{overview}</p>
            }

            <div class="detail-layout">
                <div class="hide-desktop">
                    if let Some(ref banner) = movie.banner {
                        <Image class="banner" src={banner.clone()} />
                    } else if let Some(ref backdrop) = movie.backdrop {
                        <Image class="backdrop" src={backdrop.clone()} />
                    }
                </div>

                <div class="detail-sidebar">
                    <Image class="poster hide-mobile" src={movie.poster.clone()} />
                </div>

                <div class="detail-content">
                    <div class="row-split">
                        <div class="column fill">
                            <div class="toolbar">
                                <indicator title="Release date">
                                    if movie.pending.is_some() {
                                        <span class="item-inline" title="Next movie">
                                            <span class="icon primary exclamation-circle" />
                                        </span>
                                    } else if !self.watched.is_empty() {
                                        <span class="item-inline" title="Watched">
                                            <span class="icon primary check-circle" />
                                        </span>
                                    } else {
                                        <span class="item-inline" title="Never watched">
                                            <span class="icon secondary x-circle" />
                                        </span>
                                    }

                                    <content>
                                        if let Some(ts) = movie.pending {
                                            <span>{"Movie scheduled for"}</span>
                                            {ts.human_date_time(self.time.clone()).lower().view()}
                                        } else {
                                            {match &self.watched[..] {
                                                [] => html!(<span>{"Never watched"}</span>),
                                                [w] => html! {
                                                    <>
                                                        <span>{"Watched once"}</span>
                                                        {w.watched.timestamp.human_date_time(self.time.clone()).lower().view()}
                                                    </>
                                                },
                                                [w, ..] => html! {
                                                    <>
                                                        {format!("Watched {} times, first", self.watched.len())}
                                                        {w.watched.timestamp.human_date_time(self.time.clone()).view()}
                                                    </>
                                                }
                                            }}
                                        }
                                    </content>
                                </indicator>

                                <div class="toolbar-toggle">
                                    <button class="btn" onclick={link.callback(move |_| Msg::ToggleDetailedActionsExpanded)}>
                                        <span class="item-inline"><span class={classes!("icon", if self.detailed_expand { "ellipsis-horizontal" } else { "bars-3" })} /></span>
                                    </button>
                                </div>

                                <div class={classes!("toolbar-dropdown", "desktop-input-group", (!self.detailed_expand).then_some("hide-mobile"))}>
                                    <MarkTimeMenu
                                        onerror={ctx.props().onerror.clone()}
                                        trigger_class="btn-success"
                                        icon="check"
                                        title="Mark watched"
                                        prompt="When did you watch the movie?"
                                        preset={watched_preset.clone()}
                                        on_confirm={link.callback(Msg::MarkWatched)}>
                                        <span class="icon check" />
                                        <span class="hide-desktop">{"Mark watched"}</span>
                                    </MarkTimeMenu>

                                    if movie.pending.is_some() {
                                        <button class="btn-primary" onclick={on_remove_next} title="Next movie">
                                            <span class="icon bookmark" />
                                            <span class="hide-desktop">{"Next movie"}</span>
                                        </button>
                                    } else {
                                        <MarkTimeMenu
                                            onerror={ctx.props().onerror.clone()}
                                            trigger_class="btn"
                                            title="Not next movie"
                                            prompt="When do you want to watch the movie?"
                                            preset={release_preset.clone()}
                                            on_confirm={link.callback(Msg::OnWatchNext)}>
                                            <span class="icon bookmark-slash" />
                                            <span class="hide-desktop">{"Not next movie"}</span>
                                        </MarkTimeMenu>
                                    }

                                    <button class="btn" onclick={link.callback(|_| Msg::ToggleOpenWatched)} title="Watch history">
                                        <span class="icon clock" />
                                        <span class="hide-desktop">{"Watch history"}</span>
                                    </button>

                                    <button class="btn" onclick={link.callback(|_| Msg::ToggleOpenReleases)} title="Releases">
                                        <span class="icon calendar" />
                                        <span class="hide-desktop">{"Releases"}</span>
                                    </button>
                                </div>
                            </div>
                        </div>
                    </div>

                    if self.open_watched {
                        <Modal icon="clock" title="Watch history" on_close={link.callback(|_| Msg::ToggleOpenWatched)}>
                            {self.view_watched(ctx, movie.id)}
                        </Modal>
                    }

                    if self.open_releases {
                        <Modal icon="calendar" title="Releases" on_close={link.callback(|_| Msg::ToggleOpenReleases)}>
                            {self.view_releases(ctx)}
                        </Modal>
                    }
                </div>
            </div>

            if self.image_modal {
                { self.view_image_modal(ctx) }
            }

            if self.translations_modal {
                <TranslationsModal
                    target={api::TranslationTarget::Movie(movie.id)}
                    onerror={ctx.props().onerror.clone()}
                    on_close={link.callback(|_| Msg::CloseTranslations)}
                />
            }

            if self.settings_modal {
                <MediaSettingsModal
                    title="Settings"
                    language={movie.language}
                    has_images={!movie.images.is_empty()}
                    has_remotes={!movie.remotes.is_empty()}
                    last_synced={movie.last_synced_at.map(|ts| AttrValue::from(ts.human_date_time(self.time.clone())))}
                    syncing={self.syncing}
                    on_sync={link.callback(|_| Msg::SyncMovie)}
                    auto_sync={movie.auto_sync}
                    on_auto_sync_change={link.callback(Msg::SetAutoSync)}
                    on_language_change={link.callback(Msg::SetLanguage)}
                    release_filters={movie.release_filters.clone()}
                    default_release_filters={self.default_release_filters.clone()}
                    on_release_filters_change={link.callback(Msg::SetReleaseFilters)}
                    on_edit_graphics={link.callback(|_| Msg::OpenImageModal)}
                    on_edit_remotes={link.callback(|_| Msg::OpenRemoteEditor)}
                    on_close={link.callback(|_| Msg::CloseSettingsModal)}
                />
            }

            if self.remote_editor {
                <RemoteEditor
                    title={movie.title.as_deref().unwrap_or("Untitled Movie").to_owned()}
                    kind={RemoteSourceKind::Movie}
                    remotes={movie.remotes.clone()}
                    on_add={link.callback(|(slug, remote)| Msg::AddRemote(slug, remote))}
                    on_edit={link.callback(|(id, slug, remote)| Msg::EditRemote(id, slug, remote))}
                    on_remove={link.callback(Msg::RemoveRemote)}
                    on_set_enabled={link.callback(|(id, enabled)| Msg::SetRemoteEnabled(id, enabled))}
                    on_reorder={link.callback(Msg::ReorderRemotes)}
                    on_set_sync_kinds={link.callback(|(id, kinds)| Msg::SetRemoteSyncKinds(id, kinds))}
                    global_sync_kinds={self.global_sync_kinds.clone()}
                    on_close={link.callback(|_| Msg::CloseRemoteEditor)}
                    onerror={ctx.props().onerror.clone()}
                />
            }
            </>
        }
    }

    fn view_watched(&self, ctx: &Context<Self>, movie_id: api::MovieId) -> Html {
        let link = ctx.link();

        html! {
            <div class="column">
                { for self.watched.iter().map(|w| {
                    let wid = w.watched.id;
                    let kind = api::WatchedKind::Movie { movie: movie_id };

                    html! {
                        <div class="row-split">
                            <div class="row fill date-time">
                                {w.watched.timestamp.human_date_time(self.time.clone()).view()}
                            </div>

                            <button ref={w.remove_watch_anchor.clone()} class="btn-danger" onclick={link.callback(move |_| Msg::ConfirmRemoveWatch(wid))} title="Remove">
                                <span class="icon trash" />
                                <span class="hide-desktop">{"Remove"}</span>
                            </button>

                            if self.confirm_remove_watch == Some(wid) {
                                <ContextMenu
                                    prompt="Remove watch at"
                                    label={w.watched.timestamp.human_date_time(self.time.clone())}
                                    anchor={w.remove_watch_anchor.clone()}
                                    on_close={link.callback(|_| Msg::CancelRemoveWatch)}
                                    onerror={ctx.props().onerror.clone()}>
                                    <ConfirmDanger
                                        on_confirm={link.callback(move |_| Msg::RemoveWatched(wid, kind))}
                                        on_cancel={link.callback(|_| Msg::CancelRemoveWatch)}
                                    />
                                </ContextMenu>
                            }
                        </div>
                    }
                }) }
            </div>
        }
    }

    fn view_releases(&self, ctx: &Context<Self>) -> Html {
        let link = ctx.link();

        if self
            .movie_releases
            .iter()
            .all(|(_, releases)| releases.is_empty())
        {
            return html! {};
        }

        // The release filters in effect for this movie (per-movie override or global default).
        // A release is "considered" when it matches any of them, i.e. it feeds into the release
        // date the earliest considered release determines.
        let filters: &[api::ReleaseFilter] = match self.movie.as_ref() {
            Some(movie) => movie.effective_release_filters(&self.default_release_filters),
            None => &self.default_release_filters,
        };

        let considered = |r: &api::MovieRelease| filters.iter().any(|f| f.matches(r));

        let indicator = |on: bool| {
            let (icon, title) = if on {
                ("check", "Considered for the release date")
            } else {
                ("minus", "Excluded by the current release date settings")
            };

            html! {
                <span class={classes!("item-inline", (!on).then_some("text-muted"))} title={title}>
                    <span class={classes!("icon", icon)} />
                </span>
            }
        };

        html! {
            <div class="column">
                { for self.movie_releases.iter().map(|(ty, releases)| {
                    let ty = *ty;
                    let earliest = releases.iter().min_by_key(|r| r.timestamp).unwrap();
                    let expanded = self.releases_expanded.contains(&ty);
                    let type_considered = releases.iter().any(&considered);

                    html! {
                        <div class="row clickable align-top" onclick={link.callback(move |_| Msg::ToggleReleaseType(ty))}>
                            <span class="item-inline">
                                <span class={classes!("icon", if expanded { "ellipsis-horizontal" } else { "chevron-right" })} />
                            </span>

                            <div class="column fill">
                                <div class="row-split">
                                    <div class="row">
                                        {indicator(type_considered)}
                                        <span>{ty.as_str()}</span>
                                    </div>

                                    <div class="row">
                                        <span class="text-muted">{earliest.timestamp.human_date_time(self.time.clone())}</span>
                                    </div>
                                </div>

                                if expanded {
                                    <div class="column">
                                        { for releases.iter().map(|r| html! {
                                            <div class="row-split">
                                                <div class="row">
                                                    {indicator(considered(r))}

                                                    if let Some(c) = r.country.to_iso() {
                                                        if c.has_flag {
                                                            <span class="item-inline" title={c.name}>
                                                                <span class={classes!("flag", c.alpha2)}></span>
                                                            </span>
                                                        } else {
                                                            <span class="text-muted">
                                                                <span class={c.name}></span>
                                                            </span>
                                                        }
                                                    } else {
                                                        <span class="text-muted">
                                                            {r.country}
                                                        </span>
                                                    }
                                                </div>

                                                <span class="text-muted">{r.timestamp.human_date_time(self.time.clone())}</span>
                                            </div>
                                        }) }
                                    </div>
                                }
                            </div>
                        </div>
                    }
                }) }
            </div>
        }
    }

    fn view_image_modal(&self, ctx: &Context<Self>) -> Html {
        let link = ctx.link();

        html! {
            <Modal icon="photo" title="Graphics" on_close={link.callback(|_| Msg::CloseImageModal)}>
                {for self.graphics.iter().map(|(&kind, items)| {
                    html! {
                        <ImageGallery
                            items={items.clone()}
                            kind={kind}
                            on_select={link.callback(move |id| Msg::SelectImage(kind, id))}
                            on_clear={link.callback(move |_| Msg::ClearSelectedImage(kind))}
                        />
                    }
                })}
            </Modal>
        }
    }
}
