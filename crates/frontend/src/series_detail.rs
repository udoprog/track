use musli_web::web03::prelude::*;
use yew::prelude::*;

use api::HasAired;

use crate::error::{CustomContext, Error, Message};
use crate::router::{Route, SeriesQuery};
use crate::ui::{ConfirmDanger, ImageGallery, ImageItem, MarkWatchedPicker, RemoteSourceKind, RemoteSourceSelect};

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
    expanded_episode: Option<api::EpisodeId>,
    episode_history: Vec<api::Watched>,
    image_modal: Option<api::ImageKind>,
    _setup: crate::SetupChannel,
    _broadcast: ws::Listener,
    _series_req: ws::Request,
    _seasons_req: ws::Request,
    _episodes_req: ws::Request,
    _mark_req: ws::Request,
    _remove_watch_req: ws::Request,
    _untrack_req: ws::Request,
    _remove_req: ws::Request,
    _sync_req: ws::Request,
    _watch_remaining_reqs: Vec<ws::Request>,
    _history_req: ws::Request,
    _set_next_req: ws::Request,
    _select_image_req: ws::Request,
    _set_sync_source_req: ws::Request,
}

pub(super) enum Msg {
    Channel(Result<ws::Channel, ws::Error>),
    AppBroadcast(Result<ws::Packet<api::AppBroadcast>, ws::Error>),
    SeriesLoaded(Result<ws::Packet<api::GetSeries>, ws::Error>),
    SeasonsLoaded(Result<ws::Packet<api::ListSeasons>, ws::Error>),
    SelectSeason(api::SeasonNumber),
    EpisodesLoaded(Result<ws::Packet<api::ListEpisodes>, ws::Error>),
    AskMarkWatched(api::EpisodeId),
    MarkWatched(api::SeriesId, api::EpisodeId, Option<api::Timestamp>),
    MarkWatchedDone(Result<ws::Packet<api::MarkWatched>, ws::Error>),
    CancelMarkWatch,
    RemoveWatched(api::WatchedId, api::WatchedKind),
    RemoveWatchedDone(Result<ws::Packet<api::RemoveWatched>, ws::Error>),
    ConfirmRemoveWatch(api::WatchedId),
    CancelRemoveWatch,
    WatchRemaining(api::SeasonNumber),
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
    HistoryLoaded(Result<ws::Packet<api::ListWatched>, ws::Error>),
    AddPending(api::EpisodeId),
    AddPendingDone(Result<ws::Packet<api::AddPending>, ws::Error>),
    RemovePending(api::EpisodeId),
    RemovePendingDone(Result<ws::Packet<api::RemovePending>, ws::Error>),
    SelectImage(api::ImageId),
    SelectImageDone(Result<ws::Packet<api::SelectImage>, ws::Error>),
    SetSyncSource(api::SyncSource),
    SetSyncSourceDone(
        api::SyncSource,
        Result<ws::Packet<api::SetSeriesSyncSource>, ws::Error>,
    ),
    OpenImageModal(api::ImageKind),
    CloseImageModal,
    Back,
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

        let _setup = crate::SetupChannel::new(ws.clone(), ctx.link().callback(Msg::Channel));
        let _broadcast = ws.on_broadcast(ctx.link().callback(Msg::AppBroadcast));

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
            expanded_episode: None,
            episode_history: Vec::new(),
            image_modal: None,
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
            _watch_remaining_reqs: Vec::new(),
            _history_req: ws::Request::default(),
            _set_next_req: ws::Request::default(),
            _select_image_req: ws::Request::default(),
            _set_sync_source_req: ws::Request::default(),
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

        let tz = ctx
            .link()
            .context::<crate::SystemTz>(Callback::noop())
            .map(|(t, _)| t.get().clone())
            .unwrap_or(jiff::tz::TimeZone::UTC);

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

                    <div class="row actions">
                        <RemoteSourceSelect
                            kind={RemoteSourceKind::Series}
                            remotes={series.remotes.clone()}
                            current_source={series.effective_sync_source()}
                            on_change={link.callback(Msg::SetSyncSource)}
                        />

                        if series.images.iter().any(|i| matches!(i.kind, api::ImageKind::Banner | api::ImageKind::Fanart | api::ImageKind::Backdrop)) {
                            <button class="btn" onclick={link.callback(|_| Msg::OpenImageModal(api::ImageKind::Backdrop))}>
                                <span class="icon-inline"><span class="icon photo" /></span>
                                {"Background"}
                            </button>
                        }

                        if let Some(ts) = series.last_synced_at {
                            <span class="text-muted">
                                {"Synced "}
                                {ts.display(&tz)}
                            </span>
                        }
                    </div>

                    <div class="detail-layout">
                        { self.view_sidebar(ctx, series) }
                        { self.view_episodes(ctx, series) }
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
            self.expanded_episode = None;
            self.episode_history.clear();
            if self.channel.id() != ws::ChannelId::NONE {
                self.load_series(ctx);
                self.load_seasons(ctx);
            }
        } else if ctx.props().initial_season != old_props.initial_season {
            if let Some(season) = ctx.props().initial_season {
                if self.selected != Some(season) {
                    self.selected = Some(season);
                    self.episodes.clear();
                    self.confirm_remove_watch = None;
                    self.expanded_episode = None;
                    self.episode_history.clear();
                    if self.channel.id() != ws::ChannelId::NONE {
                        self.load_episodes(ctx, season);
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
                } else {
                    self.series = None;
                    self.seasons.clear();
                    self.episodes.clear();
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
                            return Ok(true);
                        }
                        Ok(false)
                    }
                    api::AppEventKind::WatchedChanged { kind } => {
                        let relevant = match kind {
                            api::WatchedKind::Episode { series, .. } => {
                                *series == ctx.props().series_id
                            }
                            api::WatchedKind::Movie { .. } => false,
                        };

                        if relevant && let Some(season) = self.selected {
                            self.load_episodes(ctx, season);
                            self.load_episode_history(ctx);
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
                        SeriesQuery {
                            season: Some(season),
                        },
                    ));
                }
                Ok(false)
            }
            Msg::EpisodesLoaded(result) => {
                self.episodes = result
                    .context(Message::LoadingEpisodes)?
                    .decode()
                    .context(Message::LoadingEpisodes)?
                    .episodes;
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
            Msg::MarkWatched(series, episode, timestamp) => {
                self.confirming_mark_watch = None;
                self._mark_req = self
                    .channel
                    .request()
                    .body(api::MarkWatchedRequest {
                        kind: api::WatchedKind::Episode { series, episode },
                        timestamp,
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
                self.load_episode_history(ctx);
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
                self.load_episode_history(ctx);
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
            Msg::WatchRemaining(season) => {
                let series_id = ctx.props().series_id;
                let reqs: Vec<ws::Request> = self
                    .episodes
                    .iter()
                    .filter(|ep| ep.season == season && !ep.watched)
                    .map(|ep| {
                        let episode_id = ep.id;
                        self.channel
                            .request()
                            .body(api::MarkWatchedRequest {
                                kind: api::WatchedKind::Episode {
                                    series: series_id,
                                    episode: episode_id,
                                },
                                timestamp: None,
                            })
                            .on_packet(ctx.link().callback(Msg::WatchRemainingDone))
                            .send()
                    })
                    .collect();
                self._watch_remaining_reqs = reqs;
                Ok(false)
            }
            Msg::WatchRemainingDone(result) => {
                result.context(Message::MarkingWatched)?;
                if let Some(season) = self.selected {
                    self.load_episodes(ctx, season);
                }
                self.load_episode_history(ctx);
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
                ctx.props().on_navigate.emit(Route::Series);
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
                Ok(false)
            }
            Msg::SyncDone(result) => {
                result.context(Message::SyncingSeries)?;
                Ok(false)
            }
            Msg::ToggleHistory(id) => {
                if self.expanded_episode == Some(id) {
                    self.expanded_episode = None;
                    self.episode_history.clear();
                } else {
                    self.expanded_episode = Some(id);
                    self.episode_history.clear();
                    self.load_episode_history(ctx);
                }
                Ok(true)
            }
            Msg::HistoryLoaded(result) => {
                self.episode_history = result
                    .context(Message::LoadingWatched)?
                    .decode()
                    .context(Message::LoadingWatched)?
                    .watched;
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
            Msg::SelectImageDone(result) => {
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
            Msg::OpenImageModal(kind) => {
                self.image_modal = Some(kind);
                Ok(true)
            }
            Msg::CloseImageModal => {
                self.image_modal = None;
                Ok(true)
            }
            Msg::Back => {
                ctx.props().on_navigate.emit(Route::Series);
                Ok(false)
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

    fn load_episode_history(&mut self, ctx: &Context<Self>) {
        let Some(episode) = self.expanded_episode else {
            return;
        };
        let series = ctx.props().series_id;
        self._history_req = self
            .channel
            .request()
            .body(api::ListWatchedRequest {
                kind: api::WatchedKind::Episode { series, episode },
            })
            .on_packet(ctx.link().callback(Msg::HistoryLoaded))
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
                    <span class="fill">{&s.title}</span>
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
                    if s.tracked {
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

                    if !s.remotes.is_empty() {
                        <button class="btn" onclick={link.callback(|_| Msg::SyncSeries)} title="Sync from remote">
                            <span class="icon-inline"><span class={classes!("icon", "arrow-path", self.syncing.then_some("spin"))} /></span>
                            <span class="hide-mobile">{"Sync"}</span>
                        </button>
                    }

                    if self.confirm_remove {
                        <ConfirmDanger
                            prompt="Remove series"
                            label={s.title.clone()}
                            on_confirm={link.callback(|_| Msg::RemoveSeries)}
                            on_cancel={link.callback(|_| Msg::CancelRemove)}
                        />
                    } else {
                        <button class="btn btn-danger" onclick={link.callback(|_| Msg::ConfirmRemove)} title="Remove series">
                            <span class="icon-inline"><span class="icon trash" /></span>
                            <span class="hide-mobile">{"Remove"}</span>
                        </button>
                    }
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

                if let Some(date) = season.air_date {
                    <span class="text-muted">{date.year().to_string()}</span>
                }

                <span class="icon-inline">
                    <span class={classes!("icon", if active { "ellipsis-horizontal" } else { "chevron-right" })} />
                </span>
            </div>
        }
    }

    fn view_episodes(&self, ctx: &Context<Self>, series: &api::Series) -> Html {
        let series_id = ctx.props().series_id;
        let link = ctx.link();
        let tz = ctx
            .link()
            .context::<crate::SystemTz>(Callback::noop())
            .map(|(t, _)| t.get().clone())
            .unwrap_or(jiff::tz::TimeZone::UTC);

        let watched_count = self.episodes.iter().filter(|ep| ep.watched).count();
        let total = self.episodes.len();

        html! {
            <div class="detail-content section">
                if let Some(season) = self.selected {
                    <div class="row actions">
                        if total > 0 {
                            <span class="text-muted">
                                {format!("{watched_count} / {total} watched")}
                            </span>
                        }

                        if watched_count < total {
                            <button class="btn" onclick={link.callback(move |_| Msg::WatchRemaining(season))}>
                                {"Watch remaining"}
                            </button>
                        }
                    </div>
                }

                if self.episodes.is_empty() && self.selected.is_some() {
                    <div class="empty text-muted">{"No episodes."}</div>
                }

                { for self.episodes.iter().map(|ep| {
                    let episode_id = ep.id;
                    let watched = ep.watched;
                    let last_watched_id = ep.last_watched_id;
                    let expanded = self.expanded_episode == Some(episode_id);
                    let confirming_mark = self.confirming_mark_watch == Some(episode_id);
                    let on_ask_mark = link.callback(move |_| Msg::AskMarkWatched(episode_id));
                    let aired_at = ep.aired_at;
                    let aired = ep.aired;
                    let on_remove_confirm = last_watched_id.map(|wid| {
                        link.callback(move |_| Msg::ConfirmRemoveWatch(wid))
                    });
                    let confirming_remove_watch = last_watched_id
                        .map_or(false, |wid| self.confirm_remove_watch == Some(wid));
                    let series_title: AttrValue = AttrValue::from(series.title.clone());
                    let on_toggle_history = watched.then(|| {
                        link.callback(move |_| Msg::ToggleHistory(episode_id))
                    });
                    let on_add_pending = link.callback(move |_| Msg::AddPending(episode_id));
                    let on_remove_pending = link.callback(move |_| Msg::RemovePending(episode_id));

                    let actions = 'actions: {
                        if confirming_mark {
                            break 'actions html! {
                                <MarkWatchedPicker
                                    {aired_at}
                                    {aired}
                                    on_confirm={link.callback(move |ts| Msg::MarkWatched(series_id, episode_id, ts))}
                                    on_cancel={link.callback(|_| Msg::CancelMarkWatch)}
                                />
                            };
                        }

                        if let Some(wid) = last_watched_id && confirming_remove_watch {
                            break 'actions html! {
                                <ConfirmDanger
                                    prompt="Remove watch for"
                                    label={series_title}
                                    on_confirm={link.callback(move |_| Msg::RemoveWatched(wid, api::WatchedKind::Episode { series: series_id, episode: episode_id }))}
                                    on_cancel={link.callback(|_| Msg::CancelRemoveWatch)}
                                />
                            };
                        }

                        html! {
                            <div class="actions row">
                                <span class="episode-code">
                                    { format!("S{:02}E{:02}", ep.season.to_i64(), ep.number) }
                                </span>

                                <span class="fill">
                                    { ep.name.as_deref().unwrap_or("—") }
                                </span>

                                if let Some(s) = ep.display_at(&tz) {
                                    <span class="text-muted">{s}</span>
                                }

                                if watched {
                                    if let Some(on_toggle) = on_toggle_history {
                                        <button class="btn-icon" onclick={on_toggle}
                                            title={if expanded { "Hide watch history" } else { "Show watch history" }}>
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
                                    <button class="btn-icon" onclick={on_remove_pending} title="Remove from pending">
                                        <span class="icon bookmark-slash" />
                                    </button>
                                } else {
                                    <button class="btn-icon" onclick={on_add_pending} title="Add to pending">
                                        <span class="icon bookmark" />
                                    </button>
                                }
                            </div>
                        }
                    };

                    html! {
                        <div class={classes!("section", watched.then_some("watched"))}>
                            {actions}

                            if !ep.overview.is_empty() {
                                <p class="overview">{&ep.overview}</p>
                            }

                            if expanded {
                                if let Some(ref img) = ep.filename {
                                    <img class="poster-sm" src={img.proxy_url()} />
                                }
                                <div class="table">
                                    { for self.episode_history.iter().map(|w| {
                                        let wid = w.id;
                                        let wkind = api::WatchedKind::Episode { series: series_id, episode: episode_id };
                                        if self.confirm_remove_watch == Some(wid) {
                                            html! {
                                                <div class="table-entry">
                                                    <ConfirmDanger
                                                        prompt="Remove watch"
                                                        label={w.timestamp.display(&tz)}
                                                        on_confirm={link.callback(move |_| Msg::RemoveWatched(wid, wkind))}
                                                        on_cancel={link.callback(|_| Msg::CancelRemoveWatch)}
                                                    />
                                                </div>
                                            }
                                        } else {
                                            html! {
                                                <div class="table-entry text-muted">
                                                    <span class="fill">{w.timestamp.display(&tz)}</span>
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
                }) }
            </div>
        }
    }
}
