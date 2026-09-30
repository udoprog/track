use api::TimeInfo;
use musli_web::web03::prelude::*;
use yew::prelude::*;

use crate::SetupChannel;
use crate::background::Background;
use crate::error::{CustomContext, Error, Message};
use crate::router::{QueueFocus, QueueQuery, Route, Router, ShowDetailQuery};
use crate::ui::{Button, PaginationButtons, Skeleton, Variant};

const PAGE_SIZE: usize = 20;
/// How many pending and completed tasks the overview shows of each.
const OVERVIEW_SIZE: usize = 10;

pub(crate) struct Queue {
    channel: ws::Channel,
    background: Background,
    router: Router,
    pending: Vec<api::Task>,
    running: Vec<api::Task>,
    completed: Vec<api::CompletedTask>,
    /// Whether the initial task lists have loaded; gates skeleton placeholders.
    loaded: bool,
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
            loaded: false,
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

                    <Button icon="arrow-path" label="Sync all" title="Queue sync for all show and movies" onclick={link.callback(|_| Msg::SyncAll)} />
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
                    self.loaded = false;
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
                self.loaded = true;
                Ok(true)
            }
            Msg::SyncAll => {
                if self.channel.id() != ws::ChannelId::NONE {
                    self._sync_req = self
                        .channel
                        .request()
                        .body(api::SyncAllRequest)
                        .on_packet(ctx.link().callback(Msg::SyncAllDone))
                        .send();
                }

                Ok(false)
            }
            Msg::SyncAllDone(result) => {
                result.context(Message::SyncingAll)?;
                Ok(false)
            }
            Msg::Remove(id) => {
                if self.channel.id() != ws::ChannelId::NONE {
                    self._remove_req = self
                        .channel
                        .request()
                        .body(api::RemoveTaskRequest { id })
                        .on_packet(ctx.link().callback(Msg::RemoveDone))
                        .send();
                }

                Ok(false)
            }
            Msg::RemoveDone(result) => {
                result.context(Message::LoadingTasks)?;
                Ok(false)
            }
            Msg::Bump(id) => {
                if self.channel.id() != ws::ChannelId::NONE {
                    self._bump_req = self
                        .channel
                        .request()
                        .body(api::BumpTaskRequest { id })
                        .on_packet(ctx.link().callback(Msg::BumpDone))
                        .send();
                }

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
        if self.channel.id() == ws::ChannelId::NONE {
            return;
        }

        self._list_req = self
            .channel
            .request()
            .body(api::ListTasksRequest)
            .on_packet(ctx.link().callback(Msg::TasksLoaded))
            .send();
    }

    /// A short run of placeholder rows shown while the task lists load, so the
    /// page reads as loading rather than empty.
    fn view_task_skeletons() -> Html {
        html! {
            <div class="column">
                { for (0..3).map(|_| html! { <Skeleton /> }) }
            </div>
        }
    }

    /// The overview: every running task, and the first few pending and
    /// completed ones, each list offering the rest when there are more.
    fn view_overview(&self, ctx: &Context<Self>) -> Html {
        if !self.loaded {
            return Self::view_task_skeletons();
        }

        html! {
            <div class="column">
                { self.view_section(ctx, QueueFocus::Running, self.running.len(), self.running.len(), html! {
                    { for self.running.iter().map(|t| self.view_task_row(ctx, t, true)) }
                }) }

                { self.view_section(ctx, QueueFocus::Pending, self.pending.len(), OVERVIEW_SIZE, html! {
                    { for self.pending.iter().take(OVERVIEW_SIZE).map(|t| self.view_task_row(ctx, t, false)) }
                }) }

                { self.view_section(ctx, QueueFocus::Completed, self.completed.len(), OVERVIEW_SIZE, html! {
                    { for self.completed.iter().take(OVERVIEW_SIZE).map(|t| self.view_completed_row(ctx, t)) }
                }) }
            </div>
        }
    }

    /// One task list in the overview: a heading with its count, the rows shown,
    /// and a way to the whole list when `shown` is fewer than `count`.
    fn view_section(
        &self,
        ctx: &Context<Self>,
        focus: QueueFocus,
        count: usize,
        shown: usize,
        rows: Html,
    ) -> Html {
        html! {
            <section class="task-section">
                <div class="row-split">
                    <h3 class="row text-gap">
                        <span>{focus.title()}</span>
                        <span class="status">{count}</span>
                    </h3>

                    if count > shown {
                        <Button icon="chevron-right" label="Show all" title={format!("Show all {} tasks", focus.title().to_lowercase())} onclick={ctx.link().callback(move |_| Msg::Focus(Some(focus)))} />
                    }
                </div>

                if count == 0 {
                    <p class="text-muted">{"None"}</p>
                } else {
                    <div class="task-grid">{rows}</div>
                }
            </section>
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
                <div class="row-split">
                    <h3>{focus.title()}</h3>

                    <div class="row">
                        {buttons}

                        <Button icon="arrow-uturn-left" label="Back" title="Back to overview" onclick={link.callback(|_| Msg::Focus(None))} />
                    </div>
                </div>

                { body }
            </div>
        }
    }

    fn view_running_list(&self, ctx: &Context<Self>) -> Html {
        if !self.loaded {
            return Self::view_task_skeletons();
        }

        if self.running.is_empty() {
            return html! { <h4 class="text-muted">{"No running tasks"}</h4> };
        }

        html! {
            <div class="task-grid">
                { for self.running.iter().map(|t| self.view_task_row(ctx, t, true)) }
            </div>
        }
    }

    fn view_pending_list(&self, ctx: &Context<Self>) -> (Option<Html>, Html) {
        let link = ctx.link();

        if !self.loaded {
            return (None, Self::view_task_skeletons());
        }

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
            <div class="task-grid">
                { for page_pending.map(|t| self.view_task_row(ctx, t, false)) }
            </div>
        };

        (buttons, body)
    }

    fn view_task_row(&self, ctx: &Context<Self>, task: &api::Task, spinning: bool) -> Html {
        let on_navigate =
            task_route(&task.kind).map(|r| ctx.link().callback(move |_| Msg::Navigate(r.clone())));

        let id = task.id;

        let time = if spinning {
            String::from("now")
        } else {
            eta_label(task.run_at, self.time.now())
        };

        html! {
            <div class="task-row">
                <span class="task-icon">
                    <span class={classes!("icon", if spinning { "arrow-path" } else { "clock" }, spinning.then_some("spin"))} />
                </span>

                { view_task_cells(&task.kind, on_navigate) }

                <span class="task-time">{time}</span>

                <span class="task-actions">
                    if !spinning {
                        <div class="input-group">
                            <Button icon="forward" title="Run now" onclick={ctx.link().callback(move |_| Msg::Bump(id))} />
                            <Button icon="trash" variant={Variant::Danger} title="Remove from queue" onclick={ctx.link().callback(move |_| Msg::Remove(id))} />
                        </div>
                    }
                </span>
            </div>
        }
    }

    fn view_completed_list(&self, ctx: &Context<Self>) -> (Option<Html>, Html) {
        let link = ctx.link();

        if !self.loaded {
            return (None, Self::view_task_skeletons());
        }

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
            <div class="task-grid">
                { for page_completed.map(|t| self.view_completed_row(ctx, t)) }
            </div>
        };

        (buttons, body)
    }

    fn view_completed_row(&self, ctx: &Context<Self>, task: &api::CompletedTask) -> Html {
        let on_navigate =
            task_route(&task.kind).map(|r| ctx.link().callback(move |_| Msg::Navigate(r.clone())));

        html! {
            <div class="task-row">
                <span class="task-icon"><span class="icon check" /></span>

                { view_task_cells(&task.kind, on_navigate) }

                <span class="task-time">{ ago_label(task.completed_at, self.time.now()) }</span>

                <span class="task-actions" />
            </div>
        }
    }
}

/// Where a task's subject is shown, if it has one.
fn task_route(kind: &api::TaskKind) -> Option<Route> {
    match kind {
        api::TaskKind::SyncShow { show_id, .. } => {
            Some(Route::ShowDetail(*show_id, ShowDetailQuery::default()))
        }
        api::TaskKind::SyncMovie { movie_id, .. } => Some(Route::MovieDetail(*movie_id)),
        // Land on the episode itself: its season, and its code as the fragment.
        api::TaskKind::SyncEpisode { show_id, code, .. } => Some(Route::ShowDetail(
            *show_id,
            ShowDetailQuery {
                season: code.season,
                episode: Some(*code),
                ..ShowDetailQuery::default()
            },
        )),
        api::TaskKind::SyncPerson { person_id, .. } => Some(Route::PersonDetail(*person_id)),
        api::TaskKind::RefreshTopLanguages => None,
    }
}

/// The kind and subject cells of a task row.
fn view_task_cells(kind: &api::TaskKind, on_navigate: Option<Callback<MouseEvent>>) -> Html {
    let label = match kind {
        api::TaskKind::SyncShow { .. } => "Show",
        api::TaskKind::SyncMovie { .. } => "Movie",
        api::TaskKind::SyncEpisode { .. } => "Episode",
        api::TaskKind::SyncPerson { .. } => "Person",
        api::TaskKind::RefreshTopLanguages => "Languages",
    };

    html! {
        <>
            <span class="task-kind">{label}</span>

            <span class={classes!("task-title", on_navigate.is_some().then_some("clickable"))} onclick={on_navigate}>
                if let api::TaskKind::RefreshTopLanguages = kind {
                    <span class="text-muted">{"Top languages"}</span>
                } else if let Some(title) = kind.title() {
                    <span>{title}</span>
                } else {
                    <span class="text-muted">{"Untitled"}</span>
                }

                if let Some(code) = task_code(kind) {
                    <span class="text-muted">{code}</span>
                }
            </span>
        </>
    }
}

/// The episode code a task targets, when it targets one. `title` alone would render
/// two episodes of the same show identically.
fn task_code(kind: &api::TaskKind) -> Option<api::Code> {
    match kind {
        api::TaskKind::SyncEpisode { code, .. } => Some(*code),
        _ => None,
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
