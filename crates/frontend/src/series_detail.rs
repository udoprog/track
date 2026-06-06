use musli_web::web03::prelude::*;
use yew::prelude::*;

use crate::error::{CustomContext, Error, Message};
use crate::router::Route;

pub(super) struct SeriesDetail {
    channel: ws::Channel,
    series: Option<api::Series>,
    seasons: Vec<api::Season>,
    selected: Option<api::SeasonNumber>,
    episodes: Vec<api::Episode>,
    confirm_remove: bool,
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
    WatchRemaining(api::SeasonNumber),
    WatchRemainingDone(Result<ws::Packet<api::MarkWatched>, ws::Error>),
    UntrackSeries,
    UntrackDone(Result<ws::Packet<api::UntrackSeries>, ws::Error>),
    ConfirmRemove,
    CancelRemove,
    RemoveSeries,
    RemoveDone(Result<ws::Packet<api::RemoveSeries>, ws::Error>),
    SyncSeries,
    SyncDone(Result<ws::Packet<api::SyncSeries>, ws::Error>),
    Back,
}

#[derive(Properties, PartialEq)]
pub(super) struct Props {
    pub(super) series_id: api::SeriesId,
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
            <div class="outline">
                { self.view_header(ctx) }
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
                        if relevant {
                            if let Some(season) = self.selected {
                                self.load_episodes(ctx, season);
                            }
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
                    self.selected = self
                        .seasons
                        .iter()
                        .find(|s| !s.number.is_special())
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
                if let Some(season) = self.selected {
                    self.load_episodes(ctx, season);
                }
                Ok(false)
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
            Msg::UntrackSeries => {
                let id = ctx.props().series_id;
                self._untrack_req = self
                    .channel
                    .request()
                    .body(api::UntrackSeriesRequest { id })
                    .on_packet(ctx.link().callback(Msg::UntrackDone))
                    .send();
                Ok(false)
            }
            Msg::UntrackDone(result) => {
                result.context(Message::UntrackingSeries)?;
                if let Some(ref mut series) = self.series {
                    series.tracked = false;
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
            <div class="row outline-title">
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
                        <button class="btn" onclick={link.callback(|_| Msg::UntrackSeries)} title="Untrack series">
                            <span class="icon-inline"><span class="icon eye-slash" /></span>
                            <span class="hide-mobile">{"Untrack"}</span>
                        </button>
                    } else {
                        <button class="btn" onclick={link.callback(|_| Msg::UntrackSeries)} title="Track series">
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
                        <button class="btn btn-danger" onclick={link.callback(|_| Msg::RemoveSeries)} title="Confirm remove">
                            {"Confirm remove"}
                        </button>
                        <button class="btn" onclick={link.callback(|_| Msg::CancelRemove)}>
                            {"Cancel"}
                        </button>
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
            <div class="detail-sidebar">
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
            <div class={classes!("group", "row", "clickable", active.then_some("active"))} {onclick}>
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
            <div class="detail-content">
                if let Some(season) = self.selected {
                    <div class="row season-actions">
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
                    let on_mark = link.callback(move |_| Msg::MarkWatched(series_id, episode_id));
                    let on_remove = last_watched_id.map(|wid| {
                        let kind = api::WatchedKind::Episode { series: series_id, episode: episode_id };
                        link.callback(move |_| Msg::RemoveWatched(wid, kind))
                    });

                    html! {
                        <div class={classes!("group", watched.then_some("ep-watched"))}>
                            <div class="row">
                                <span class="ep-code">
                                    { format!("S{:02}E{:02}", ep.season.to_i64(), ep.number) }
                                </span>
                                <span class="fill">
                                    { ep.name.as_deref().unwrap_or("—") }
                                </span>
                                if let Some(date) = ep.aired {
                                    <span class="text-muted">{date.to_string()}</span>
                                }
                                if ep.watched_count > 1 {
                                    <span class="text-muted">{ep.watched_count}{"×"}</span>
                                }
                                if watched {
                                    if let Some(on_remove) = on_remove {
                                        <button class="btn-icon" onclick={on_remove} title="Remove watch">
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
                            </div>
                            if !ep.overview.is_empty() {
                                <p class="ep-overview">{&ep.overview}</p>
                            }
                        </div>
                    }
                }) }
            </div>
        }
    }
}
