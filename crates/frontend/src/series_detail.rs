use musli_web::web03::prelude::*;
use yew::prelude::*;

use crate::error::{CustomContext, Error, Message};
use crate::router::Route;
use crate::ui::ConfirmDanger;

pub(super) struct SeriesDetail {
    channel: ws::Channel,
    series: Option<api::Series>,
    seasons: Vec<api::Season>,
    selected: Option<api::SeasonNumber>,
    episodes: Vec<api::Episode>,
    confirm_remove: bool,
    confirm_remove_watch: Option<api::EpisodeId>,
    expanded_episode: Option<api::EpisodeId>,
    episode_history: Vec<api::Watched>,
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
}

pub(super) enum Msg {
    Channel(Result<ws::Channel, ws::Error>),
    AppBroadcast(Result<ws::Packet<api::AppBroadcast>, ws::Error>),
    SeriesLoaded(Result<ws::Packet<api::GetSeries>, ws::Error>),
    SeasonsLoaded(Result<ws::Packet<api::ListSeasons>, ws::Error>),
    SelectSeason(api::SeasonNumber),
    EpisodesLoaded(Result<ws::Packet<api::ListEpisodes>, ws::Error>),
    MarkWatched(api::SeriesId, api::EpisodeId),
    MarkWatchedDone(Result<ws::Packet<api::MarkWatched>, ws::Error>),
    RemoveWatched(api::WatchedId, api::WatchedKind),
    RemoveWatchedDone(Result<ws::Packet<api::RemoveWatched>, ws::Error>),
    ConfirmRemoveWatch(api::EpisodeId),
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
    SetNextEpisode(Option<api::EpisodeId>),
    SetNextEpisodeDone(
        Option<api::EpisodeId>,
        Result<ws::Packet<api::SetNextEpisode>, ws::Error>,
    ),
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
            confirm_remove_watch: None,
            expanded_episode: None,
            episode_history: Vec::new(),
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
        html! {
            <div class="page">
                { self.view_header(ctx) }
                if let Some(banner) = self.series.as_ref().and_then(|s| s.banner.as_ref()) {
                    <img class="banner" src={banner.proxy_url()} alt="" />
                }
                <div class="detail-layout">
                    { self.view_sidebar(ctx) }
                    { self.view_episodes(ctx) }
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
            self.confirm_remove_watch = None;
            self.expanded_episode = None;
            self.episode_history.clear();
            if self.channel.id() != ws::ChannelId::NONE {
                self.load_series(ctx);
                self.load_seasons(ctx);
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
                    api::AppEventKind::WatchedChanged { kind } => {
                        let relevant = match kind {
                            api::WatchedKind::Episode { series, .. } => {
                                *series == ctx.props().series_id
                            }
                            api::WatchedKind::Movie { .. } => false,
                        };

                        if relevant && let Some(season) = self.selected {
                            self.load_episodes(ctx, season);
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
                    self.selected = Some(season);
                    self.episodes.clear();
                    self.confirm_remove_watch = None;
                    self.expanded_episode = None;
                    self.episode_history.clear();
                    self.load_episodes(ctx, season);
                }
                Ok(true)
            }
            Msg::EpisodesLoaded(result) => {
                self.episodes = result
                    .context(Message::LoadingEpisodes)?
                    .decode()
                    .context(Message::LoadingEpisodes)?
                    .episodes;
                Ok(true)
            }
            Msg::MarkWatched(series, episode) => {
                self._mark_req = self
                    .channel
                    .request()
                    .body(api::MarkWatchedRequest {
                        kind: api::WatchedKind::Episode { series, episode },
                        timestamp: None,
                    })
                    .on_packet(ctx.link().callback(Msg::MarkWatchedDone))
                    .send();
                Ok(false)
            }
            Msg::MarkWatchedDone(result) => {
                result.context(Message::MarkingWatched)?;
                if let Some(season) = self.selected {
                    self.load_episodes(ctx, season);
                }
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
                    let series = ctx.props().series_id;
                    self._history_req = self
                        .channel
                        .request()
                        .body(api::ListWatchedRequest {
                            kind: api::WatchedKind::Episode {
                                series,
                                episode: id,
                            },
                        })
                        .on_packet(ctx.link().callback(Msg::HistoryLoaded))
                        .send();
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
            Msg::SetNextEpisode(episode_id) => {
                let series_id = ctx.props().series_id;
                self._set_next_req = self
                    .channel
                    .request()
                    .body(api::SetNextEpisodeRequest {
                        series_id,
                        episode_id,
                    })
                    .on_packet(
                        ctx.link()
                            .callback(move |r| Msg::SetNextEpisodeDone(episode_id, r)),
                    )
                    .send();
                Ok(false)
            }
            Msg::SetNextEpisodeDone(episode_id, result) => {
                result.context(Message::SyncingSeries)?;
                if let Some(ref mut series) = self.series {
                    series.pending_episode_id = episode_id;
                }
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
                    if !s.tracked {
                        <span class="license">{"Untracked"}</span>
                    }
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
                    if s.remote_id.is_some() {
                        <button class="btn" onclick={link.callback(|_| Msg::SyncSeries)} title="Sync from remote">
                            <span class="icon-inline"><span class="icon arrow-path" /></span>
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

    fn view_sidebar(&self, ctx: &Context<Self>) -> Html {
        html! {
            <div class="detail-sidebar section">
                { for self.seasons.iter().map(|s| self.view_season_item(ctx, s)) }
            </div>
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
            <div class={classes!("section", "row", "clickable", active.then_some("active"))} {onclick}>
                <span class="fill">{label}</span>
                if let Some(date) = season.air_date {
                    <span class="text-muted">{date.year().to_string()}</span>
                }
            </div>
        }
    }

    fn view_episodes(&self, ctx: &Context<Self>) -> Html {
        let series_id = ctx.props().series_id;
        let link = ctx.link();

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
                    let is_next = self.series.as_ref()
                        .and_then(|s| s.pending_episode_id)
                        == Some(episode_id);
                    let on_mark = link.callback(move |_| Msg::MarkWatched(series_id, episode_id));
                    let on_remove_confirm = last_watched_id.map(|_| {
                        link.callback(move |_| Msg::ConfirmRemoveWatch(episode_id))
                    });
                    let confirming_remove_watch = self.confirm_remove_watch == Some(episode_id);
                    let series_title: AttrValue = self.series.as_ref()
                        .map(|s| AttrValue::from(s.title.clone()))
                        .unwrap_or_default();
                    let on_toggle_history = watched.then(|| {
                        link.callback(move |_| Msg::ToggleHistory(episode_id))
                    });
                    let on_set_next = if is_next {
                        link.callback(|_| Msg::SetNextEpisode(None))
                    } else {
                        link.callback(move |_| Msg::SetNextEpisode(Some(episode_id)))
                    };

                    let actions = 'actions: {
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

                                if is_next {
                                    <span class="license">{"Next"}</span>
                                }

                                if let Some(date) = ep.aired {
                                    <span class="text-muted">{date.to_string()}</span>
                                }

                                if watched {
                                    if let Some(on_toggle) = on_toggle_history {
                                        <button class="btn-icon" onclick={on_toggle}
                                            title={if expanded { "Hide watch history" } else { "Show watch history" }}>
                                            <span class={if expanded { "icon chevron-up" } else { "icon clock" }} />
                                        </button>
                                    }

                                    <button class="btn-icon-success" onclick={on_mark.clone()} title="Watch again">
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
                                    <button class="btn-icon-success" onclick={on_mark} title="Mark watched">
                                        <span class="icon check" />
                                    </button>
                                }

                                <button class={if is_next { "btn-icon-primary" } else { "btn-icon" }}
                                    onclick={on_set_next}
                                    title={if is_next { "Clear next episode" } else { "Set as next episode" }}>
                                    <span class="icon bookmark" />
                                </button>
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
                                <div class="table">
                                    { for self.episode_history.iter().map(|w| html! {
                                        <div class="table-entry text-muted">
                                            <span>{w.timestamp.to_string()}</span>
                                        </div>
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
