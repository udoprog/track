use musli_web::web03::prelude::*;
use yew::prelude::*;

use crate::error::{CustomContext, Error, Message};

pub(super) struct MoviesList {
    channel: ws::Channel,
    movies: Vec<api::Movie>,
    _setup: crate::SetupChannel,
    _broadcast: ws::Listener,
    _list_req: ws::Request,
    _mark_req: ws::Request,
}

pub(super) enum Msg {
    Channel(Result<ws::Channel, ws::Error>),
    AppBroadcast(Result<ws::Packet<api::AppBroadcast>, ws::Error>),
    MoviesLoaded(Result<ws::Packet<api::ListMovies>, ws::Error>),
    MarkWatched(api::MovieId),
    MarkWatchedDone(Result<ws::Packet<api::MarkWatched>, ws::Error>),
}

#[derive(Properties, PartialEq)]
pub(super) struct Props {
    pub(super) onerror: Callback<Error>,
}

impl Component for MoviesList {
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
            movies: Vec::new(),
            _setup,
            _broadcast,
            _list_req: ws::Request::default(),
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
                <div class="outline-title">{"Movies"}</div>
                if self.movies.is_empty() {
                    <div class="empty text-muted">{"No movies tracked."}</div>
                } else {
                    { for self.movies.iter().map(|m| self.view_row(ctx, m)) }
                }
            </div>
        }
    }
}

impl MoviesList {
    fn try_update(&mut self, ctx: &Context<Self>, msg: Msg) -> Result<bool, Error> {
        match msg {
            Msg::Channel(result) => {
                self.channel = result?;
                if self.channel.id() != ws::ChannelId::NONE {
                    self.load(ctx);
                } else {
                    self.movies.clear();
                }
                Ok(true)
            }
            Msg::AppBroadcast(packet) => {
                let event = packet?.decode_event()?;
                if event.channel == self.channel.id() {
                    return Ok(false);
                }
                match event.kind {
                    api::AppEventKind::MovieCreated { .. }
                    | api::AppEventKind::MovieChanged { .. }
                    | api::AppEventKind::MovieDeleted { .. }
                    | api::AppEventKind::WatchedChanged { .. } => {
                        if self.channel.id() != ws::ChannelId::NONE {
                            self.load(ctx);
                        }
                        Ok(false)
                    }
                    _ => Ok(false),
                }
            }
            Msg::MoviesLoaded(result) => {
                self.movies = result
                    .context(Message::LoadingMovies)?
                    .decode()
                    .context(Message::LoadingMovies)?
                    .movies;
                Ok(true)
            }
            Msg::MarkWatched(movie_id) => {
                self._mark_req = self
                    .channel
                    .request()
                    .body(api::MarkWatchedRequest {
                        kind: api::WatchedKind::Movie { movie: movie_id },
                        timestamp: None,
                    })
                    .on_packet(ctx.link().callback(Msg::MarkWatchedDone))
                    .send();
                Ok(false)
            }
            Msg::MarkWatchedDone(result) => {
                result.context(Message::MarkingWatched)?;
                Ok(false)
            }
        }
    }

    fn load(&mut self, ctx: &Context<Self>) {
        self._list_req = self
            .channel
            .request()
            .body(api::ListMoviesRequest)
            .on_packet(ctx.link().callback(Msg::MoviesLoaded))
            .send();
    }

    fn view_row(&self, ctx: &Context<Self>, m: &api::Movie) -> Html {
        let movie_id = m.id;

        html! {
            <div class="group row">
                if let Some(ref poster) = m.poster {
                    <img class="poster-sm" src={poster.proxy_url()} alt="" />
                } else {
                    <div class="poster-sm" />
                }
                <span class="fill">{&m.title}</span>
                if let Some(date) = m.release_date {
                    <span class="text-muted">{date.year().to_string()}</span>
                }
                if m.watched {
                    <span class="icon-inline" title="Watched"><span class="icon check-circle" /></span>
                } else {
                    <button
                        class="btn-icon-success"
                        title="Mark watched"
                        onclick={ctx.link().callback(move |_| Msg::MarkWatched(movie_id))}
                    >
                        <span class="icon check" />
                    </button>
                }
            </div>
        }
    }
}
