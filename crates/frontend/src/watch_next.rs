use musli_web::web03::prelude::*;
use yew::prelude::*;

use api::HasAired;

use crate::error::{CustomContext, Error, Message};
use crate::router::{Route, SeriesDetailQuery};

pub(super) struct WatchNext {
    channel: ws::Channel,
    pending: Vec<api::Pending>,
    _setup: crate::SetupChannel,
    _broadcast: ws::Listener,
    _list_req: ws::Request,
    _mark_req: ws::Request,
}

pub(super) enum Msg {
    Channel(Result<ws::Channel, ws::Error>),
    AppBroadcast(Result<ws::Packet<api::AppBroadcast>, ws::Error>),
    Loaded(Result<ws::Packet<api::ListWatchNext>, ws::Error>),
    MarkWatched(api::WatchedKind),
    MarkWatchedDone(Result<ws::Packet<api::MarkWatched>, ws::Error>),
    Navigate(Route),
}

#[derive(Properties, PartialEq)]
pub(super) struct Props {
    pub(super) onerror: Callback<Error>,
    pub(super) on_navigate: Callback<Route>,
}

impl Component for WatchNext {
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
            <div class="page">
                <div class="page-title">{"Watch Next"}</div>

                if self.pending.is_empty() {
                    <div class="empty text-muted">{"Nothing to watch next."}</div>
                } else {
                    <div class="table">
                        { for self.pending.iter().map(|p| self.view_row(ctx, p)) }
                    </div>
                }
            </div>
        }
    }
}

impl WatchNext {
    fn try_update(&mut self, ctx: &Context<Self>, msg: Msg) -> Result<bool, Error> {
        match msg {
            Msg::Channel(result) => {
                self.channel = result?;
                if self.channel.id() != ws::ChannelId::NONE {
                    self.load(ctx);
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
                        self.load(ctx);
                        Ok(false)
                    }
                    _ => Ok(false),
                }
            }
            Msg::Loaded(result) => {
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
                    self.load(ctx);
                }
                Ok(false)
            }
            Msg::Navigate(route) => {
                ctx.props().on_navigate.emit(route);
                Ok(false)
            }
        }
    }

    fn load(&mut self, ctx: &Context<Self>) {
        self._list_req = self
            .channel
            .request()
            .body(api::ListWatchNextRequest)
            .on_packet(ctx.link().callback(Msg::Loaded))
            .send();
    }

    fn view_row(&self, ctx: &Context<Self>, p: &api::Pending) -> Html {
        let tz = ctx
            .link()
            .context::<crate::SystemTz>(Callback::noop())
            .map(|(t, _)| t.get().clone())
            .unwrap_or(jiff::tz::TimeZone::UTC);
        let kind = p.kind.clone();

        let route = match p.kind {
            api::PendingKind::Episode { series, .. } => {
                Route::SeriesDetail(series, SeriesDetailQuery::default())
            }
            api::PendingKind::Movie { movie } => Route::MovieDetail(movie),
        };

        let on_navigate = ctx.link().callback(move |_| Msg::Navigate(route.clone()));

        let on_mark = ctx.link().callback(move |_| {
            Msg::MarkWatched(match kind {
                api::PendingKind::Episode { series, episode } => {
                    api::WatchedKind::Episode { series, episode }
                }
                api::PendingKind::Movie { movie } => api::WatchedKind::Movie { movie },
            })
        });

        html! {
            <div class="table-entry">
                <div class="row">
                    if let Some(ref poster) = p.poster {
                        <img class="poster-sm" src={poster.proxy_url()} />
                    } else {
                        <div class="poster-sm" />
                    }

                    <div class="fill">
                        <div class="row clickable" onclick={on_navigate}>
                            <span class="fill">
                                if let Some(ref title) = p.series_title {
                                    <span class="text-muted">{title}{" — "}</span>
                                }

                                {&p.label}
                            </span>

                            if let Some(s) = p.display_at(&tz) {
                                <span class="text-muted">{s}</span>
                            }
                        </div>
                    </div>

                    <button class="btn-icon-success" onclick={on_mark} title="Mark watched">
                        <span class="icon check" />
                    </button>
                </div>
            </div>
        }
    }
}
