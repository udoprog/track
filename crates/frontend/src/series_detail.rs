use std::collections::{HashMap, HashSet};

use musli_web::web03::prelude::*;
use yew::prelude::*;

use api::{HasAired, TimeZone};

use crate::SetupChannel;
use crate::error::{CustomContext, Error, Message};
use crate::router::{PagedQuery, Route, SeriesDetailQuery};
use crate::ui::{
    ConfirmDanger, EpisodePicker, ImageGallery, ImageItem, LanguagePicker, MarkWatchedPicker,
    RemoteSourceKind, RemoteSourceSelect,
};

pub(super) struct SeriesDetail {
    channel: ws::Channel,
    series: Option<api::Series>,
    seasons: Vec<api::Season>,
    selected: Option<api::SeasonNumber>,
    episodes: Vec<api::Episode>,
    confirm_remove: bool,
    syncing: bool,
    confirm_remove_watch: Option<api::WatchedId>,
    confirming_mark_watch: Option<api::EpisodeId>,
    select_mark_remaining: bool,
    watched: HashMap<api::EpisodeId, Vec<api::WatchedEpisode>>,
    expanded: HashSet<api::EpisodeId>,
    orphaned: Vec<api::OrphanedWatched>,
    fixing_watched: Option<api::WatchedId>,
    image_modal: Option<api::ImageKind>,
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
}

pub(super) enum Msg {
    Channel(Result<ws::Channel, ws::Error>),
    AppBroadcast(Result<ws::Packet<api::AppBroadcast>, ws::Error>),
    SeriesLoaded(Result<ws::Packet<api::GetSeries>, ws::Error>),
    SeasonsLoaded(Result<ws::Packet<api::ListSeasons>, ws::Error>),
    SelectSeason(api::SeasonNumber),
    EpisodesLoaded(Result<ws::Packet<api::ListEpisodes>, ws::Error>),
    AskMarkWatched(api::EpisodeId),
    MarkWatched(api::SeriesId, api::EpisodeId, api::MarkTime),
    MarkWatchedDone(Result<ws::Packet<api::MarkWatched>, ws::Error>),
    CancelMarkWatch,
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
    AddPending(api::EpisodeId),
    AddPendingDone(Result<ws::Packet<api::AddPending>, ws::Error>),
    RemovePending(api::EpisodeId),
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
    Back,
    SetTz(TimeZone),
    FixWatched(api::WatchedId),
    CancelFixWatched,
    MoveWatched(api::WatchedId, api::SeasonNumber, u32),
    MoveWatchedDone(Result<ws::Packet<api::MoveWatchedEpisode>, ws::Error>),
    OrphanedLoaded(Result<ws::Packet<api::ListOrphanedWatched>, ws::Error>),
}

#[derive(Properties, PartialEq)]
pub(super) struct Props {
    pub(super) series_id: api::SeriesId,
    #[prop_or_default]
    pub(super) initial_season: Option<api::SeasonNumber>,
    pub(super) onerror: Callback<Error>,
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
            episodes: Vec::new(),
            confirm_remove: false,
            syncing: false,
            confirm_remove_watch: None,
            confirming_mark_watch: None,
            select_mark_remaining: false,
            watched: HashMap::new(),
            expanded: HashSet::new(),
            orphaned: Vec::new(),
            fixing_watched: None,
            image_modal: None,
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
        let Some(ref series) = self.series else {
            return html! {
                <div class="page">
                    <div class="empty text-muted">{"Loading…"}</div>
                </div>
            };
        };

        let link = ctx.link();

        let url = series
            .images
            .iter()
            .find(|i| matches!(i.kind, api::ImageKind::Backdrop))
            .map(|image| image.image.proxy_url());

        let style = url
            .as_ref()
            .map(|url| format!("--background: url('{}')", url))
            .unwrap_or_default();

        html! {
            <div class="page-container" {style}>
                <div class="page">
                    { self.view_header(ctx) }

                    <div class="row-fill actions">
                        <div class="row fill start">
                            <RemoteSourceSelect
                                kind={RemoteSourceKind::Series}
                                remotes={series.remotes.clone()}
                                current_source={series.effective_sync_source()}
                                on_change={link.callback(Msg::SetSyncSource)}
                            />

                            <LanguagePicker
                                current={series.language.clone()}
                                placeholder="Default"
                                on_change={link.callback(Msg::SetLanguage)}
                            />

                            if series.images.iter().any(|i| matches!(i.kind, api::ImageKind::Poster)) {
                                <button class="btn" onclick={link.callback(|_| Msg::OpenImageModal(api::ImageKind::Poster))}>
                                    <span class="icon-inline"><span class="icon photo" /></span>
                                    <span class="hide-mobile">{"Poster"}</span>
                                </button>
                            }

                            if series.images.iter().any(|i| matches!(i.kind, api::ImageKind::Backdrop)) {
                                <button class="btn" onclick={link.callback(|_| Msg::OpenImageModal(api::ImageKind::Backdrop))}>
                                    <span class="icon-inline"><span class="icon photo" /></span>
                                    <span class="hide-mobile">{"Backdrop"}</span>
                                </button>
                            }

                            if let Some(ts) = series.last_synced_at {
                                <span class="text-muted hide-mobile" title="Last synced at">
                                    {ts.display(self.tz.clone())}
                                </span>
                            }
                        </div>

                        <div class="row end">
                            if series.tracked {
                                <button class="btn" onclick={link.callback(|_| Msg::SetTracked(false))} title="Untrack series">
                                    <span class="icon-inline"><span class="icon eye-slash" /></span>
                                    <span class="hide-mobile">{"Untrack"}</span>
                                </button>
                            } else {
                                <button class="btn" onclick={link.callback(|_| Msg::SetTracked(true))} title="Track series">
                                    <span class="icon-inline"><span class="icon eye" /></span>
                                    <span class="hide-mobile">{"Track"}</span>
                                </button>
                            }

                            if !series.remotes.is_empty() {
                                <button class="btn" onclick={link.callback(|_| Msg::SyncSeries)} title="Sync from remote">
                                    <span class="icon-inline"><span class={classes!("icon", "arrow-path", self.syncing.then_some("spin"))} /></span>
                                    <span class="hide-mobile">{"Sync"}</span>
                                </button>
                            }

                            if self.confirm_remove {
                                <ConfirmDanger
                                    prompt="Remove series"
                                    label={series.title.clone()}
                                    on_confirm={link.callback(|_| Msg::RemoveSeries)}
                                    on_cancel={link.callback(|_| Msg::CancelRemove)}
                                />
                            } else {
                                <button class="btn btn-danger" onclick={link.callback(|_| Msg::ConfirmRemove)} title="Remove series">
                                    <span class="icon-inline"><span class="icon trash" /></span>
                                    <span class="hide-mobile">{"Remove"}</span>
                                </button>
                            }
                        </div>
                    </div>

                    <div class="detail-layout">
                        { self.view_sidebar(ctx, series) }
                        { self.view_episodes(ctx) }
                    </div>

                    if let Some(kind) = self.image_modal {
                        { self.view_image_modal(ctx, kind, series) }
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
            self.episodes.clear();
            self.confirm_remove = false;
            self.syncing = false;
            self.confirm_remove_watch = None;
            self.watched.clear();

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
                    self.watched.clear();

                    if self.channel.id() != ws::ChannelId::NONE {
                        self.load_episodes(ctx, season);
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
                        Ok(false)
                    }
                    api::AppEventKind::PendingChanged => {
                        if let Some(season) = self.selected {
                            self.load_episodes(ctx, season);
                        }
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
                            self.load_series(ctx);
                            self.load_seasons(ctx);
                            if let Some(season) = self.selected {
                                self.load_episodes(ctx, season);
                            }
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
                                self.load_history(ctx);
                            }
                            self.load_orphaned(ctx);
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

                Ok(false)
            }
            Msg::EpisodesLoaded(result) => {
                let result = result
                    .context(Message::LoadingEpisodes)?
                    .decode()
                    .context(Message::LoadingEpisodes)?;

                self.watched.clear();

                self.episodes = result.episodes;

                for w in result.watched {
                    self.watched.entry(w.episode_id).or_default().push(w);
                }

                self.expanded.retain(|episode_id| {
                    self.watched
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
            Msg::CancelMarkWatch => {
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

                if let Some(season) = self.selected {
                    self.load_episodes(ctx, season);
                }

                self.load_history(ctx);
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
                Ok(false)
            }
            Msg::SetTracked(tracked) => {
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
                if !self.expanded.insert(id) {
                    self.expanded.remove(&id);
                }

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
                    self.watched.entry(w.episode_id).or_default().push(w);
                }

                self.expanded.retain(|episode_id| {
                    self.watched
                        .get(episode_id)
                        .is_some_and(|watched| !watched.is_empty())
                });

                Ok(true)
            }
            Msg::AddPending(episode_id) => {
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

                Ok(false)
            }
            Msg::RemovePending(episode_id) => {
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
            Msg::Back => {
                ctx.props()
                    .on_navigate
                    .emit(Route::Series(PagedQuery::default()));
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

    fn view_header(&self, ctx: &Context<Self>) -> Html {
        let link = ctx.link();

        html! {
            <div class="row page-title">
                <button class="btn" onclick={link.callback(|_| Msg::Back)}>
                    <span class="icon-inline"><span class="icon arrow-left" /></span>
                    {"Series"}
                </button>
                if let Some(ref s) = self.series {
                    if let Some(ref title) = s.title {
                        <span class="fill">{title}</span>
                    } else {
                        <span class="fill text-muted">{"Untitled Series"}</span>
                    }

                    { for s.remotes.iter().filter_map(|r| {
                        let url = r.series_url()?;
                        let label = r.source().as_str().to_uppercase();
                        Some(html! {
                            <a class="btn" href={url} target="_blank" rel="noopener noreferrer" title={format!("Open on {label}")}>
                                <span class="icon-inline"><span class="icon arrow-top-right-on-square" /></span>
                                <span class="hide-mobile">{label}</span>
                            </a>
                        })
                    }) }
                } else {
                    <span class="fill" />
                }
            </div>
        }
    }

    fn view_sidebar(&self, ctx: &Context<Self>, series: &api::Series) -> Html {
        html! {
            <div class="detail-sidebar section">
                if let Some(poster) = series.selected_image(api::ImageKind::Poster) {
                    <img class="poster hide-mobile" src={poster.proxy_url()} />
                }

                <div class="table table-striped">
                    { for self.seasons.iter().map(|s| self.view_season_item(ctx, s)) }
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
        let items: Vec<ImageItem> = series
            .images
            .iter()
            .map(|img| ImageItem {
                id: img.id,
                kind: img.kind,
                source: img.source,
                image: img.image.clone(),
                selected: img.selected,
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

    fn view_season_item(&self, ctx: &Context<Self>, season: &api::Season) -> Html {
        let number = season.number;
        let active = self.selected == Some(number);
        let onclick = ctx.link().callback(move |_| Msg::SelectSeason(number));

        let label = match season.number {
            api::SeasonNumber::Specials => "Specials".to_string(),
            api::SeasonNumber::Number(n) => format!("Season {n}"),
        };

        html! {
            <div class={classes!("table-entry", "row", "clickable", active.then_some("active"))} {onclick}>
                <span class="fill">{label}</span>

                if let Some(ts) = season.air_date {
                    <span class="text-muted">{ts.date(self.tz.clone()).year().to_string()}</span>
                }

                <span class="icon-inline">
                    <span class={classes!("icon", if active { "ellipsis-horizontal" } else { "chevron-right" })} />
                </span>
            </div>
        }
    }

    fn view_episodes(&self, ctx: &Context<Self>) -> Html {
        let link = ctx.link();

        let watched_count = self
            .episodes
            .iter()
            .filter(|ep| self.watched.get(&ep.id).map(Vec::len).unwrap_or_default() > 0)
            .count();
        let total = self.episodes.len();

        html! {
            <div class="detail-content section">
                if let Some(season) = self.selected {
                    <div class="row-fill actions">
                        if self.select_mark_remaining {
                            <MarkWatchedPicker
                                on_confirm={link.callback(move |mark_time| Msg::WatchRemaining(season, mark_time))}
                                on_cancel={link.callback(|_| Msg::CancelMarkRemainingWatch)}
                            />
                        } else {
                            if total > 0 {
                                <span class="text-muted">
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
                }

                if self.episodes.is_empty() && self.selected.is_some() {
                    <div class="empty text-muted">{"No episodes."}</div>
                }

                { for self.episodes.iter().map(|ep| self.view_episode(ctx, ep)) }

                { self.view_orphaned(ctx) }
            </div>
        }
    }

    fn view_episode(&self, ctx: &Context<Self>, ep: &api::Episode) -> Html {
        let link = ctx.link();

        let series_id = ctx.props().series_id;
        let episode_id = ep.id;

        let watched = self
            .watched
            .get(&episode_id)
            .map(Vec::as_slice)
            .unwrap_or_default();

        let expanded = self.expanded.contains(&episode_id);
        let confirming_mark = self.confirming_mark_watch == Some(episode_id);
        let on_ask_mark = link.callback(move |_| Msg::AskMarkWatched(episode_id));
        let on_remove_confirm = watched.last().map(|w| {
            let id = w.id;
            link.callback(move |_| Msg::ConfirmRemoveWatch(id))
        });
        let on_toggle_history =
            (!watched.is_empty()).then(|| link.callback(move |_| Msg::ToggleHistory(episode_id)));
        let on_add_pending = link.callback(move |_| Msg::AddPending(episode_id));
        let on_remove_pending = link.callback(move |_| Msg::RemovePending(episode_id));

        let actions = 'actions: {
            if confirming_mark {
                break 'actions html! {
                    <MarkWatchedPicker
                        on_confirm={link.callback(move |mark_time| Msg::MarkWatched(series_id, episode_id, mark_time))}
                        on_cancel={link.callback(|_| Msg::CancelMarkWatch)}
                    />
                };
            }

            html! {
                <div class="actions row-fill">
                    <div class="row">
                        <span class="episode-code">
                            { format!("S{:02}E{:02}", ep.season.to_u32(), ep.number) }
                        </span>

                        <span class="fill">
                            { ep.name.as_deref().unwrap_or("—") }
                        </span>
                    </div>

                    <div class="row end">
                        if let Some(s) = ep.display_at(self.tz.clone()) {
                            <span class="text-muted">{s}</span>
                        }

                        if !watched.is_empty() {
                            if let Some(on_toggle) = on_toggle_history {
                                <button class="btn-icon" onclick={on_toggle} title={if expanded { "Hide watch history" } else { "Show watch history" }}>
                                    <span class={if expanded { "icon chevron-up" } else { "icon clock" }} />
                                </button>
                            }

                            <button class="btn-icon-success" onclick={on_ask_mark.clone()} title="Watch again">
                                <span class="icon check" />
                            </button>

                            if let Some(on_remove) = on_remove_confirm {
                                <button class="btn-icon" onclick={on_remove} title="Remove last watch">
                                    <span class="icon check-circle" />
                                </button>
                            } else {
                                <span class="icon-inline" title="Watched"><span class="icon check-circle" /></span>
                            }
                        } else {
                            <button class="btn-icon-success" onclick={on_ask_mark} title="Mark watched">
                                <span class="icon check" />
                            </button>
                        }

                        if ep.pending {
                            <button class="btn-icon" onclick={on_remove_pending} title="Remove from watch next">
                                <span class="icon bookmark-slash" />
                            </button>
                        } else {
                            <button class="btn-icon" onclick={on_add_pending} title="Watch next">
                                <span class="icon bookmark" />
                            </button>
                        }
                    </div>
                </div>
            }
        };

        html! {
            <div class={classes!("section", (!watched.is_empty()).then_some("watched"))}>
                {actions}

                if let Some(ref overview) = ep.overview {
                    <p class="overview">{overview}</p>
                }

                if let Some(ref img) = ep.filename {
                    <img src={img.proxy_url()} />
                }

                if expanded {
                    <div class="table">
                        { for watched.iter().map(|w| {
                            let wid = w.id;
                            let kind = api::WatchedKind::Episode { series: series_id, episode: episode_id };

                            if self.confirm_remove_watch == Some(wid) {
                                html! {
                                    <div class="table-entry">
                                        <ConfirmDanger
                                            prompt="Remove watch"
                                            label={w.timestamp.display(self.tz.clone())}
                                            on_confirm={link.callback(move |_| Msg::RemoveWatched(wid, kind))}
                                            on_cancel={link.callback(|_| Msg::CancelRemoveWatch)}
                                        />
                                    </div>
                                }
                            } else if self.fixing_watched == Some(wid) {
                                html! {
                                    <div class="table-entry">
                                        <EpisodePicker
                                            series_id={series_id}
                                            seasons={self.seasons.clone()}
                                            on_confirm={link.callback(move |(season, ep)| Msg::MoveWatched(wid, season, ep))}
                                            on_cancel={link.callback(|_| Msg::CancelFixWatched)}
                                        />
                                    </div>
                                }
                            } else {
                                html! {
                                    <div class="table-entry row">
                                        <span class="fill">{w.timestamp.display(self.tz.clone())}</span>

                                        <button class="btn-icon" onclick={link.callback(move |_| Msg::FixWatched(wid))} title="Move to different episode">
                                            <span class="icon pencil-square" />
                                        </button>

                                        <button class="btn-icon" onclick={link.callback(move |_| Msg::ConfirmRemoveWatch(wid))} title="Remove">
                                            <span class="icon x-mark" />
                                        </button>
                                    </div>
                                }
                            }
                        }) }
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
            <div class="section">
                <div class="outline-title">{"Unrecognized watch entries"}</div>

                <div class="table">
                    { for self.orphaned.iter().map(|w| {
                        let wid = w.id;
                        let season_label = match w.season {
                            api::SeasonNumber::Specials => format!("Sx{:02}", w.episode),
                            api::SeasonNumber::Number(n) => format!("{}x{:02}", n, w.episode),
                        };
                        let kind = api::WatchedKind::Episode { series: series_id, episode: api::EpisodeId::new(0) };

                        if self.fixing_watched == Some(wid) {
                            html! {
                                <div class="table-entry">
                                    <EpisodePicker
                                        {series_id}
                                        seasons={self.seasons.clone()}
                                        on_confirm={link.callback(move |(season, ep)| Msg::MoveWatched(wid, season, ep))}
                                        on_cancel={link.callback(|_| Msg::CancelFixWatched)}
                                    />
                                </div>
                            }
                        } else if self.confirm_remove_watch == Some(wid) {
                            html! {
                                <div class="table-entry">
                                    <ConfirmDanger
                                        prompt="Remove watch"
                                        label={w.timestamp.display(self.tz.clone())}
                                        on_confirm={link.callback(move |_| Msg::RemoveWatched(wid, kind))}
                                        on_cancel={link.callback(|_| Msg::CancelRemoveWatch)}
                                    />
                                </div>
                            }
                        } else {
                            html! {
                                <div class="table-entry row">
                                    <span class="text-muted">{season_label}</span>
                                    <span class="fill">{w.timestamp.display(self.tz.clone())}</span>

                                    <button class="btn-icon" onclick={link.callback(move |_| Msg::FixWatched(wid))} title="Move to episode">
                                        <span class="icon pencil-square" />
                                    </button>

                                    <button class="btn-icon" onclick={link.callback(move |_| Msg::ConfirmRemoveWatch(wid))} title="Remove">
                                        <span class="icon x-mark" />
                                    </button>
                                </div>
                            }
                        }
                    }) }
                </div>
            </div>
        }
    }
}
