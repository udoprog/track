use std::collections::HashSet;

use musli_web::web03::prelude::*;
use yew::prelude::*;

use crate::error::{CustomContext, Error, Message};
use crate::router::Route;

struct Completed {
    series_id: api::SeriesId,
    title: String,
}

pub(super) struct Queue {
    channel: ws::Channel,
    series: Vec<api::Series>,
    syncing: HashSet<api::SeriesId>,
    completed: Vec<Completed>,
    _setup: crate::SetupChannel,
    _broadcast: ws::Listener,
    _list_req: ws::Request,
    _sync_req: ws::Request,
}

pub(super) enum Msg {
    Channel(Result<ws::Channel, ws::Error>),
    AppBroadcast(Result<ws::Packet<api::AppBroadcast>, ws::Error>),
    SeriesLoaded(Result<ws::Packet<api::ListSeries>, ws::Error>),
    SyncSeries(api::SeriesId),
    SyncAll,
    SyncDone(Result<ws::Packet<api::SyncSeries>, ws::Error>),
    SyncAllDone(Result<ws::Packet<api::SyncAll>, ws::Error>),
    Navigate(Route),
}

#[derive(Properties, PartialEq)]
pub(super) struct Props {
    pub(super) onerror: Callback<Error>,
    pub(super) on_navigate: Callback<Route>,
}

impl Component for Queue {
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
            series: Vec::new(),
            syncing: HashSet::new(),
            completed: Vec::new(),
            _setup,
            _broadcast,
            _list_req: ws::Request::default(),
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
            <div class="outline">
                <div class="outline-title row">
                    <span class="fill">{"Sync Queue"}</span>
                    <button class="btn" onclick={ctx.link().callback(|_| Msg::SyncAll)}
                        title="Sync all series">
                        <span class="icon-inline"><span class="icon arrow-path" /></span>
                        <span class="hide-mobile">{"Sync All"}</span>
                    </button>
                </div>
                { self.view_running(ctx) }
                { self.view_series(ctx) }
                { self.view_completed(ctx) }
            </div>
        }
    }
}

impl Queue {
    fn try_update(&mut self, ctx: &Context<Self>, msg: Msg) -> Result<bool, Error> {
        match msg {
            Msg::Channel(result) => {
                self.channel = result?;
                if self.channel.id() != ws::ChannelId::NONE {
                    self.load(ctx);
                } else {
                    self.series.clear();
                    self.syncing.clear();
                }
                Ok(true)
            }
            Msg::AppBroadcast(packet) => {
                let event = packet?.decode_event()?;
                match event.kind {
                    api::AppEventKind::SyncStarted { series_id: Some(id) } => {
                        self.syncing.insert(id);
                        Ok(true)
                    }
                    api::AppEventKind::SyncFinished { series_id: Some(id) } => {
                        self.syncing.remove(&id);
                        let title = self
                            .series
                            .iter()
                            .find(|s| s.id == id)
                            .map(|s| s.title.clone())
                            .unwrap_or_else(|| format!("{id}"));
                        self.completed.insert(0, Completed { series_id: id, title });
                        self.completed.truncate(20);
                        Ok(true)
                    }
                    api::AppEventKind::SeriesCreated { .. }
                    | api::AppEventKind::SeriesChanged { .. }
                    | api::AppEventKind::SeriesDeleted { .. } => {
                        if self.channel.id() != ws::ChannelId::NONE {
                            self.load(ctx);
                        }
                        Ok(false)
                    }
                    _ => Ok(false),
                }
            }
            Msg::SeriesLoaded(result) => {
                self.series = result
                    .context(Message::LoadingSeries)?
                    .decode()
                    .context(Message::LoadingSeries)?
                    .series;
                Ok(true)
            }
            Msg::SyncSeries(id) => {
                self._sync_req = self
                    .channel
                    .request()
                    .body(api::SyncSeriesRequest { id })
                    .on_packet(ctx.link().callback(Msg::SyncDone))
                    .send();
                Ok(false)
            }
            Msg::SyncAll => {
                self._sync_req = self
                    .channel
                    .request()
                    .body(api::SyncAllRequest)
                    .on_packet(ctx.link().callback(Msg::SyncAllDone))
                    .send();
                Ok(false)
            }
            Msg::SyncDone(result) => {
                result.context(Message::SyncingSeries)?;
                Ok(false)
            }
            Msg::SyncAllDone(result) => {
                result.context(Message::SyncingSeries)?;
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
            .body(api::ListSeriesRequest)
            .on_packet(ctx.link().callback(Msg::SeriesLoaded))
            .send();
    }

    fn view_running(&self, ctx: &Context<Self>) -> Html {
        if self.syncing.is_empty() {
            return html! {};
        }
        html! {
            <div class="section">
                <div class="row"><h3>{"Running"}</h3></div>
                { for self.syncing.iter().map(|&id| {
                    let title = self.series.iter()
                        .find(|s| s.id == id)
                        .map(|s| s.title.as_str())
                        .unwrap_or("…");
                    let on_navigate = ctx.link().callback(move |_| Msg::Navigate(Route::SeriesDetail(id)));
                    html! {
                        <div class="group row">
                            <span class="icon-inline"><span class="icon arrow-path" /></span>
                            <span class="fill clickable" onclick={on_navigate}>{title}</span>
                        </div>
                    }
                }) }
            </div>
        }
    }

    fn view_series(&self, ctx: &Context<Self>) -> Html {
        if self.series.is_empty() {
            return html! {
                <div class="empty text-muted">{"No series tracked."}</div>
            };
        }
        html! {
            <div class="section">
                <div class="row"><h3>{"Series"}</h3></div>
                { for self.series.iter().map(|s| self.view_series_row(ctx, s)) }
            </div>
        }
    }

    fn view_series_row(&self, ctx: &Context<Self>, s: &api::Series) -> Html {
        let id = s.id;
        let is_syncing = self.syncing.contains(&id);
        let on_navigate = ctx.link().callback(move |_| Msg::Navigate(Route::SeriesDetail(id)));
        let on_sync = ctx.link().callback(move |_| Msg::SyncSeries(id));

        html! {
            <div class="group row">
                if let Some(ref poster) = s.poster {
                    <img class="poster-sm" src={poster.proxy_url()} alt="" />
                } else {
                    <div class="poster-sm" />
                }
                <span class="fill clickable" onclick={on_navigate}>{&s.title}</span>
                if is_syncing {
                    <span class="icon-inline"><span class="icon arrow-path" /></span>
                } else {
                    <button class="btn-icon" onclick={on_sync} title="Sync series">
                        <span class="icon arrow-path" />
                    </button>
                }
            </div>
        }
    }

    fn view_completed(&self, ctx: &Context<Self>) -> Html {
        if self.completed.is_empty() {
            return html! {};
        }
        html! {
            <div class="section">
                <div class="row"><h3>{"Completed"}</h3></div>
                { for self.completed.iter().map(|c| {
                    let id = c.series_id;
                    let on_navigate = ctx.link().callback(move |_| Msg::Navigate(Route::SeriesDetail(id)));
                    html! {
                        <div class="group row">
                            <span class="icon-inline"><span class="icon check" /></span>
                            <span class="fill clickable" onclick={on_navigate}>{&c.title}</span>
                        </div>
                    }
                }) }
            </div>
        }
    }
}
