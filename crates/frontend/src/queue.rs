use musli_web::web03::prelude::*;
use yew::prelude::*;

use crate::error::{CustomContext, Error, Message};
use crate::router::Route;

pub(super) struct Queue {
    channel: ws::Channel,
    pending: Vec<api::Task>,
    running: Vec<api::Task>,
    completed: Vec<api::CompletedTask>,
    _setup: crate::SetupChannel,
    _broadcast: ws::Listener,
    _list_req: ws::Request,
    _sync_req: ws::Request,
}

pub(super) enum Msg {
    Channel(Result<ws::Channel, ws::Error>),
    AppBroadcast(Result<ws::Packet<api::AppBroadcast>, ws::Error>),
    TasksLoaded(Result<ws::Packet<api::ListTasks>, ws::Error>),
    SyncAll,
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
            pending: Vec::new(),
            running: Vec::new(),
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
                        title="Queue sync for all series">
                        <span class="icon-inline"><span class="icon arrow-path" /></span>
                        <span class="hide-mobile">{"Sync All"}</span>
                    </button>
                </div>
                { self.view_section(ctx, "Running", &self.running, true) }
                { self.view_section(ctx, "Pending", &self.pending, false) }
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
                    self.pending.clear();
                    self.running.clear();
                    self.completed.clear();
                }
                Ok(true)
            }
            Msg::AppBroadcast(packet) => {
                let event = packet?.decode_event()?;
                match event.kind {
                    api::AppEventKind::TaskAdded { task } => {
                        self.pending.push(task);
                        Ok(true)
                    }
                    api::AppEventKind::TaskStarted { task } => {
                        self.pending.retain(|t| t.id != task.id);
                        self.running.push(task);
                        Ok(true)
                    }
                    api::AppEventKind::TaskCompleted { task } => {
                        self.running.retain(|t| t.id != task.id);
                        self.completed.insert(0, task);
                        self.completed.truncate(20);
                        Ok(true)
                    }
                    _ => Ok(false),
                }
            }
            Msg::TasksLoaded(result) => {
                let resp = result
                    .context(Message::LoadingTasks)?
                    .decode()
                    .context(Message::LoadingTasks)?;
                self.pending = resp.pending;
                self.running = resp.running;
                self.completed = resp.completed;
                Ok(true)
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
            .body(api::ListTasksRequest)
            .on_packet(ctx.link().callback(Msg::TasksLoaded))
            .send();
    }

    fn view_section(
        &self,
        ctx: &Context<Self>,
        title: &str,
        tasks: &[api::Task],
        spinning: bool,
    ) -> Html {
        if tasks.is_empty() {
            return html! {};
        }
        html! {
            <div class="section">
                <div class="row"><h3>{title}</h3></div>
                { for tasks.iter().map(|t| self.view_task_row(ctx, t, spinning)) }
            </div>
        }
    }

    fn view_task_row(&self, ctx: &Context<Self>, task: &api::Task, spinning: bool) -> Html {
        let route = match &task.kind {
            api::TaskKind::SyncSeries { series_id, .. } => Some(Route::SeriesDetail(*series_id)),
            api::TaskKind::SyncMovie { movie_id, .. } => Some(Route::MovieDetail(*movie_id)),
        };
        let on_navigate = route.map(|r| ctx.link().callback(move |_| Msg::Navigate(r.clone())));

        html! {
            <div class="group row">
                <span class="icon-inline">
                    <span class={if spinning { "icon arrow-path" } else { "icon clock" }} />
                </span>
                <span class="fill">
                    { self.view_task_label(task) }
                </span>
                if let Some(onclick) = on_navigate {
                    <button class="btn-icon" {onclick} title="Go to detail">
                        <span class="icon chevron-right" />
                    </button>
                }
            </div>
        }
    }

    fn view_task_label(&self, task: &api::Task) -> Html {
        let verb = match &task.kind {
            api::TaskKind::SyncSeries { .. } => "Updating series",
            api::TaskKind::SyncMovie { .. } => "Updating movie",
        };
        html! {
            <>
                <span class="text-muted">{verb}{" — "}</span>
                {task.kind.title()}
            </>
        }
    }

    fn view_completed(&self, ctx: &Context<Self>) -> Html {
        if self.completed.is_empty() {
            return html! {};
        }
        html! {
            <div class="section">
                <div class="row"><h3>{"Completed"}</h3></div>
                { for self.completed.iter().map(|t| self.view_completed_row(ctx, t)) }
            </div>
        }
    }

    fn view_completed_row(&self, ctx: &Context<Self>, task: &api::CompletedTask) -> Html {
        let route = match &task.kind {
            api::TaskKind::SyncSeries { series_id, .. } => Some(Route::SeriesDetail(*series_id)),
            api::TaskKind::SyncMovie { movie_id, .. } => Some(Route::MovieDetail(*movie_id)),
        };
        let on_navigate = route.map(|r| ctx.link().callback(move |_| Msg::Navigate(r.clone())));

        html! {
            <div class="group row">
                <span class="icon-inline"><span class="icon check" /></span>
                <span class="fill">
                    { self.view_completed_label(task) }
                </span>
                if let Some(onclick) = on_navigate {
                    <button class="btn-icon" {onclick} title="Go to detail">
                        <span class="icon chevron-right" />
                    </button>
                }
            </div>
        }
    }

    fn view_completed_label(&self, task: &api::CompletedTask) -> Html {
        let verb = match &task.kind {
            api::TaskKind::SyncSeries { .. } => "Updated series",
            api::TaskKind::SyncMovie { .. } => "Updated movie",
        };
        html! {
            <>
                <span class="text-muted">{verb}{" — "}</span>
                {task.kind.title()}
            </>
        }
    }
}
