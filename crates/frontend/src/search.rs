use musli_web::web03::prelude::*;
use wasm_bindgen::JsCast as _;
use yew::prelude::*;

use crate::error::{CustomContext, Error, Message, RcError};
use crate::router::{Route, SeriesDetailQuery};
use crate::ui::ErrorBox;
use crate::{Image, SetupChannel};

pub(super) struct Search {
    channel: ws::Channel,
    query: String,
    kind: api::SearchKind,
    series: Vec<api::SearchSeries>,
    movies: Vec<api::SearchMovie>,
    _setup: SetupChannel,
    _broadcast: ws::Listener,
    _search_req: ws::Request,
    _track_req: ws::Request,
}

pub(super) enum Msg {
    Channel(Result<ws::Channel, ws::Error>),
    AppBroadcast(Result<ws::Packet<api::AppBroadcast>, ws::Error>),
    QueryInput(String),
    KindChanged(api::SearchKind),
    Submit,
    SearchDone(Result<ws::Packet<api::Search>, ws::Error>),
    TrackSeries(api::RemoteId),
    TrackMovie(api::RemoteId),
    TrackSeriesDone(Result<ws::Packet<api::TrackSeries>, ws::Error>),
    TrackMovieDone(Result<ws::Packet<api::TrackMovie>, ws::Error>),
    Navigate(Route),
}

#[derive(Properties, PartialEq)]
pub(super) struct Props {
    pub(super) error: Option<RcError>,
    pub(super) onerror: Callback<Option<Error>>,
    pub(super) on_navigate: Callback<Route>,
}

impl Component for Search {
    type Message = Msg;
    type Properties = Props;

    fn create(ctx: &Context<Self>) -> Self {
        let (ws, _) = ctx
            .link()
            .context::<ws::Handle>(Callback::noop())
            .expect("ws::Handle context not found");

        let _setup = SetupChannel::new(ws.clone(), ctx.link().callback(Msg::Channel));
        let _broadcast = ws.on_broadcast(ctx.link().callback(Msg::AppBroadcast));

        Self {
            channel: ws::Channel::default(),
            query: String::new(),
            kind: api::SearchKind::Series,
            series: Vec::new(),
            movies: Vec::new(),
            _setup,
            _broadcast,
            _search_req: ws::Request::default(),
            _track_req: ws::Request::default(),
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
        let link = ctx.link();

        let on_input = link.callback(|e: InputEvent| {
            let input = e
                .target()
                .and_then(|t| t.dyn_into::<web_sys::HtmlInputElement>().ok());
            Msg::QueryInput(input.map(|i| i.value()).unwrap_or_default())
        });

        let on_keydown = link.batch_callback(|e: KeyboardEvent| {
            if e.key() == "Enter" {
                Some(Msg::Submit)
            } else {
                None
            }
        });

        let on_kind = link.callback(|e: Event| {
            let select = e
                .target()
                .and_then(|t| t.dyn_into::<web_sys::HtmlSelectElement>().ok());
            let val = select.map(|s| s.value()).unwrap_or_default();
            Msg::KindChanged(if val == "movies" {
                api::SearchKind::Movies
            } else {
                api::SearchKind::Series
            })
        });

        let on_submit = link.callback(|_| Msg::Submit);

        let kind_val = match self.kind {
            api::SearchKind::Series => "series",
            api::SearchKind::Movies => "movies",
        };

        html! {
            <div class="page">
                if let Some(ref error) = ctx.props().error {
                    <ErrorBox error={error.clone()} onclearerror={ctx.props().onerror.reform(|()| None)} />
                }

                <div class="page-title">{"Search"}</div>

                <div class="row">
                    <select class="input-select" onchange={on_kind} value={kind_val}>
                        <option value="series" selected={matches!(self.kind, api::SearchKind::Series)}>
                            {"Series"}
                        </option>

                        <option value="movies" selected={matches!(self.kind, api::SearchKind::Movies)}>
                            {"Movies"}
                        </option>
                    </select>

                    <input
                        class="input-text fill"
                        type="text"
                        placeholder="Search…"
                        value={self.query.clone()}
                        oninput={on_input}
                        onkeydown={on_keydown}
                    />

                    <button class="btn" onclick={on_submit}>{"Search"}</button>
                </div>

                { self.view_results(ctx) }
            </div>
        }
    }
}

impl Search {
    fn try_update(&mut self, ctx: &Context<Self>, msg: Msg) -> Result<bool, Error> {
        match msg {
            Msg::Channel(result) => {
                self.channel = result?;
                Ok(false)
            }
            Msg::AppBroadcast(packet) => {
                let event = packet?.decode_event()?;
                if event.channel == self.channel.id() {
                    return Ok(false);
                }
                match event.kind {
                    api::AppEventKind::SeriesCreated { series } => {
                        for remote_id in &series.remotes {
                            for r in &mut self.series {
                                if &r.remote_id == remote_id {
                                    r.already_tracked = Some(series.id);
                                }
                            }
                        }
                        Ok(true)
                    }
                    api::AppEventKind::MovieCreated { movie } => {
                        for remote_id in &movie.remotes {
                            for r in &mut self.movies {
                                if &r.remote_id == remote_id {
                                    r.already_tracked = Some(movie.id);
                                }
                            }
                        }
                        Ok(true)
                    }
                    _ => Ok(false),
                }
            }
            Msg::QueryInput(q) => {
                self.query = q;
                Ok(false)
            }
            Msg::KindChanged(kind) => {
                self.kind = kind;
                self.series.clear();
                self.movies.clear();
                Ok(true)
            }
            Msg::Submit => {
                if self.query.is_empty() || self.channel.id() == ws::ChannelId::NONE {
                    return Ok(false);
                }
                self._search_req = self
                    .channel
                    .request()
                    .body(api::SearchRequest {
                        kind: self.kind,
                        query: self.query.clone(),
                    })
                    .on_packet(ctx.link().callback(Msg::SearchDone))
                    .send();
                Ok(false)
            }
            Msg::SearchDone(result) => {
                let resp = result
                    .context(Message::Searching)?
                    .decode()
                    .context(Message::Searching)?;
                self.series = resp.series;
                self.movies = resp.movies;
                Ok(true)
            }
            Msg::TrackSeries(remote_id) => {
                self._track_req = self
                    .channel
                    .request()
                    .body(api::TrackSeriesRequest { remote_id })
                    .on_packet(ctx.link().callback(Msg::TrackSeriesDone))
                    .send();
                Ok(false)
            }
            Msg::TrackMovie(remote_id) => {
                self._track_req = self
                    .channel
                    .request()
                    .body(api::TrackMovieRequest { remote_id })
                    .on_packet(ctx.link().callback(Msg::TrackMovieDone))
                    .send();
                Ok(false)
            }
            Msg::TrackSeriesDone(result) => {
                let series = result
                    .context(Message::TrackingSeries)?
                    .decode()
                    .context(Message::TrackingSeries)?;
                ctx.props()
                    .on_navigate
                    .emit(Route::SeriesDetail(series.id, SeriesDetailQuery::default()));
                Ok(false)
            }
            Msg::TrackMovieDone(result) => {
                let movie = result
                    .context(Message::TrackingMovie)?
                    .decode()
                    .context(Message::TrackingMovie)?;
                ctx.props().on_navigate.emit(Route::MovieDetail(movie.id));
                Ok(false)
            }
            Msg::Navigate(route) => {
                ctx.props().on_navigate.emit(route);
                Ok(false)
            }
        }
    }

    fn view_results(&self, ctx: &Context<Self>) -> Html {
        if self.series.is_empty() && self.movies.is_empty() {
            return html! {};
        }

        html! {
            <>
                { for self.series.iter().map(|r| self.view_series_result(ctx, r)) }
                { for self.movies.iter().map(|r| self.view_movie_result(ctx, r)) }
            </>
        }
    }

    fn view_series_result(&self, ctx: &Context<Self>, r: &api::SearchSeries) -> Html {
        let remote_id = r.remote_id.clone();
        let series_id = r.already_tracked;

        html! {
            <div class="row">
                <Image class="poster-sm" src={r.poster.clone()} />

                <div class="fill">
                    <div class="row">
                        if let Some(ref title) = r.title {
                            <span class="fill">{title}</span>
                        }

                        if let Some(date) = r.first_air_date {
                            <span class="text-muted">{date.year().to_string()}</span>
                        }
                    </div>

                    if let Some(ref overview) = r.overview {
                        <p class="overview text-muted">{overview}</p>
                    }
                </div>

                {
                    if let Some(id) = series_id {
                        let on_nav = ctx.link().callback(move |_| Msg::Navigate(Route::SeriesDetail(id, SeriesDetailQuery::default())));
                        html! {
                            <button class="btn" onclick={on_nav} title="Already tracked">
                                <span class="icon-inline"><span class="icon check" /></span>
                                <span class="hide-mobile">{"Tracked"}</span>
                            </button>
                        }
                    } else {
                        let on_track = ctx.link().callback(move |_| Msg::TrackSeries(remote_id.clone()));
                        html! {
                            <button class="btn" onclick={on_track} title="Track series">
                                <span class="icon-inline"><span class="icon plus" /></span>
                                <span class="hide-mobile">{"Track"}</span>
                            </button>
                        }
                    }
                }
            </div>
        }
    }

    fn view_movie_result(&self, ctx: &Context<Self>, r: &api::SearchMovie) -> Html {
        let remote_id = r.remote_id.clone();
        let movie_id = r.already_tracked;

        html! {
            <div class="row">
                <Image class="poster-sm" src={r.poster.clone()} />

                <div class="fill">
                    <div class="row">
                        if let Some(ref title) = r.title {
                            <span class="fill">{title}</span>
                        }

                        if let Some(date) = r.release_date {
                            <span class="text-muted">{date.year().to_string()}</span>
                        }
                    </div>

                    if let Some(ref overview) = r.overview {
                        <p class="overview text-muted">{overview}</p>
                    }
                </div>

                {
                    if let Some(id) = movie_id {
                        let on_nav = ctx.link().callback(move |_| Msg::Navigate(Route::MovieDetail(id)));
                        html! {
                            <button class="btn" onclick={on_nav} title="Already tracked">
                                <span class="icon-inline"><span class="icon check" /></span>
                                <span class="hide-mobile">{"Tracked"}</span>
                            </button>
                        }
                    } else {
                        let on_track = ctx.link().callback(move |_| Msg::TrackMovie(remote_id.clone()));
                        html! {
                            <button class="btn" onclick={on_track} title="Track movie">
                                <span class="icon-inline"><span class="icon plus" /></span>
                                <span class="hide-mobile">{"Track"}</span>
                            </button>
                        }
                    }
                }
            </div>
        }
    }
}
