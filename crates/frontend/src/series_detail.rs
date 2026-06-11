use std::collections::{HashMap, HashSet};

use musli_web::web03::prelude::*;
use yew::prelude::*;

use api::{HasAired, TimeZone};

use crate::error::{CustomContext, Error, Message, RcError};
use crate::router::{PagedQuery, Route, SeriesDetailQuery};
use crate::ui::{
    ConfirmDanger, EpisodePicker, ErrorBox, ImageGallery, ImageItem, LanguagePicker, LoadingPage,
    MarkWatchedPicker, RemoteEditor, RemoteSourceKind, RemoteSourceSelect, Tracked,
};
use crate::{Image, SetupChannel};

pub(super) struct SeriesDetail {
    channel: ws::Channel,
    series: Option<api::Series>,
    seasons: Vec<api::Season>,
    selected: Option<api::SeasonNumber>,
    expanded_seasons: bool,
    episodes: Vec<api::Episode>,
    view_orphaned: bool,
    confirm_remove: bool,
    syncing: bool,
    actions_expanded: bool,
    episode_actions_expanded: HashSet<api::EpisodeId>,
    confirm_remove_watch: Option<api::WatchedId>,
    confirming_mark_watch: Option<api::EpisodeId>,
    select_mark_remaining: bool,
    watched_by_episode: HashMap<api::EpisodeId, Vec<api::WatchedEpisode>>,
    history_expanded: HashSet<api::EpisodeId>,
    orphaned: Vec<api::OrphanedWatched>,
    fixing_watched: Option<api::WatchedId>,
    image_modal: Option<api::ImageKind>,
    remote_editor: bool,
    tz: TimeZone,
    _tz_handle: ContextHandle<TimeZone>,
    _setup: SetupChannel,
    _broadcast: ws::Listener,
    _series_req: ws::Request,
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
    _set_sync_source_req: ws::Request,
    _set_language_req: ws::Request,
    _orphaned_req: ws::Request,
    _move_req: ws::Request,
    _remote_req: ws::Request,
}

pub(super) enum Msg {
    Channel(Result<ws::Channel, ws::Error>),
    AppBroadcast(Result<ws::Packet<api::AppBroadcast>, ws::Error>),
    SeriesLoaded(Result<ws::Packet<api::GetSeries>, ws::Error>),
    SeasonsLoaded(Result<ws::Packet<api::ListSeasons>, ws::Error>),
    SelectSeason(api::SeasonNumber),
    ToggleExpandSeasons,
    EpisodesLoaded(Result<ws::Packet<api::ListEpisodes>, ws::Error>),
    AskMarkWatched(api::EpisodeId),
    MarkWatched(api::SeriesId, api::EpisodeId, api::MarkTime),
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
    SetTrackedDone(bool, Result<ws::Packet<api::UntrackSeries>, ws::Error>),
    ConfirmRemove,
    CancelRemove,
    RemoveSeries,
    RemoveDone(Result<ws::Packet<api::RemoveSeries>, ws::Error>),
    SyncSeries,
    SyncDone(Result<ws::Packet<api::SyncSeries>, ws::Error>),
    ToggleHistory(api::EpisodeId),
    WatchedLoaded(Result<ws::Packet<api::ListEpisodesWatched>, ws::Error>),
    OnWatchNext(api::EpisodeId),
    AddPendingDone(Result<ws::Packet<api::AddPending>, ws::Error>),
    OnRemoveNext(api::EpisodeId),
    RemovePendingDone(Result<ws::Packet<api::RemovePending>, ws::Error>),
    SelectImage(api::ImageId),
    ClearSelectedImage(api::ImageKind),
    SelectImageDone(Result<ws::Packet<api::SelectImage>, ws::Error>),
    ClearSelectedImageDone(Result<ws::Packet<api::ClearSelectedImage>, ws::Error>),
    SetSyncSource(api::SyncSource),
    SetSyncSourceDone(
        api::SyncSource,
        Result<ws::Packet<api::SetSeriesSyncSource>, ws::Error>,
    ),
    SetLanguage(Option<String>),
    SetLanguageDone(
        Option<String>,
        Result<ws::Packet<api::SetSeriesLanguage>, ws::Error>,
    ),
    OpenImageModal(api::ImageKind),
    CloseImageModal,
    OpenRemoteEditor,
    CloseRemoteEditor,
    AddRemote(api::RemoteId),
    EditRemote(api::RemoteId, api::RemoteId),
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
    pub(super) error: Option<RcError>,
    pub(super) onerror: Callback<Option<Error>>,
    pub(super) series_id: api::SeriesId,
    #[prop_or_default]
    pub(super) initial_season: Option<api::SeasonNumber>,
    pub(super) on_navigate: Callback<Route>,
}

impl Component for SeriesDetail {
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
            series: None,
            seasons: Vec::new(),
            selected: None,
            expanded_seasons: false,
            episodes: Vec::new(),
            view_orphaned: false,
            confirm_remove: false,
            syncing: false,
            actions_expanded: false,
            episode_actions_expanded: HashSet::new(),
            confirm_remove_watch: None,
            confirming_mark_watch: None,
            select_mark_remaining: false,
            watched_by_episode: HashMap::new(),
            history_expanded: HashSet::new(),
            orphaned: Vec::new(),
            fixing_watched: None,
            image_modal: None,
            remote_editor: false,
            tz,
            _tz_handle,
            _setup,
            _broadcast,
            _series_req: ws::Request::default(),
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
            _set_sync_source_req: ws::Request::default(),
            _set_language_req: ws::Request::default(),
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

    fn view(&self, ctx: &Context<Self>) -> Html {
        let (Some(series), Some(season)) = (&self.series, self.selected) else {
            return html!(<LoadingPage />);
        };

        let link = ctx.link();

        let url = series
            .backdrop
            .as_ref()
            .or(series.fanart.as_ref())
            .map(|i| i.proxy_url());

        let style = url
            .as_ref()
            .map(|url| format!("--background: url('{}')", url))
            .unwrap_or_default();

        html! {
            <div class="page-container" {style}>
                <div class="page">
                    if let Some(ref error) = ctx.props().error {
                        <ErrorBox error={error.clone()} onclearerror={ctx.props().onerror.reform(|()| None)} />
                    }

                    { self.view_header(ctx, series) }

                    <div class={classes!("desktop-row-fill", "mobile-column", "actions", (!self.actions_expanded).then_some("hide-mobile"))}>
                        <div class="desktop-row mobile-column fill start">
                            <LanguagePicker
                                current={series.language.clone()}
                                placeholder="Default"
                                on_change={link.callback(Msg::SetLanguage)}
                            />

                            if series.images.iter().any(|i| matches!(i.kind, api::ImageKind::Poster)) {
                                <button class="btn" onclick={link.callback(|_| Msg::OpenImageModal(api::ImageKind::Poster))}>
                                    <span class="icon-inline"><span class="icon photo" /></span>
                                    <span>{"Poster"}</span>
                                </button>
                            }

                            if series.images.iter().any(|i| matches!(i.kind, api::ImageKind::Backdrop)) {
                                <button class="btn" onclick={link.callback(|_| Msg::OpenImageModal(api::ImageKind::Backdrop))}>
                                    <span class="icon-inline"><span class="icon photo" /></span>
                                    <span>{"Backdrop"}</span>
                                </button>
                            }

                            <div class="input-group">
                                <div class="input-label">{"Sync"}</div>

                                <RemoteSourceSelect
                                    kind={RemoteSourceKind::Series}
                                    remotes={series.remotes.clone()}
                                    current_source={series.effective_sync_source().map(|r| r.into_remote_source())}
                                    on_change={link.callback(|s: api::RemoteSource| Msg::SetSyncSource(s.into_sync_source()))}
                                />

                                if let Some(ts) = series.last_synced_at {
                                    <div class="input-text fill" title="Last synced at">
                                        <span>{ts.display(self.tz.clone())}</span>
                                    </div>
                                } else {
                                    <div class="input-text fill">{"Never synced"}</div>
                                }

                                if !series.remotes.is_empty() {
                                    <button class="btn" onclick={link.callback(|_| Msg::SyncSeries)} title="Sync now">
                                        <span class="icon-inline"><span class={classes!("icon", "arrow-path", self.syncing.then_some("spin"))} /></span>
                                    </button>
                                }
                            </div>

                            <button class="btn" onclick={link.callback(|_| Msg::OpenRemoteEditor)} title="Repair remote identifiers">
                                <span class="icon-inline"><span class="icon identification" /></span>
                                <span>{"Identifiers"}</span>
                            </button>
                        </div>

                        <div class="desktop-row mobile-column end desktop-input-group">
                            <Tracked tracked={series.tracked} ontoggle={link.callback(Msg::SetTracked)} />

                            if self.confirm_remove {
                                <ConfirmDanger prompt="Remove series" label={series.title.clone()} on_confirm={link.callback(|_| Msg::RemoveSeries)} on_cancel={link.callback(|_| Msg::CancelRemove)} />
                            } else {
                                <button class="btn btn-danger" onclick={link.callback(|_| Msg::ConfirmRemove)} title="Remove series">
                                    <span class="icon-inline"><span class="icon trash" /></span>
                                    <span class="hide-desktop">{"Remove"}</span>
                                </button>
                            }

                            if !series.remotes.is_empty() {
                                <div class="hide-desktop row">
                                    {for series.remotes.iter().filter_map(|r| {
                                        let url = r.series_url()?;
                                        let label = r.source().as_str().to_uppercase();

                                        Some(html! {
                                            <a class="btn" href={url} target="_blank" rel="noopener noreferrer" title={format!("Open on {label}")}>
                                                <span class="icon-inline"><span class="icon arrow-top-right-on-square" /></span>
                                                <span>{label}</span>
                                            </a>
                                        })
                                    })}
                                </div>
                            }
                        </div>
                    </div>

                    <div class="detail-layout">
                        <Image class="banner hide-desktop" src={series.banner.clone()} />

                        { self.view_sidebar(ctx, series) }

                        { self.view_episodes(ctx, season) }
                    </div>

                    if let Some(kind) = self.image_modal {
                        { self.view_image_modal(ctx, kind, series) }
                    }

                    if self.remote_editor {
                        <RemoteEditor
                            title={series.title.as_deref().unwrap_or("Untitled Series").to_owned()}
                            remotes={series.remotes.clone()}
                            on_add={link.callback(Msg::AddRemote)}
                            on_edit={link.callback(|(old, new)| Msg::EditRemote(old, new))}
                            on_remove={link.callback(Msg::RemoveRemote)}
                            on_close={link.callback(|_| Msg::CloseRemoteEditor)}
                        />
                    }
                </div>
            </div>
        }
    }

    fn changed(&mut self, ctx: &Context<Self>, old_props: &Props) -> bool {
        if ctx.props().series_id != old_props.series_id {
            self.series = None;
            self.seasons.clear();
            self.selected = None;
            self.expanded_seasons = false;
            self.episodes.clear();
            self.confirm_remove = false;
            self.syncing = false;
            self.confirm_remove_watch = None;
            self.watched_by_episode.clear();

            if self.channel.id() != ws::ChannelId::NONE {
                self.load_series(ctx);
                self.load_seasons(ctx);
                self.load_history(ctx);
            }
        } else if ctx.props().initial_season != old_props.initial_season {
            if let Some(season) = ctx.props().initial_season {
                if self.selected != Some(season) {
                    self.selected = Some(season);
                    self.episodes.clear();
                    self.confirm_remove_watch = None;
                    self.watched_by_episode.clear();

                    if self.channel.id() != ws::ChannelId::NONE {
                        self.load_episodes(ctx, season);
                        self.load_orphaned(ctx);
                        self.load_history(ctx);
                    }
                }
            }
        }
        true
    }
}

impl SeriesDetail {
    fn try_update(&mut self, ctx: &Context<Self>, msg: Msg) -> Result<bool, Error> {
        match msg {
            Msg::Channel(result) => {
                self.channel = result?;
                if self.channel.id() != ws::ChannelId::NONE {
                    self.load_series(ctx);
                    self.load_seasons(ctx);
                    self.load_orphaned(ctx);
                } else {
                    self.series = None;
                    self.seasons.clear();
                    self.episodes.clear();
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
                    api::AppEventKind::SeriesChanged { series }
                        if series.id == ctx.props().series_id =>
                    {
                        self.series = Some(series.clone());
                        Ok(true)
                    }
                    api::AppEventKind::SeasonsChanged { series_id, .. }
                        if *series_id == ctx.props().series_id =>
                    {
                        if self.channel.id() != ws::ChannelId::NONE {
                            self.load_seasons(ctx);
                        }
                        Ok(false)
                    }
                    api::AppEventKind::EpisodesChanged { series_id, season }
                        if *series_id == ctx.props().series_id =>
                    {
                        if self.selected == Some(*season) {
                            self.load_episodes(ctx, *season);
                        }

                        self.load_orphaned(ctx);
                        Ok(false)
                    }
                    api::AppEventKind::PendingChanged => {
                        if let Some(season) = self.selected {
                            self.load_episodes(ctx, season);
                        }

                        self.load_orphaned(ctx);
                        Ok(false)
                    }
                    api::AppEventKind::TaskAdded { task }
                    | api::AppEventKind::TaskStarted { task } => {
                        if matches!(&task.kind, api::TaskKind::SyncSeries { series_id, .. } if *series_id == ctx.props().series_id)
                        {
                            self.syncing = true;
                            return Ok(true);
                        }
                        Ok(false)
                    }
                    api::AppEventKind::TaskCompleted { task } => {
                        if matches!(&task.kind, api::TaskKind::SyncSeries { series_id, .. } if *series_id == ctx.props().series_id)
                        {
                            self.syncing = false;

                            if let Some(season) = self.selected {
                                self.load_episodes(ctx, season);
                            }

                            self.load_series(ctx);
                            self.load_seasons(ctx);
                            self.load_orphaned(ctx);
                            return Ok(true);
                        }
                        Ok(false)
                    }
                    api::AppEventKind::WatchedChanged { event: kind } => {
                        let relevant = match kind {
                            api::WatchedEvent::Episode { series, .. } => {
                                *series == ctx.props().series_id
                            }
                            api::WatchedEvent::RemainingSeason { series, .. } => {
                                *series == ctx.props().series_id
                            }
                            api::WatchedEvent::Movie { .. } => false,
                        };

                        if relevant {
                            if let Some(season) = self.selected {
                                self.load_episodes(ctx, season);
                            }

                            self.load_orphaned(ctx);
                            self.load_history(ctx);
                        }

                        Ok(false)
                    }
                    _ => Ok(false),
                }
            }
            Msg::SeriesLoaded(result) => {
                self.series = Some(
                    result
                        .context(Message::LoadingSeries)?
                        .decode()
                        .context(Message::LoadingSeries)?,
                );

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
                        .and_then(|n| self.seasons.iter().find(|s| s.number == n));

                    self.selected = initial
                        .or_else(|| self.seasons.iter().find(|s| !s.number.is_special()))
                        .or_else(|| self.seasons.first())
                        .map(|s| s.number);

                    if let Some(season) = self.selected {
                        self.load_episodes(ctx, season);
                    }

                    self.load_history(ctx);
                    self.load_orphaned(ctx);
                }

                Ok(true)
            }
            Msg::SelectSeason(season) => {
                if self.selected != Some(season) {
                    let id = ctx.props().series_id;

                    ctx.props().on_navigate.emit(Route::SeriesDetail(
                        id,
                        SeriesDetailQuery {
                            season: Some(season),
                        },
                    ));
                }

                self.expanded_seasons = false;
                Ok(false)
            }
            Msg::ToggleExpandSeasons => {
                self.expanded_seasons = !self.expanded_seasons;
                Ok(true)
            }
            Msg::EpisodesLoaded(result) => {
                let result = result
                    .context(Message::LoadingEpisodes)?
                    .decode()
                    .context(Message::LoadingEpisodes)?;

                self.watched_by_episode.clear();

                self.episodes = result.episodes;

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
            Msg::MarkWatched(series, episode, mark_time) => {
                self.episode_actions_expanded.remove(&episode);
                self.confirming_mark_watch = None;
                self._mark_req = self
                    .channel
                    .request()
                    .body(api::MarkWatchedRequest {
                        kind: api::WatchedKind::Episode { series, episode },
                        mark_time,
                    })
                    .on_packet(ctx.link().callback(Msg::MarkWatchedDone))
                    .send();
                Ok(true)
            }
            Msg::MarkWatchedDone(result) => {
                result.context(Message::MarkingWatched)?;

                if let Some(season) = self.selected {
                    self.load_episodes(ctx, season);
                }

                self.load_history(ctx);
                self.load_orphaned(ctx);
                Ok(false)
            }
            Msg::RemoveWatched(id, kind) => {
                self.orphaned.retain(|w| w.id != id);

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

                if let Some(season) = self.selected {
                    self.load_episodes(ctx, season);
                }

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

                let series_id = ctx.props().series_id;

                self._watch_remaining_reqs = self
                    .channel
                    .request()
                    .body(api::MarkWatchedRemainingRequest {
                        series_id: series_id,
                        season,
                        mark_time,
                    })
                    .on_packet(ctx.link().callback(Msg::WatchRemainingDone))
                    .send();
                Ok(false)
            }
            Msg::WatchRemainingDone(result) => {
                result.context(Message::MarkingWatched)?;

                if let Some(season) = self.selected {
                    self.load_episodes(ctx, season);
                }

                self.load_history(ctx);
                self.load_orphaned(ctx);
                Ok(false)
            }
            Msg::SetTracked(tracked) => {
                self.actions_expanded = false;

                let id = ctx.props().series_id;

                self._untrack_req = self
                    .channel
                    .request()
                    .body(api::UntrackSeriesRequest { id, tracked })
                    .on_packet(
                        ctx.link()
                            .callback(move |r| Msg::SetTrackedDone(tracked, r)),
                    )
                    .send();

                Ok(false)
            }
            Msg::SetTrackedDone(tracked, result) => {
                result.context(Message::UntrackingSeries)?;
                if let Some(ref mut series) = self.series {
                    series.tracked = tracked;
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
            Msg::RemoveSeries => {
                let id = ctx.props().series_id;
                self._remove_req = self
                    .channel
                    .request()
                    .body(api::RemoveSeriesRequest { id })
                    .on_packet(ctx.link().callback(Msg::RemoveDone))
                    .send();
                Ok(false)
            }
            Msg::RemoveDone(result) => {
                result.context(Message::RemovingSeries)?;
                ctx.props()
                    .on_navigate
                    .emit(Route::Series(PagedQuery::default()));
                Ok(false)
            }
            Msg::SyncSeries => {
                let id = ctx.props().series_id;

                self._sync_req = self
                    .channel
                    .request()
                    .body(api::SyncSeriesRequest { id })
                    .on_packet(ctx.link().callback(Msg::SyncDone))
                    .send();

                Ok(true)
            }
            Msg::SyncDone(result) => {
                result.context(Message::SyncingSeries)?;
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
            Msg::OnWatchNext(episode_id) => {
                self.episode_actions_expanded.remove(&episode_id);

                let series_id = ctx.props().series_id;

                self._set_next_req = self
                    .channel
                    .request()
                    .body(api::AddPendingRequest {
                        kind: api::PendingKind::Episode {
                            series: series_id,
                            episode: episode_id,
                        },
                    })
                    .on_packet(ctx.link().callback(Msg::AddPendingDone))
                    .send();

                Ok(false)
            }
            Msg::AddPendingDone(result) => {
                result.context(Message::SyncingSeries)?;

                if let Some(season) = self.selected {
                    self.load_episodes(ctx, season);
                }

                self.load_orphaned(ctx);
                Ok(false)
            }
            Msg::OnRemoveNext(episode_id) => {
                self.episode_actions_expanded.remove(&episode_id);
                let series_id = ctx.props().series_id;

                self._set_next_req = self
                    .channel
                    .request()
                    .body(api::RemovePendingRequest {
                        kind: api::PendingKind::Episode {
                            series: series_id,
                            episode: episode_id,
                        },
                    })
                    .on_packet(ctx.link().callback(Msg::RemovePendingDone))
                    .send();

                Ok(false)
            }
            Msg::RemovePendingDone(result) => {
                result.context(Message::SyncingSeries)?;

                if let Some(season) = self.selected {
                    self.load_episodes(ctx, season);
                }

                Ok(false)
            }
            Msg::SelectImage(id) => {
                self._select_image_req = self
                    .channel
                    .request()
                    .body(api::SelectImageRequest { id })
                    .on_packet(ctx.link().callback(Msg::SelectImageDone))
                    .send();

                Ok(false)
            }
            Msg::ClearSelectedImage(kind) => {
                self._clear_image_req = self
                    .channel
                    .request()
                    .body(api::ClearSelectedImageRequest {
                        owner: api::ImageOwner::Series(ctx.props().series_id),
                        kind,
                    })
                    .on_packet(ctx.link().callback(Msg::ClearSelectedImageDone))
                    .send();

                Ok(false)
            }
            Msg::SelectImageDone(result) => {
                result.context(Message::SyncingSeries)?;
                self.image_modal = None;
                self.load_series(ctx);
                Ok(true)
            }
            Msg::ClearSelectedImageDone(result) => {
                result.context(Message::SyncingSeries)?;
                self.image_modal = None;
                self.load_series(ctx);
                Ok(true)
            }
            Msg::SetSyncSource(source) => {
                let id = ctx.props().series_id;

                self._set_sync_source_req = self
                    .channel
                    .request()
                    .body(api::SetSeriesSyncSourceRequest {
                        id,
                        source: source.clone(),
                    })
                    .on_packet(
                        ctx.link()
                            .callback(move |r| Msg::SetSyncSourceDone(source.clone(), r)),
                    )
                    .send();

                Ok(false)
            }
            Msg::SetSyncSourceDone(source, result) => {
                result.context(Message::SettingSyncSource)?;
                if let Some(ref mut series) = self.series {
                    series.sync_source = Some(source);
                }
                Ok(true)
            }
            Msg::SetLanguage(language) => {
                let id = ctx.props().series_id;

                self._set_language_req = self
                    .channel
                    .request()
                    .body(api::SetSeriesLanguageRequest {
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
                if let Some(ref mut series) = self.series {
                    series.language = language;
                }
                Ok(true)
            }
            Msg::OpenImageModal(kind) => {
                self.image_modal = Some(kind);
                Ok(true)
            }
            Msg::CloseImageModal => {
                self.image_modal = None;
                Ok(true)
            }
            Msg::OpenRemoteEditor => {
                self.remote_editor = true;
                Ok(true)
            }
            Msg::CloseRemoteEditor => {
                self.remote_editor = false;
                Ok(true)
            }
            Msg::AddRemote(remote_id) => {
                let id = ctx.props().series_id;

                self._remote_req = self
                    .channel
                    .request()
                    .body(api::AddSeriesRemoteRequest { id, remote_id })
                    .on_packet(ctx.link().callback(
                        |r: Result<ws::Packet<api::AddSeriesRemote>, ws::Error>| {
                            Msg::RemoteDone(r.map(|_| ()))
                        },
                    ))
                    .send();

                Ok(false)
            }
            Msg::EditRemote(old, new) => {
                let id = ctx.props().series_id;

                self._remote_req = self
                    .channel
                    .request()
                    .body(api::UpdateSeriesRemoteRequest { id, old, new })
                    .on_packet(ctx.link().callback(
                        |r: Result<ws::Packet<api::UpdateSeriesRemote>, ws::Error>| {
                            Msg::RemoteDone(r.map(|_| ()))
                        },
                    ))
                    .send();

                Ok(false)
            }
            Msg::RemoveRemote(remote_id) => {
                let id = ctx.props().series_id;

                self._remote_req = self
                    .channel
                    .request()
                    .body(api::RemoveSeriesRemoteRequest { id, remote_id })
                    .on_packet(ctx.link().callback(
                        |r: Result<ws::Packet<api::RemoveSeriesRemote>, ws::Error>| {
                            Msg::RemoteDone(r.map(|_| ()))
                        },
                    ))
                    .send();

                Ok(false)
            }
            Msg::RemoteDone(result) => {
                result.context(Message::EditingRemotes)?;
                self.load_series(ctx);
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
                let series_id = ctx.props().series_id;
                self._move_req = self
                    .channel
                    .request()
                    .body(api::MoveWatchedEpisodeRequest {
                        id,
                        series_id,
                        season,
                        episode,
                    })
                    .on_packet(ctx.link().callback(Msg::MoveWatchedDone))
                    .send();
                Ok(false)
            }
            Msg::MoveWatchedDone(result) => {
                result.context(Message::MarkingWatched)?;

                if let Some(season) = self.selected {
                    self.load_episodes(ctx, season);
                    self.load_history(ctx);
                }

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

    fn load_series(&mut self, ctx: &Context<Self>) {
        self._series_req = self
            .channel
            .request()
            .body(api::GetSeriesRequest {
                id: ctx.props().series_id,
            })
            .on_packet(ctx.link().callback(Msg::SeriesLoaded))
            .send();
    }

    fn load_seasons(&mut self, ctx: &Context<Self>) {
        self._seasons_req = self
            .channel
            .request()
            .body(api::ListSeasonsRequest {
                series_id: ctx.props().series_id,
            })
            .on_packet(ctx.link().callback(Msg::SeasonsLoaded))
            .send();
    }

    fn load_episodes(&mut self, ctx: &Context<Self>, season: api::SeasonNumber) {
        self._episodes_req = self
            .channel
            .request()
            .body(api::ListEpisodesRequest {
                series_id: ctx.props().series_id,
                season,
            })
            .on_packet(ctx.link().callback(Msg::EpisodesLoaded))
            .send();
    }

    fn load_history(&mut self, ctx: &Context<Self>) {
        let series_id = ctx.props().series_id;

        self._watched_req = self
            .channel
            .request()
            .body(api::ListEpisodesWatchedRequest { series_id })
            .on_packet(ctx.link().callback(Msg::WatchedLoaded))
            .send();
    }

    fn load_orphaned(&mut self, ctx: &Context<Self>) {
        let series_id = ctx.props().series_id;

        self._orphaned_req = self
            .channel
            .request()
            .body(api::ListOrphanedWatchedRequest { series_id })
            .on_packet(ctx.link().callback(Msg::OrphanedLoaded))
            .send();
    }

    fn view_header(&self, ctx: &Context<Self>, series: &api::Series) -> Html {
        let link = ctx.link();

        html! {
            <div class="row-fill page-title">
                if let Some(ref title) = series.title {
                    <span class="fill">{title}</span>
                } else {
                    <span class="fill text-muted">{"Untitled Series"}</span>
                }

                <div class="row end">
                    if !series.remotes.is_empty() {
                        <div class="row hide-mobile">
                            <div class="input-group">
                                {for series.remotes.iter().filter_map(|r| {
                                    let url = r.series_url()?;
                                    let label = r.source().as_str().to_uppercase();

                                    Some(html! {
                                        <a class="btn" href={url} target="_blank" rel="noopener noreferrer" title={format!("Open on {label}")}>
                                            <span class="icon-inline"><span class="icon arrow-top-right-on-square" /></span>
                                            <span>{label}</span>
                                        </a>
                                    })
                                })}
                            </div>
                        </div>
                    }

                    <button class="hide-desktop btn" onclick={link.callback(|_| Msg::ToggleActionsExpanded)}>
                        <span class="icon-inline"><span class={classes!("icon", if self.actions_expanded { "ellipsis-horizontal" } else { "bars-2" })} /></span>
                    </button>
                </div>
            </div>
        }
    }

    fn view_sidebar(&self, ctx: &Context<Self>, series: &api::Series) -> Html {
        html! {
            <div class="detail-sidebar">
                <Image class="poster hide-mobile" src={series.poster.clone()} />

                <div class="table table-striped">
                    { for self.seasons.iter().map(|s| self.view_season(ctx, s, self.seasons.len())) }
                </div>
            </div>
        }
    }

    fn view_image_modal(
        &self,
        ctx: &Context<Self>,
        kind: api::ImageKind,
        series: &api::Series,
    ) -> Html {
        let selected_for_kind = match kind {
            api::ImageKind::Poster => series.poster.as_ref(),
            api::ImageKind::Banner => series.banner.as_ref(),
            api::ImageKind::Fanart => series.fanart.as_ref(),
            api::ImageKind::Backdrop => series.backdrop.as_ref(),
            api::ImageKind::Screenshot => None,
        };

        let items: Vec<ImageItem> = series
            .images
            .iter()
            .map(|img| ImageItem {
                id: img.id,
                kind: img.kind,
                source: img.source,
                image: img.image.clone(),
                selected: Some(&img.image) == selected_for_kind,
            })
            .collect();

        let link = ctx.link();

        html! {
            <ImageGallery
                {items}
                {kind}
                on_select={link.callback(Msg::SelectImage)}
                on_clear={if kind == api::ImageKind::Backdrop {
                    Some(link.callback(move |_| Msg::ClearSelectedImage(kind)))
                } else {
                    None
                }}
                on_close={link.callback(|_| Msg::CloseImageModal)}
            />
        }
    }

    fn view_season(&self, ctx: &Context<Self>, season: &api::Season, total: usize) -> Html {
        let number = season.number;
        let active = self.selected == Some(number);
        let clickable = total > 1;

        let onclick = if !clickable {
            Callback::noop()
        } else if active {
            ctx.link().callback(move |_| Msg::ToggleExpandSeasons)
        } else {
            ctx.link().callback(move |_| Msg::SelectSeason(number))
        };

        let style = if season.total_count > 0 {
            let frac = (season.watched_count.min(season.total_count) as f64 * 100.0)
                / season.total_count as f64;

            Some(format!("width: {frac:.0}%"))
        } else {
            None
        };

        html! {
            <div class={classes!("table-entry", "column", (!active && clickable).then_some("clickable"), active.then_some("active"), (!active && !self.expanded_seasons).then_some("hide-mobile"))} {onclick}>
                <div class="row-fill fill">
                    <span>{season.number.long().to_string()}</span>

                    <div class="row">
                        if let Some(ts) = season.air_date {
                            <span class="text-muted">{ts.date(self.tz.clone()).year().to_string()}</span>
                        }

                        if clickable {
                            <span class="icon-inline">
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

    fn view_episodes(&self, ctx: &Context<Self>, season: api::SeasonNumber) -> Html {
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

        html! {
            <div class="detail-content">
                <div class="row-fill actions">
                    if !self.orphaned.is_empty() {
                        <button class="btn-danger" onclick={link.callback(|_| Msg::ToggleOrphaned)} title="View orphaned watched episodes">
                            <span class="icon-inline"><span class="icon exclamation-triangle" /></span>
                            <span class="hide-mobile">{if self.view_orphaned { "Hide orphaned watches" } else { "Show orphaned watches" }}</span>
                        </button>
                    }

                    if self.select_mark_remaining {
                        <MarkWatchedPicker
                            on_confirm={link.callback(move |mark_time| Msg::WatchRemaining(season, mark_time))}
                            on_cancel={link.callback(|_| Msg::CancelMarkRemainingWatch)}
                        />
                    } else {
                        <div class="page-sub-title">{season.long().to_string()}</div>

                        if total > 0 {
                            <span>
                                {format!("{watched_count} / {total} watched")}
                            </span>
                        }

                        if watched_count < total {
                            <button class="btn-success end" onclick={link.callback(move |_| Msg::MarkRemainingWatch)} title="Mark remaining episodes as watched">
                                <span class="icon-inline"><span class="icon check" /></span>
                                <span class="hide-mobile">{"Remaining"}</span>
                            </button>
                        }
                    }
                </div>

                if self.episodes.is_empty() && self.selected.is_some() {
                    <div class="empty text-muted">{"No episodes."}</div>
                }

                if self.view_orphaned {
                    { self.view_orphaned(ctx) }
                } else {
                    { for self.episodes.iter().map(|ep| self.view_episode(ctx, ep)) }
                }
            </div>
        }
    }

    fn view_episode(&self, ctx: &Context<Self>, episode: &api::Episode) -> Html {
        let link = ctx.link();

        let series_id = ctx.props().series_id;
        let episode_id = episode.id;

        let watched = self
            .watched_by_episode
            .get(&episode_id)
            .map(Vec::as_slice)
            .unwrap_or_default();

        let history_expanded = self.history_expanded.contains(&episode_id);
        let confirming_mark = self.confirming_mark_watch == Some(episode_id);
        let on_ask_mark = link.callback(move |_| Msg::AskMarkWatched(episode_id));
        let on_toggle_history =
            (!watched.is_empty()).then(|| link.callback(move |_| Msg::ToggleHistory(episode_id)));

        let actions_expanded = self.episode_actions_expanded.contains(&episode_id);

        let toggle_pending = move |mobile: bool| {
            let on_remove_next = link.callback(move |_| Msg::OnRemoveNext(episode_id));
            let on_watch_next = link.callback(move |_| Msg::OnWatchNext(episode_id));

            html! {
                if episode.pending {
                    <button class="btn" onclick={on_remove_next} title="Remove from watch next">
                        <span class="icon bookmark-slash" />
                        <span class={classes!(mobile.then_some("hide-mobile"), "hide-desktop")}>{"Remove watch next"}</span>
                    </button>
                } else {
                    <button class="btn" onclick={on_watch_next} title="Watch next">
                        <span class="icon bookmark" />
                        <span class={classes!(mobile.then_some("hide-mobile"), "hide-desktop")}>{"Watch next"}</span>
                    </button>
                }
            }
        };

        let main_actions = html! {
            <>
                if let Some(on_toggle) = on_toggle_history {
                    <button class="btn" onclick={on_toggle} title={if history_expanded { "Hide watch history" } else { "Show watch history" }}>
                        <span class={if history_expanded { "icon chevron-up" } else { "icon clock" }} />
                        <span class="hide-desktop">{if history_expanded { "History" } else { "Show history" }}</span>
                    </button>
                }

                <button class="btn-success" onclick={on_ask_mark.clone()} title="Mark watched">
                    <span class="icon check" />
                    <span class="hide-desktop">{"Mark watched"}</span>
                </button>

                {toggle_pending(false)}
            </>
        };

        let actions = 'actions: {
            if confirming_mark {
                break 'actions html! {
                    <MarkWatchedPicker
                        on_confirm={link.callback(move |mark_time| Msg::MarkWatched(series_id, episode_id, mark_time))}
                        on_cancel={link.callback(move |_| Msg::CancelMarkWatch(episode_id))}
                    />
                };
            }

            html! {
                <div class="actions row-fill">
                    <div class="column fill">
                        <div class="row-fill">
                            <div class="row">
                                if watched.len() > 0 {
                                    <span class="icon-inline" title="Watched"><span class="icon check-circle" /></span>
                                } else {
                                    <span class="icon-inline" title="Not watched"><span class="icon x-circle" /></span>
                                }

                                <span class="text-muted fill">
                                    {match &watched[..] {
                                        [] => "Not watched".to_string(),
                                        [w] => format!("Watched at {}", w.timestamp.display(self.tz.clone())),
                                        [first, ..] => format!("Watched {} times, first at {}", watched.len(), first.timestamp.display(self.tz.clone())),
                                    }}
                                </span>
                            </div>

                            <div class="row end">
                                <div class="hide-desktop">
                                    <div class="input-group">
                                        {toggle_pending(true)}

                                        <button class="btn" onclick={link.callback(move |_| Msg::ToggleEpisodeActionsExpanded(episode_id))}>
                                            <span class="icon-inline"><span class={classes!("icon", if actions_expanded { "ellipsis-horizontal" } else { "bars-2" })} /></span>
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

                        <div class={classes!("column", "hide-desktop", (!actions_expanded).then_some("hide-mobile"))}>
                            {main_actions.clone()}
                        </div>

                        <div class="column fill">
                            <span class={classes!("row", actions_expanded.then_some("hide-mobile"))}>
                                { episode.name.as_deref().unwrap_or("—") }
                            </span>

                            if let Some(s) = episode.display_at(self.tz.clone()) {
                                <span class={classes!("text-muted", actions_expanded.then_some("hide-mobile"))}>{s}</span>
                            }
                        </div>
                    </div>
                </div>
            }
        };

        html! {
            <div class={classes!("column", (!watched.is_empty()).then_some("watched"))}>
                <a class="episode-code">
                    { format!("{}E{:02}", episode.season.short(), episode.number) }
                </a>

                <Image class="screenshot" src={episode.screenshot.clone()} />

                {actions}

                if let Some(ref overview) = episode.overview {
                    <p class={classes!("overview", actions_expanded.then_some("hide-mobile"))}>{overview}</p>
                }

                if history_expanded {
                    <div class="column">
                        <h3>{"Watch history"}</h3>

                        <div class="column">
                            { for watched.iter().map(|w| {
                                let wid = w.id;
                                let kind = api::WatchedKind::Episode { series: series_id, episode: episode_id };

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
                                            series_id={series_id}
                                            seasons={self.seasons.clone()}
                                            selected_season={episode.season}
                                            selected_episode={episode.number}
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
        }
    }

    fn view_orphaned(&self, ctx: &Context<Self>) -> Html {
        if self.orphaned.is_empty() {
            return html! {};
        }

        let link = ctx.link();
        let series_id = ctx.props().series_id;

        html! {
            <div class="column">
                <div class="table">
                    { for self.orphaned.iter().map(|w| {
                        let wid = w.id;

                        let kind = api::WatchedKind::Episode { series: series_id, episode: api::EpisodeId::new(0) };

                        if self.fixing_watched == Some(wid) {
                            html! {
                                <div class="table-entry">
                                    <EpisodePicker
                                        prompt="Move watch at"
                                        label={w.timestamp.display(self.tz.clone())}
                                        {series_id}
                                        seasons={self.seasons.clone()}
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
                                        <span class="text-muted">{format!("{}E{:02}", w.season.short(), w.episode)}</span>
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
