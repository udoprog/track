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
    _setup: crate::SetupChannel,
    _broadcast: ws::Listener,
    _series_req: ws::Request,
    _seasons_req: ws::Request,
    _episodes_req: ws::Request,
    _mark_req: ws::Request,
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
            _setup,
            _broadcast,
            _series_req: ws::Request::default(),
            _seasons_req: ws::Request::default(),
            _episodes_req: ws::Request::default(),
            _mark_req: ws::Request::default(),
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
                <div class="row outline-title">
                    <button class="btn" onclick={ctx.link().callback(|_| Msg::Back)}>
                        <span class="icon-inline"><span class="icon arrow-left" /></span>
                        {"Series"}
                    </button>
                    if let Some(ref s) = self.series {
                        <span>{&s.title}</span>
                    }
                </div>
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
                // Auto-select first non-special season, or first season if all special
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

        html! {
            <div class="detail-content">
                if self.episodes.is_empty() && self.selected.is_some() {
                    <div class="empty text-muted">{"No episodes."}</div>
                }
                { for self.episodes.iter().map(|ep| {
                    let episode_id = ep.id;
                    let watched = ep.watched;
                    let on_mark = ctx.link().callback(move |_| Msg::MarkWatched(series_id, episode_id));

                    html! {
                        <div class={classes!("group", "row", watched.then_some("ep-watched"))}>
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
                                <span class="icon-inline" title="Watched"><span class="icon check-circle" /></span>
                            } else {
                                <button class="btn-icon-success" onclick={on_mark} title="Mark watched">
                                    <span class="icon check" />
                                </button>
                            }
                        </div>
                    }
                }) }
            </div>
        }
    }
}
