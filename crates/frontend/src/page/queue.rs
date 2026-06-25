use api::TimeInfo;
use musli_web::web03::prelude::*;
use yew::prelude::*;

use crate::SetupChannel;
use crate::background::Background;
use crate::error::{CustomContext, Error, Message};
use crate::router::{QueueFocus, QueueQuery, Route, Router, ShowDetailQuery};
use crate::ui::{Button, MDASH, PaginationButtons, Variant};

const PAGE_SIZE: usize = 20;

pub(crate) struct Queue {
    channel: ws::Channel,
    background: Background,
    router: Router,
    pending: Vec<api::Task>,
    running: Vec<api::Task>,
    completed: Vec<api::CompletedTask>,
    time: TimeInfo,
    _time_handle: ContextHandle<TimeInfo>,
    _setup: SetupChannel,
    _broadcast: ws::Listener,
    _list_req: ws::Request,
    _sync_req: ws::Request,
    _remove_req: ws::Request,
    _bump_req: ws::Request,
}
pub(crate) enum Msg {
    Channel(Result<ws::Channel, ws::Error>),
    AppBroadcast(Result<ws::Packet<api::AppBroadcast>, ws::Error>),
    TasksLoaded(Result<ws::Packet<api::ListTasks>, ws::Error>),
    SyncAll,
    SyncAllDone(Result<ws::Packet<api::SyncAll>, ws::Error>),
    Remove(api::TaskId),
    RemoveDone(Result<ws::Packet<api::RemoveTask>, ws::Error>),
    Bump(api::TaskId),
    BumpDone(Result<ws::Packet<api::BumpTask>, ws::Error>),
    Focus(Option<QueueFocus>),
    SetPage(usize),
    Navigate(Route),
    SetTime(TimeInfo),
}

#[derive(Properties, PartialEq)]
pub(crate) struct Props {
    /// Which list is focused, persisted in the route query. `None` is the overview.
    pub(crate) focus: Option<QueueFocus>,
    /// Current page of the focused pending list, persisted in the route query.
    pub(crate) page: usize,
}

impl Component for Queue {
    type Message = Msg;
    type Properties = Props;

    fn create(ctx: &Context<Self>) -> Self {
        let (ws, _) = ctx
            .link()
            .context::<ws::Handle>(Callback::noop())
            .expect("Expected ws::Handle in context");

        let _setup = SetupChannel::new(ws.clone(), ctx.link().callback(Msg::Channel));
        let _broadcast = ws.on_broadcast(ctx.link().callback(Msg::AppBroadcast));

        let (background, _) = ctx
            .link()
            .context::<Background>(Callback::noop())
            .expect("Expected background handle in context");

        let (router, _) = ctx
            .link()
            .context::<Router>(Callback::noop())
            .expect("Expected router in context");

        let (time, _time_handle) = ctx
            .link()
            .context::<TimeInfo>(ctx.link().callback(Msg::SetTime))
            .expect("Expected a configured time zone");

        Self {
            channel: ws::Channel::default(),
            background,
            router,
            pending: Vec::new(),
            running: Vec::new(),
            completed: Vec::new(),
            time,
            _time_handle,
            _setup,
            _broadcast,
            _list_req: ws::Request::default(),
            _sync_req: ws::Request::default(),
            _remove_req: ws::Request::default(),
            _bump_req: ws::Request::default(),
        }
    }

    fn update(&mut self, ctx: &Context<Self>, msg: Self::Message) -> bool {
        match self.try_update(ctx, msg) {
            Ok(render) => render,
            Err(e) => {
                self.background.error(e);
                false
            }
        }
    }

    fn rendered(&mut self, _ctx: &Context<Self>, first_render: bool) {
        if first_render {
            self.background.title(Some("Queue".to_string()));
        }
    }

    fn destroy(&mut self, _ctx: &Context<Self>) {
        self.background.title(None);
    }

    fn view(&self, ctx: &Context<Self>) -> Html {
        let link = ctx.link();

        html! {
            <>
                <div class="row-split">
                    <h1>{"Queue"}</h1>

                    <button class="desktop-has-text" onclick={link.callback(|_| Msg::SyncAll)} title="Queue sync for all show and movies">
                        <span class="icon arrow-path" />
                        <span class="desktop-only">{"Sync All"}</span>
                    </button>
                </div>

                if let Some(focus) = ctx.props().focus {
                    { self.view_focused(ctx, focus) }
                } else {
                    { self.view_overview(ctx) }
                }
            </>
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
                    api::AppEventKind::TaskBumped { task } => {
                        self.pending.retain(|t| t.id != task.id);
                        self.pending.insert(0, task);
                        Ok(true)
                    }
                    api::AppEventKind::TaskRemoved { task_id } => {
                        self.pending.retain(|t| t.id != task_id);
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
                result.context(Message::SyncingAll)?;
                Ok(false)
            }
            Msg::Remove(id) => {
                self._remove_req = self
                    .channel
                    .request()
                    .body(api::RemoveTaskRequest { id })
                    .on_packet(ctx.link().callback(Msg::RemoveDone))
                    .send();
                Ok(false)
            }
            Msg::RemoveDone(result) => {
                result.context(Message::LoadingTasks)?;
                Ok(false)
            }
            Msg::Bump(id) => {
                self._bump_req = self
                    .channel
                    .request()
                    .body(api::BumpTaskRequest { id })
                    .on_packet(ctx.link().callback(Msg::BumpDone))
                    .send();
                Ok(false)
            }
            Msg::BumpDone(result) => {
                result.context(Message::LoadingTasks)?;
                Ok(false)
            }
            Msg::Focus(focus) => {
                self.router
                    .push(Route::Queue(QueueQuery { focus, page: 0 }));
                Ok(false)
            }
            Msg::SetPage(page) => {
                self.router.push(Route::Queue(QueueQuery {
                    focus: ctx.props().focus,
                    page,
                }));
                Ok(false)
            }
            Msg::Navigate(route) => {
                self.router.push(route);
                Ok(false)
            }
            Msg::SetTime(time) => {
                self.time = time;
                Ok(!self.pending.is_empty() || !self.completed.is_empty())
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

    /// The overview: one clickable card per task list showing its count and the
    /// current (first) task, without rendering the full, churning lists.
    fn view_overview(&self, ctx: &Context<Self>) -> Html {
        let running_current = self.running.first().map(|t| self.view_task_label(t, None));

        let pending_current = self.pending.first().map(|t| self.view_task_label(t, None));

        let completed_current = self
            .completed
            .first()
            .map(|t| self.view_completed_label(t, None));

        html! {
            <div class="column">
                { self.view_overview_card(ctx, QueueFocus::Running, "arrow-path", self.running.len(), running_current) }
                { self.view_overview_card(ctx, QueueFocus::Pending, "clock", self.pending.len(), pending_current) }
                { self.view_overview_card(ctx, QueueFocus::Completed, "check", self.completed.len(), completed_current) }
            </div>
        }
    }

    fn view_overview_card(
        &self,
        ctx: &Context<Self>,
        focus: QueueFocus,
        icon: &'static str,
        count: usize,
        current: Option<Html>,
    ) -> Html {
        let onclick = (count > 0).then(|| ctx.link().callback(move |_| Msg::Focus(Some(focus))));
        let clickable = onclick.is_some().then_some("clickable");

        html! {
            <div class={classes!("row", clickable)} onclick={onclick}>
                <span class="item-inline"><span class={classes!("icon", icon)} /></span>

                <span class="row fill">
                    <strong>{focus.title()}</strong>

                    <span>{MDASH}</span>

                    if let Some(current) = current {
                        { current }
                    } else {
                        <span class="text-muted">{"None"}</span>
                    }
                </span>

                <span class="status">{count}</span>
            </div>
        }
    }

    /// The focused stage: a back button and a single task list.
    fn view_focused(&self, ctx: &Context<Self>, focus: QueueFocus) -> Html {
        let link = ctx.link();

        let (buttons, body) = match focus {
            QueueFocus::Running => (None, self.view_running_list(ctx)),
            QueueFocus::Pending => self.view_pending_list(ctx),
            QueueFocus::Completed => self.view_completed_list(ctx),
        };

        html! {
            <div class="column">
                <h3>{focus.title()}</h3>

                <div class="row">
                    <div class="input-group">
                        <button class="desktop-has-text" onclick={link.callback(|_| Msg::Focus(None))} title="Back to overview">
                            <span class="icon arrow-uturn-left" />
                            <span class="desktop-only">{"Back"}</span>
                        </button>

                        {buttons}
                    </div>
                </div>

                { body }
            </div>
        }
    }

    fn view_running_list(&self, ctx: &Context<Self>) -> Html {
        if self.running.is_empty() {
            return html! { <h4 class="text-muted">{"No running tasks"}</h4> };
        }

        html! {
            <div class="column">
                { for self.running.iter().map(|t| self.view_task_row(ctx, t, true)) }
            </div>
        }
    }

    fn view_pending_list(&self, ctx: &Context<Self>) -> (Option<Html>, Html) {
        let link = ctx.link();

        let total_pending = self.pending.len();

        if total_pending == 0 {
            return (
                None,
                html! { <h4 class="text-muted">{"No pending tasks"}</h4> },
            );
        }

        let total_pages = total_pending.div_ceil(PAGE_SIZE).max(1);
        let page = ctx.props().page.min(total_pages - 1);

        let page_pending = self.pending.iter().skip(page * PAGE_SIZE).take(PAGE_SIZE);

        let buttons = (total_pages > 1).then(|| {
            html! {
                <PaginationButtons {page} {total_pages} on_page={link.callback(Msg::SetPage)} />
            }
        });

        let body = html! {
                <div class="column">
                    { for page_pending.map(|t| self.view_task_row(ctx, t, false)) }
                </div>
        };

        (buttons, body)
    }

    fn view_task_row(&self, ctx: &Context<Self>, task: &api::Task, spinning: bool) -> Html {
        let route = match &task.kind {
            api::TaskKind::SyncShow { show_id, .. } => {
                Some(Route::ShowDetail(*show_id, ShowDetailQuery::default()))
            }
            api::TaskKind::SyncMovie { movie_id, .. } => Some(Route::MovieDetail(*movie_id)),
            api::TaskKind::RefreshTopLanguages => None,
        };

        let on_navigate = route.map(|r| ctx.link().callback(move |_| Msg::Navigate(r.clone())));

        let id = task.id;

        html! {
            <div class="row">
                <span class="item-inline">
                    <span class={if spinning { "icon arrow-path" } else { "icon clock" }} />
                </span>

                <span class="row fill">
                    { self.view_task_label(task, on_navigate) }
                </span>

                if !spinning {
                    <span class="text-muted">{ eta_label(task.run_at, self.time.now()) }</span>

                    <Button icon="forward" title="Run now" onclick={ctx.link().callback(move |_| Msg::Bump(id))} />

                    <Button icon="trash" variant={Variant::Danger} title="Remove from queue" onclick={ctx.link().callback(move |_| Msg::Remove(id))} />
                }
            </div>
        }
    }

    fn view_task_label(&self, task: &api::Task, on_navigate: Option<Callback<MouseEvent>>) -> Html {
        let verb = match &task.kind {
            api::TaskKind::SyncShow { .. } => "Updating show",
            api::TaskKind::SyncMovie { .. } => "Updating movie",
            api::TaskKind::RefreshTopLanguages => "Refreshing top languages",
        };

        // Tasks without an associated show/movie show just the verb.
        let has_target = !matches!(task.kind, api::TaskKind::RefreshTopLanguages);

        html! {
            <>
                <span class="text-muted">{verb}</span>

                if has_target {
                    <span>{MDASH}</span>

                    <span class={classes!(on_navigate.is_some().then_some("clickable"))} onclick={on_navigate}>
                        if let Some(ref title) = task.kind.title() {
                            {title}
                        } else {
                            <span class="text-muted">{"Untitled"}</span>
                        }
                    </span>
                }
            </>
        }
    }

    fn view_completed_list(&self, ctx: &Context<Self>) -> (Option<Html>, Html) {
        let link = ctx.link();

        let total = self.completed.len();

        if total == 0 {
            return (
                None,
                html! { <h4 class="text-muted">{"No completed tasks"}</h4> },
            );
        }

        let total_pages = total.div_ceil(PAGE_SIZE).max(1);
        let page = ctx.props().page.min(total_pages - 1);

        let page_completed = self.completed.iter().skip(page * PAGE_SIZE).take(PAGE_SIZE);

        let buttons = (total_pages > 1).then(|| {
            html! {
                <PaginationButtons {page} {total_pages} on_page={link.callback(Msg::SetPage)} />
            }
        });

        let body = html! {
            <div class="column">
                { for page_completed.map(|t| self.view_completed_row(ctx, t)) }
            </div>
        };

        (buttons, body)
    }

    fn view_completed_row(&self, ctx: &Context<Self>, task: &api::CompletedTask) -> Html {
        let route = match &task.kind {
            api::TaskKind::SyncShow { show_id, .. } => {
                Some(Route::ShowDetail(*show_id, ShowDetailQuery::default()))
            }
            api::TaskKind::SyncMovie { movie_id, .. } => Some(Route::MovieDetail(*movie_id)),
            api::TaskKind::RefreshTopLanguages => None,
        };

        let on_navigate = route.map(|r| ctx.link().callback(move |_| Msg::Navigate(r.clone())));

        html! {
            <div class="row">
                <span class="item-inline"><span class="icon check" /></span>

                <span class="row fill">
                    { self.view_completed_label(task, on_navigate) }
                </span>

                <span class="text-muted">{ ago_label(task.completed_at, self.time.now()) }</span>
            </div>
        }
    }

    fn view_completed_label(
        &self,
        task: &api::CompletedTask,
        on_navigate: Option<Callback<MouseEvent>>,
    ) -> Html {
        let verb = match &task.kind {
            api::TaskKind::SyncShow { .. } => "Updated show",
            api::TaskKind::SyncMovie { .. } => "Updated movie",
            api::TaskKind::RefreshTopLanguages => "Refreshed top languages",
        };

        // Tasks without an associated show/movie show just the verb.
        let has_target = !matches!(task.kind, api::TaskKind::RefreshTopLanguages);

        html! {
            <>
                <span class="text-muted">{verb}</span>

                if has_target {
                    <span>{MDASH}</span>

                    <span class="clickable" onclick={on_navigate}>
                        if let Some(ref title) = task.kind.title() {
                            {title}
                        } else {
                            <span class="text-muted">{"Untitled"}</span>
                        }
                    </span>
                }
            </>
        }
    }
}

/// Split a whole-second duration into the largest sensible unit and its count,
/// e.g. 90 -> (1, "minute").
fn humanize_count(secs: u64) -> (u64, &'static str) {
    if secs < 60 {
        (secs, "second")
    } else if secs < 3600 {
        (secs / 60, "minute")
    } else if secs < 86400 {
        (secs / 3600, "hour")
    } else {
        (secs / 86400, "day")
    }
}

/// Format when a pending task is expected to run as a human label, e.g.
/// "in 20 seconds", "in 5 minutes", "in 1 hour", or "momentarily" when it is
/// already due.
fn eta_label(run_at: Option<api::Timestamp>, now: api::Timestamp) -> String {
    let Some(run_at) = run_at else {
        return "momentarily".to_string();
    };

    let Some(remaining) = run_at.checked_duration_since(now) else {
        return "momentarily".to_string();
    };

    let (n, unit) = humanize_count(remaining.as_secs());

    if n == 0 {
        "momentarily".to_string()
    } else if n == 1 {
        format!("in 1 {unit}")
    } else {
        format!("in {n} {unit}s")
    }
}

/// Format how long ago a task completed as a human label, e.g. "5 seconds ago",
/// "2 minutes ago", or "just now".
fn ago_label(completed_at: api::Timestamp, now: api::Timestamp) -> String {
    let Some(elapsed) = now.checked_duration_since(completed_at) else {
        return "just now".to_string();
    };

    if elapsed.as_secs() < 10 {
        return "just now".to_string();
    }

    let (n, unit) = humanize_count(elapsed.as_secs());

    if n == 1 {
        format!("1 {unit} ago")
    } else {
        format!("{n} {unit}s ago")
    }
}
