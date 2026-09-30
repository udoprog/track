use gloo::render::{AnimationFrame, request_animation_frame};
use gloo::timers::callback::Interval;
use musli_web::web03::prelude::*;
use yew::prelude::*;

use crate::SetupChannel;
use crate::background::Background;
use crate::error::{CustomContext, Error, Message};
use crate::router::{QueueFilter, QueueQuery, Route, Router, ShowDetailQuery};
use crate::ui::{Button, Link, PaginationButtons, Skeleton, Variant};

const PAGE_SIZE: usize = 20;
/// Completed tasks kept, matching the server's history.
const HISTORY: usize = 500;

/// Where a task is in its life. Rows keep their place in the timeline while
/// this changes.
enum State {
    Pending { run_at: Option<api::Timestamp> },
    Running { since: Option<api::Timestamp> },
    Done(api::CompletedTask),
}

struct Entry {
    id: api::TaskId,
    kind: api::TaskKind,
    state: State,
}

impl Entry {
    fn pending(task: api::Task) -> Self {
        Self {
            id: task.id,
            kind: task.kind,
            state: State::Pending {
                run_at: task.run_at,
            },
        }
    }

    fn is_pending(&self) -> bool {
        matches!(self.state, State::Pending { .. })
    }

    fn is_done(&self) -> bool {
        matches!(self.state, State::Done(..))
    }

    fn matches(&self, filter: QueueFilter) -> bool {
        match (filter, &self.state) {
            (QueueFilter::All, _) => true,
            (QueueFilter::Upcoming, State::Pending { .. } | State::Running { .. }) => true,
            (QueueFilter::Done, State::Done(task)) => task.error.is_none(),
            (QueueFilter::Failed, State::Done(task)) => task.error.is_some(),
            _ => false,
        }
    }
}

pub(crate) struct Queue {
    channel: ws::Channel,
    background: Background,
    router: Router,
    /// Every task in the order it runs: completed, running, then pending.
    entries: Vec<Entry>,
    /// Whether the initial task lists have loaded; gates skeleton placeholders.
    loaded: bool,
    now: api::Timestamp,
    /// A render waiting for the next animation frame, so a burst of queue
    /// events renders once.
    frame: Option<AnimationFrame>,
    _tick: Interval,
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
    Frame,
    Tick,
    SyncAll,
    SyncAllDone(Result<ws::Packet<api::SyncAll>, ws::Error>),
    Remove(api::TaskId),
    RemoveDone(Result<ws::Packet<api::RemoveTask>, ws::Error>),
    Bump(api::TaskId),
    BumpDone(Result<ws::Packet<api::BumpTask>, ws::Error>),
    Query(QueueQuery),
}

#[derive(Properties, PartialEq)]
pub(crate) struct Props {
    pub(crate) filter: QueueFilter,
    /// The page shown, or `None` to follow the running task.
    pub(crate) page: Option<usize>,
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

        let _tick = Interval::new(1000, {
            let link = ctx.link().clone();
            move || link.send_message(Msg::Tick)
        });

        Self {
            channel: ws::Channel::default(),
            background,
            router,
            entries: Vec::new(),
            loaded: false,
            now: api::Timestamp::now(),
            frame: None,
            _tick,
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
        html! {
            <>
                <h1 class="visually-hidden">{"Queue"}</h1>

                { self.view_now(ctx) }

                if self.loaded {
                    { self.view_timeline(ctx) }
                } else {
                    <div class="column">
                        { for (0..3).map(|_| html! { <Skeleton /> }) }
                    </div>
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
                    self.entries.clear();
                    self.loaded = false;
                }
                Ok(true)
            }
            Msg::AppBroadcast(packet) => {
                let event = packet?.decode_event()?;

                match event.kind {
                    api::AppEventKind::TaskAdded { task } => {
                        self.entries.push(Entry::pending(task));
                    }
                    api::AppEventKind::TaskBumped { task } => {
                        self.move_to_next(Entry::pending(task));
                    }
                    api::AppEventKind::TaskRemoved { task_id } => {
                        self.entries.retain(|e| e.id != task_id);
                    }
                    api::AppEventKind::TaskStarted { task } => {
                        self.move_to_next(Entry {
                            id: task.id,
                            kind: task.kind,
                            state: State::Running { since: task.run_at },
                        });
                    }
                    api::AppEventKind::TaskCompleted { task } => {
                        self.complete(task);
                    }
                    _ => return Ok(false),
                }

                self.render_next_frame(ctx);
                Ok(false)
            }
            Msg::TasksLoaded(result) => {
                let resp = result
                    .context(Message::LoadingTasks)?
                    .decode()
                    .context(Message::LoadingTasks)?;

                let done = resp.completed.into_iter().rev().map(|task| Entry {
                    id: task.id,
                    kind: task.kind.clone(),
                    state: State::Done(task),
                });

                let running = resp.running.into_iter().map(|task| Entry {
                    id: task.id,
                    kind: task.kind,
                    state: State::Running { since: task.run_at },
                });

                self.entries = done
                    .chain(running)
                    .chain(resp.pending.into_iter().map(Entry::pending))
                    .collect();

                self.loaded = true;
                Ok(true)
            }
            Msg::Frame => {
                self.frame = None;
                Ok(true)
            }
            Msg::Tick => {
                self.now = api::Timestamp::now();
                Ok(!self.entries.is_empty())
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
            Msg::Query(query) => {
                self.router.push(Route::Queue(query));
                Ok(false)
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

    fn render_next_frame(&mut self, ctx: &Context<Self>) {
        if self.frame.is_none() {
            let link = ctx.link().clone();
            self.frame = Some(request_animation_frame(move |_| {
                link.send_message(Msg::Frame)
            }));
        }
    }

    /// Put `entry` where the next task to run goes: ahead of every pending
    /// task, replacing its old row if it has one.
    fn move_to_next(&mut self, entry: Entry) {
        self.entries.retain(|e| e.id != entry.id);

        let at = self
            .entries
            .iter()
            .position(Entry::is_pending)
            .unwrap_or(self.entries.len());

        self.entries.insert(at, entry);
    }

    fn complete(&mut self, task: api::CompletedTask) {
        if let Some(entry) = self.entries.iter_mut().find(|e| e.id == task.id) {
            entry.state = State::Done(task);
        } else {
            let at = self
                .entries
                .iter()
                .position(|e| !e.is_done())
                .unwrap_or(self.entries.len());

            self.entries.insert(
                at,
                Entry {
                    id: task.id,
                    kind: task.kind.clone(),
                    state: State::Done(task),
                },
            );
        }

        let done = self.entries.iter().filter(|e| e.is_done()).count();

        if done > HISTORY {
            let mut excess = done - HISTORY;

            self.entries.retain(|e| {
                if excess > 0 && e.is_done() {
                    excess -= 1;
                    false
                } else {
                    true
                }
            });
        }
    }

    /// The fixed card above the timeline: what runs now or next, and Sync all.
    /// It never changes height.
    fn view_now(&self, ctx: &Context<Self>) -> Html {
        let running = self.entries.iter().find_map(|e| match e.state {
            State::Running { since } => Some((e, since)),
            _ => None,
        });

        let next = self.entries.iter().find_map(|e| match e.state {
            State::Pending { run_at } => Some((e, run_at)),
            _ => None,
        });

        let (state, icon, caption, entry) = if let Some((entry, since)) = running {
            (
                "running",
                "arrow-path spin",
                running_label(since, self.now),
                Some(entry),
            )
        } else if let Some((entry, run_at)) = next {
            (
                "waiting",
                "clock",
                format!("Up next {}", eta_label(run_at, self.now)),
                Some(entry),
            )
        } else {
            ("idle", "check", String::from("Idle"), None)
        };

        html! {
            <div class={classes!("queue-now", state)} data-test="queue-now">
                <span class="queue-now-badge"><span class={classes!("icon", icon)} aria-hidden="true" /></span>

                <div class="queue-now-text">
                    <span class="queue-now-caption">{caption}</span>

                    if let Some(entry) = entry {
                        <span class="queue-now-title">
                            <span class="badge">{kind_label(&entry.kind)}</span>
                            { view_task_title(&entry.kind, task_route(&entry.kind), None) }
                        </span>
                    } else {
                        <span class="queue-now-title">{"All caught up"}</span>
                    }
                </div>

                <Button icon="arrow-path" label="Sync all" title="Queue sync for all show and movies" onclick={ctx.link().callback(|_| Msg::SyncAll)} />
            </div>
        }
    }

    fn count(&self, filter: QueueFilter) -> usize {
        self.entries.iter().filter(|e| e.matches(filter)).count()
    }

    fn view_timeline(&self, ctx: &Context<Self>) -> Html {
        let link = ctx.link();
        let filter = ctx.props().filter;

        let entries = self
            .entries
            .iter()
            .filter(|e| e.matches(filter))
            .collect::<Vec<_>>();

        let total_pages = entries.len().div_ceil(PAGE_SIZE).max(1);

        // Follow the running task, or the next one to run; with neither, the
        // newest entry.
        let followed = entries
            .iter()
            .position(|e| !e.is_done())
            .unwrap_or(entries.len().saturating_sub(1));

        let following = ctx.props().page.is_none();

        let page = ctx
            .props()
            .page
            .unwrap_or(followed / PAGE_SIZE)
            .min(total_pages - 1);

        let chip = |f: QueueFilter, icon: &'static str, label: &'static str| {
            let onclick = link.callback(move |_| {
                Msg::Query(QueueQuery {
                    filter: f,
                    page: None,
                })
            });

            html! {
                <Button {icon} {label} title={format!("Show {} tasks", label.to_lowercase())} class={classes!("chip", (filter == f).then_some("selected"))} pressed={Some(filter == f)} {onclick}>
                    <span class="chip-count">{self.count(f)}</span>
                </Button>
            }
        };

        let on_page = link.callback(move |page| {
            Msg::Query(QueueQuery {
                filter,
                page: Some(page),
            })
        });

        let on_follow = link.callback(move |_| Msg::Query(QueueQuery { filter, page: None }));

        html! {
            <div class="column">
                <div class="row-split queue-controls">
                    <div class="chips queue-filters">
                        { chip(QueueFilter::All, "queue-list", "All") }
                        { chip(QueueFilter::Upcoming, "clock", "Upcoming") }
                        { chip(QueueFilter::Done, "check", "Done") }
                        { chip(QueueFilter::Failed, "x-mark", "Failed") }
                    </div>

                    <div class="row">
                        if !following {
                            <Button icon="arrow-down-circle" label="Follow" title="Follow the running task" onclick={on_follow} />
                        }

                        if total_pages > 1 {
                            <PaginationButtons {page} {total_pages} {on_page} />
                        }
                    </div>
                </div>

                <div class="task-grid task-timeline" style={format!("--page-rows: {PAGE_SIZE}")}>
                    if entries.is_empty() {
                        <p class="task-empty">
                            {match filter {
                                QueueFilter::All => "Nothing has been synced yet.",
                                QueueFilter::Upcoming => "Nothing is waiting to sync.",
                                QueueFilter::Done => "Nothing has finished syncing yet.",
                                QueueFilter::Failed => "No syncs have failed.",
                            }}
                        </p>
                    }

                    { for entries.iter().skip(page * PAGE_SIZE).take(PAGE_SIZE).map(|e| self.view_row(ctx, e)) }
                </div>
            </div>
        }
    }

    fn view_row(&self, ctx: &Context<Self>, entry: &Entry) -> Html {
        let id = entry.id;
        let route = task_route(&entry.kind);

        let (state, icon, duration, time) = match &entry.state {
            State::Pending { run_at } => (
                "pending",
                classes!("icon", "clock"),
                None,
                eta_label(*run_at, self.now),
            ),
            State::Running { since } => (
                "running",
                classes!("icon", "arrow-path", "spin"),
                since.and_then(|since| elapsed_label(since, self.now)),
                String::from("now"),
            ),
            State::Done(task) => (
                if task.error.is_some() {
                    "failed"
                } else {
                    "done"
                },
                classes!(
                    "icon",
                    if task.error.is_some() {
                        "x-mark"
                    } else {
                        "check"
                    }
                ),
                Some(duration_label(task.duration.millis())),
                ago_label(task.completed_at, self.now),
            ),
        };

        let error = match &entry.state {
            State::Done(task) => task.error.as_deref(),
            _ => None,
        };

        html! {
            <div key={id.get()} class={classes!("task-row", state)} data-task={id.get().to_string()} title={error.map(str::to_owned)}>
                <span class="task-icon"><span class={icon} /></span>

                { view_task_cells(&entry.kind, route, error) }

                <span class="task-duration">{duration}</span>
                <span class="task-time">{time}</span>

                <span class="task-actions">
                    if entry.is_pending() {
                        <div class="input-group">
                            <Button icon="forward" title="Run now" onclick={ctx.link().callback(move |_| Msg::Bump(id))} />
                            <Button icon="trash" variant={Variant::Danger} title="Remove from queue" onclick={ctx.link().callback(move |_| Msg::Remove(id))} />
                        </div>
                    }
                </span>

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

/// What kind of task this is, in a word.
fn kind_label(kind: &api::TaskKind) -> &'static str {
    match kind {
        api::TaskKind::SyncShow { .. } => "Show",
        api::TaskKind::SyncMovie { .. } => "Movie",
        api::TaskKind::SyncEpisode { .. } => "Episode",
        api::TaskKind::SyncPerson { .. } => "Person",
        api::TaskKind::RefreshTopLanguages => "Languages",
    }
}

/// The kind and subject cells of a task row.
fn view_task_cells(kind: &api::TaskKind, route: Option<Route>, error: Option<&str>) -> Html {
    html! {
        <>
            <span class="task-kind">{kind_label(kind)}</span>
            { view_task_title(kind, route, error) }
        </>
    }
}

/// A task's subject: its title, episode code and any error.
fn view_task_title(kind: &api::TaskKind, route: Option<Route>, error: Option<&str>) -> Html {
    let body = html! {
        <>
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

                if let Some(error) = error {
                    <span class="task-error">{error}</span>
                }
        </>
    };

    html! {
        if let Some(to) = route {
            <Link {to} class="task-title">{body}</Link>
        } else {
            <span class="task-title">{body}</span>
        }
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

/// How long a task ran, e.g. "<0.1s", "0.4s", "12s", "3m 4s" or "1h 2m".
fn duration_label(millis: i64) -> String {
    if millis < 50 {
        String::from("<0.1s")
    } else if millis < 10_000 {
        format!("{:.1}s", millis.max(0) as f64 / 1000.0)
    } else {
        seconds_label(millis as u64 / 1000)
    }
}

/// How long a running task has run so far, in whole seconds, or `None` in its
/// first second, where "0s" would read like a finished countdown.
fn elapsed_label(since: api::Timestamp, now: api::Timestamp) -> Option<String> {
    let secs = now.checked_duration_since(since)?.as_secs();
    (secs > 0).then(|| seconds_label(secs))
}

/// What the Now strip says about the running task.
fn running_label(since: Option<api::Timestamp>, now: api::Timestamp) -> String {
    match since.and_then(|since| elapsed_label(since, now)) {
        Some(elapsed) => format!("Running for {elapsed}"),
        None => String::from("Running"),
    }
}

fn seconds_label(secs: u64) -> String {
    if secs < 60 {
        format!("{secs}s")
    } else if secs < 3600 {
        format!("{}m {}s", secs / 60, secs % 60)
    } else {
        format!("{}h {}m", secs / 3600, secs % 3600 / 60)
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
