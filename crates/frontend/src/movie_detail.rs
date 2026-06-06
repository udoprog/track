use musli_web::web03::prelude::*;
use yew::prelude::*;

use crate::error::{CustomContext, Error, Message};
use crate::router::Route;

pub(super) struct MovieDetail {
    channel: ws::Channel,
    movie: Option<api::Movie>,
    watched: Vec<api::Watched>,
    confirm_remove: bool,
    _setup: crate::SetupChannel,
    _broadcast: ws::Listener,
    _movie_req: ws::Request,
    _watched_req: ws::Request,
    _mark_req: ws::Request,
    _remove_watch_req: ws::Request,
    _remove_req: ws::Request,
    _sync_req: ws::Request,
}

pub(super) enum Msg {
    Channel(Result<ws::Channel, ws::Error>),
    AppBroadcast(Result<ws::Packet<api::AppBroadcast>, ws::Error>),
    MovieLoaded(Result<ws::Packet<api::GetMovie>, ws::Error>),
    WatchedLoaded(Result<ws::Packet<api::ListWatched>, ws::Error>),
    MarkWatched,
    MarkWatchedDone(Result<ws::Packet<api::MarkWatched>, ws::Error>),
    RemoveWatched(api::WatchedId, api::WatchedKind),
    RemoveWatchedDone(Result<ws::Packet<api::RemoveWatched>, ws::Error>),
    ConfirmRemove,
    CancelRemove,
    RemoveMovie,
    RemoveDone(Result<ws::Packet<api::RemoveMovie>, ws::Error>),
    SyncMovie,
    SyncDone(Result<ws::Packet<api::SyncMovie>, ws::Error>),
    Back,
}

#[derive(Properties, PartialEq)]
pub(super) struct Props {
    pub(super) movie_id: api::MovieId,
    pub(super) onerror: Callback<Error>,
    pub(super) on_navigate: Callback<Route>,
}

impl Component for MovieDetail {
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
            movie: None,
            watched: Vec::new(),
            confirm_remove: false,
            _setup,
            _broadcast,
            _movie_req: ws::Request::default(),
            _watched_req: ws::Request::default(),
            _mark_req: ws::Request::default(),
            _remove_watch_req: ws::Request::default(),
            _remove_req: ws::Request::default(),
            _sync_req: ws::Request::default(),
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
                { self.view_body(ctx) }
            </div>
        }
    }

    fn changed(&mut self, ctx: &Context<Self>, old_props: &Props) -> bool {
        if ctx.props().movie_id != old_props.movie_id {
            self.movie = None;
            self.watched.clear();
            self.confirm_remove = false;
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
                } else {
                    self.movie = None;
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
                        self.movie = Some(movie.clone());
                        Ok(true)
                    }
                    api::AppEventKind::MovieDeleted { movie_id }
                        if *movie_id == ctx.props().movie_id =>
                    {
                        ctx.props().on_navigate.emit(Route::Movies);
                        Ok(false)
                    }
                    api::AppEventKind::WatchedChanged { kind } => {
                        let relevant = matches!(
                            kind,
                            api::WatchedKind::Movie { movie } if *movie == ctx.props().movie_id
                        );
                        if relevant && self.channel.id() != ws::ChannelId::NONE {
                            self.load_movie(ctx);
                            self.load_watched(ctx);
                        }
                        Ok(false)
                    }
                    _ => Ok(false),
                }
            }
            Msg::MovieLoaded(result) => {
                self.movie = Some(
                    result
                        .context(Message::LoadingMovies)?
                        .decode()
                        .context(Message::LoadingMovies)?,
                );
                Ok(true)
            }
            Msg::WatchedLoaded(result) => {
                self.watched = result
                    .context(Message::LoadingWatched)?
                    .decode()
                    .context(Message::LoadingWatched)?
                    .watched;
                Ok(true)
            }
            Msg::MarkWatched => {
                let movie = ctx.props().movie_id;
                self._mark_req = self
                    .channel
                    .request()
                    .body(api::MarkWatchedRequest {
                        kind: api::WatchedKind::Movie { movie },
                        timestamp: None,
                    })
                    .on_packet(ctx.link().callback(Msg::MarkWatchedDone))
                    .send();
                Ok(false)
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
                self.load_movie(ctx);
                self.load_watched(ctx);
                Ok(false)
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
                ctx.props().on_navigate.emit(Route::Movies);
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
                Ok(false)
            }
            Msg::SyncDone(result) => {
                result.context(Message::SyncingSeries)?;
                Ok(false)
            }
            Msg::Back => {
                ctx.props().on_navigate.emit(Route::Movies);
                Ok(false)
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

    fn view_header(&self, ctx: &Context<Self>) -> Html {
        let link = ctx.link();
        html! {
            <div class="row page-title">
                <button class="btn" onclick={link.callback(|_| Msg::Back)}>
                    <span class="icon-inline"><span class="icon arrow-left" /></span>
                    {"Movies"}
                </button>
                if let Some(ref m) = self.movie {
                    <span class="fill">{&m.title}</span>
                    if m.remote_id.is_some() {
                        <button class="btn" onclick={link.callback(|_| Msg::SyncMovie)} title="Sync from remote">
                            <span class="icon-inline"><span class="icon arrow-path" /></span>
                            <span class="hide-mobile">{"Sync"}</span>
                        </button>
                    }
                    if self.confirm_remove {
                        <button class="btn btn-danger" onclick={link.callback(|_| Msg::RemoveMovie)}>
                            {"Confirm remove"}
                        </button>
                        <button class="btn" onclick={link.callback(|_| Msg::CancelRemove)}>
                            {"Cancel"}
                        </button>
                    } else {
                        <button class="btn btn-danger" onclick={link.callback(|_| Msg::ConfirmRemove)} title="Remove movie">
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

    fn view_body(&self, ctx: &Context<Self>) -> Html {
        let Some(ref movie) = self.movie else {
            return html! { <div class="empty text-muted">{"Loading…"}</div> };
        };

        let last_watched_id = self.watched.first().map(|w| w.id);
        let movie_id = ctx.props().movie_id;
        let link = ctx.link();

        html! {
            <div class="detail-layout">
                <div class="detail-sidebar">
                    if let Some(ref poster) = movie.poster {
                        <img class="movie-poster" src={poster.proxy_url()} alt="" />
                    } else {
                        <div class="movie-poster" />
                    }
                </div>
                <div class="detail-content">
                    if let Some(date) = movie.release_date {
                        <div class="group text-muted">{date.to_string()}</div>
                    }

                    if !movie.overview.is_empty() {
                        <p class="overview">{&movie.overview}</p>
                    }

                    <div class="row season-actions">
                        if movie.watched {
                            <span class="icon-inline" title="Watched"><span class="icon check-circle" /></span>
                            <span class="text-muted">
                                if movie.watched_count == 1 {
                                    {"Watched once"}
                                } else {
                                    {format!("Watched {} times", movie.watched_count)}
                                }
                            </span>
                            <button class="btn" onclick={link.callback(|_| Msg::MarkWatched)} title="Watch again">
                                <span class="icon-inline"><span class="icon check" /></span>
                                {"Watch again"}
                            </button>
                            if let Some(wid) = last_watched_id {
                                <button class="btn btn-danger" onclick={link.callback(move |_| Msg::RemoveWatched(wid, api::WatchedKind::Movie { movie: movie_id }))} title="Remove last watch">
                                    <span class="icon-inline"><span class="icon x-mark" /></span>
                                    {"Remove watch"}
                                </button>
                            }
                        } else {
                            <button class="btn btn-success" onclick={link.callback(|_| Msg::MarkWatched)} title="Mark watched">
                                <span class="icon-inline"><span class="icon check" /></span>
                                {"Mark watched"}
                            </button>
                        }
                    </div>
                    if !self.watched.is_empty() {
                        <div class="section">
                            <div class="group text-muted">{"Watch history"}</div>
                            { for self.watched.iter().map(|w| html! {
                                <div class="group text-muted">{w.timestamp.to_string()}</div>
                            }) }
                        </div>
                    }
                </div>
            </div>
        }
    }
}
