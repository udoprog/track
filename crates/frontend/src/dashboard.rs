use musli_web::web03::prelude::*;
use yew::prelude::*;

use crate::error::{CustomContext, Error, Message};
use crate::router::{Route, SeriesQuery};

pub(super) struct Dashboard {
    channel: ws::Channel,
    pending: Vec<api::Pending>,
    _setup: crate::SetupChannel,
    _broadcast: ws::Listener,
    _pending_req: ws::Request,
    _mark_req: ws::Request,
}

pub(super) enum Msg {
    Channel(Result<ws::Channel, ws::Error>),
    AppBroadcast(Result<ws::Packet<api::AppBroadcast>, ws::Error>),
    PendingLoaded(Result<ws::Packet<api::ListPending>, ws::Error>),
    MarkWatched(api::WatchedKind),
    MarkWatchedDone(Result<ws::Packet<api::MarkWatched>, ws::Error>),
    Navigate(Route),
}

#[derive(Properties, PartialEq)]
pub(super) struct Props {
    pub(super) onerror: Callback<Error>,
    pub(super) on_navigate: Callback<Route>,
}

impl Component for Dashboard {
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
            pending: Vec::new(),
            _setup,
            _broadcast,
            _pending_req: ws::Request::default(),
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
            <div class="page">
                { self.view_pending(ctx) }
                <div class="section">
                    <div class="row"><h2>{"Coming Up"}</h2></div>
                    <crate::Calendar
                        on_navigate={ctx.props().on_navigate.clone()}
                        onerror={ctx.props().onerror.clone()}
                    />
                </div>
            </div>
        }
    }
}

impl Dashboard {
    fn try_update(&mut self, ctx: &Context<Self>, msg: Msg) -> Result<bool, Error> {
        match msg {
            Msg::Channel(result) => {
                self.channel = result?;
                if self.channel.id() != ws::ChannelId::NONE {
                    self.load_pending(ctx);
                } else {
                    self.pending.clear();
                }
                Ok(true)
            }
            Msg::AppBroadcast(packet) => {
                let event = packet?.decode_event()?;
                if event.channel == self.channel.id() {
                    return Ok(false);
                }
                match event.kind {
                    api::AppEventKind::PendingChanged
                    | api::AppEventKind::WatchedChanged { .. }
                    | api::AppEventKind::SeriesCreated { .. }
                    | api::AppEventKind::SeriesDeleted { .. }
                    | api::AppEventKind::MovieCreated { .. }
                    | api::AppEventKind::MovieDeleted { .. }
                    | api::AppEventKind::TaskCompleted { .. } => {
                        if self.channel.id() != ws::ChannelId::NONE {
                            self.load_pending(ctx);
                        }
                        Ok(false)
                    }
                    _ => Ok(false),
                }
            }
            Msg::PendingLoaded(result) => {
                self.pending = result
                    .context(Message::LoadingPending)?
                    .decode()
                    .context(Message::LoadingPending)?
                    .pending;
                Ok(true)
            }
            Msg::MarkWatched(kind) => {
                self._mark_req = self
                    .channel
                    .request()
                    .body(api::MarkWatchedRequest {
                        kind,
                        timestamp: None,
                    })
                    .on_packet(ctx.link().callback(Msg::MarkWatchedDone))
                    .send();
                Ok(false)
            }
            Msg::MarkWatchedDone(result) => {
                result.context(Message::MarkingWatched)?;
                if self.channel.id() != ws::ChannelId::NONE {
                    self.load_pending(ctx);
                }
                Ok(false)
            }
            Msg::Navigate(route) => {
                ctx.props().on_navigate.emit(route);
                Ok(false)
            }
        }
    }

    fn load_pending(&mut self, ctx: &Context<Self>) {
        self._pending_req = self
            .channel
            .request()
            .body(api::ListPendingRequest)
            .on_packet(ctx.link().callback(Msg::PendingLoaded))
            .send();
    }

    fn view_pending(&self, ctx: &Context<Self>) -> Html {
        html! {
            <div class="section">
                <div class="row"><h2>{"Up Next"}</h2></div>
                if self.pending.is_empty() {
                    <p class="text-muted">{"Nothing pending."}</p>
                } else {
                    <div class="pending-grid">
                        { for self.pending.iter().map(|p| self.view_pending_item(ctx, p)) }
                    </div>
                }
            </div>
        }
    }

    fn view_pending_item(&self, ctx: &Context<Self>, p: &api::Pending) -> Html {
        let kind = p.kind.clone();
        let route = match p.kind {
            api::PendingKind::Episode { series, .. } => {
                Route::SeriesDetail(series, SeriesQuery::default())
            }
            api::PendingKind::Movie { movie } => Route::MovieDetail(movie),
        };
        let route_poster = route.clone();
        let on_navigate = ctx.link().callback(move |_| Msg::Navigate(route.clone()));
        let on_navigate_poster = ctx
            .link()
            .callback(move |_| Msg::Navigate(route_poster.clone()));
        let on_mark = ctx.link().callback(move |_| {
            Msg::MarkWatched(match kind {
                api::PendingKind::Episode { series, episode } => {
                    api::WatchedKind::Episode { series, episode }
                }
                api::PendingKind::Movie { movie } => api::WatchedKind::Movie { movie },
            })
        });

        html! {
            <div class="pending-item">
                if let Some(ref poster) = p.poster {
                    <img class="pending-poster clickable" src={poster.proxy_url()} alt=""
                        onclick={on_navigate_poster} />
                } else {
                    <div class="pending-poster pending-poster-placeholder" />
                }
                <div class="pending-info">
                    if let Some(ref title) = p.series_title {
                        <span class="pending-series clickable" onclick={on_navigate}>{title}</span>
                    }
                    <span class="pending-label">{&p.label}</span>
                    if let Some(date) = p.aired {
                        <span class="pending-date">{date.to_string()}</span>
                    }
                    <button class="btn-icon-success" onclick={on_mark} title="Mark watched">
                        <span class="icon check" />
                    </button>
                </div>
            </div>
        }
    }
}
