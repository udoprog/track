use core::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

use musli_web::web03::prelude::*;
use yew::prelude::*;

use crate::SetupChannel;
use crate::background::Background;
use crate::error::{CustomContext, Error, Message};

/// What a sync task syncs.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) enum SyncTarget {
    Show(api::ShowId),
    Movie(api::MovieId),
    Episode(api::EpisodeId),
    Person(api::PersonId),
}

impl SyncTarget {
    fn of(kind: &api::TaskKind) -> Option<Self> {
        match kind {
            api::TaskKind::SyncShow { show_id, .. } => Some(Self::Show(*show_id)),
            api::TaskKind::SyncMovie { movie_id, .. } => Some(Self::Movie(*movie_id)),
            api::TaskKind::SyncEpisode { episode_id, .. } => Some(Self::Episode(*episode_id)),
            api::TaskKind::SyncPerson { person_id, .. } => Some(Self::Person(*person_id)),
            api::TaskKind::RefreshTopLanguages => None,
        }
    }
}

/// The queued and running sync tasks in the server's task queue, provided as
/// context by [`ActiveTasksProvider`]. Loaded on every connect, so it survives
/// page loads and reconnects, then kept current by the task broadcasts.
#[derive(Clone, Default)]
pub(crate) struct ActiveTasks {
    version: u64,
    /// The target the last change touched; `None` after a full reload.
    changed: Option<SyncTarget>,
    tasks: Rc<RefCell<HashMap<SyncTarget, api::TaskId>>>,
}

impl PartialEq for ActiveTasks {
    fn eq(&self, other: &Self) -> bool {
        self.version == other.version
    }
}

impl ActiveTasks {
    /// The queued or running task syncing `target`.
    pub(crate) fn task(&self, target: SyncTarget) -> Option<api::TaskId> {
        self.tasks.borrow().get(&target).copied()
    }

    /// Whether the last change could have touched any target `is_relevant`
    /// accepts, so a consumer only re-renders for its own spinners.
    pub(crate) fn changed(&self, is_relevant: impl FnOnce(SyncTarget) -> bool) -> bool {
        self.changed.is_none_or(is_relevant)
    }

    fn apply(&mut self, change: Change) -> bool {
        let mut tasks = self.tasks.borrow_mut();

        let changed = match change {
            Change::Insert(target, id) => (tasks.insert(target, id) != Some(id)).then_some(target),
            Change::Remove(id) => {
                let target = tasks
                    .iter()
                    .find(|(_, t)| **t == id)
                    .map(|(target, _)| *target);

                if let Some(target) = target {
                    tasks.remove(&target);
                }

                target
            }
        };

        drop(tasks);

        if changed.is_some() {
            self.version += 1;
            self.changed = changed;
        }

        changed.is_some()
    }
}

#[derive(Clone, Copy)]
enum Change {
    Insert(SyncTarget, api::TaskId),
    Remove(api::TaskId),
}

impl Change {
    fn of(kind: &api::AppEventKind) -> Option<Self> {
        match kind {
            api::AppEventKind::TaskAdded { task }
            | api::AppEventKind::TaskBumped { task }
            | api::AppEventKind::TaskStarted { task } => {
                Some(Self::Insert(SyncTarget::of(&task.kind)?, task.id))
            }
            api::AppEventKind::TaskCompleted { task } => Some(Self::Remove(task.id)),
            api::AppEventKind::TaskRemoved { task_id } => Some(Self::Remove(*task_id)),
            _ => None,
        }
    }
}

pub(crate) struct ActiveTasksProvider {
    channel: ws::Channel,
    background: Background,
    tasks: ActiveTasks,
    /// Changes broadcast while the task list loads, replayed over it since the
    /// list may predate them.
    loading: Option<Vec<Change>>,
    _setup: SetupChannel,
    _broadcast: ws::Listener,
    _list_req: ws::Request,
}

pub(crate) enum Msg {
    Channel(Result<ws::Channel, ws::Error>),
    AppBroadcast(Result<ws::Packet<api::AppBroadcast>, ws::Error>),
    TasksLoaded(Result<ws::Packet<api::ListTasks>, ws::Error>),
}

#[derive(Properties, PartialEq)]
pub(crate) struct Props {
    pub(crate) children: Children,
}

impl Component for ActiveTasksProvider {
    type Message = Msg;
    type Properties = Props;

    fn create(ctx: &Context<Self>) -> Self {
        let (ws, _) = ctx
            .link()
            .context::<ws::Handle>(Callback::noop())
            .expect("Expected ws::Handle in context");

        let (background, _) = ctx
            .link()
            .context::<Background>(Callback::noop())
            .expect("Expected background handle in context");

        let _setup = SetupChannel::new(ws.clone(), ctx.link().callback(Msg::Channel));
        let _broadcast = ws.on_broadcast(ctx.link().callback(Msg::AppBroadcast));

        Self {
            channel: ws::Channel::default(),
            background,
            tasks: ActiveTasks::default(),
            loading: None,
            _setup,
            _broadcast,
            _list_req: ws::Request::default(),
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

    fn view(&self, ctx: &Context<Self>) -> Html {
        html! {
            <ContextProvider<ActiveTasks> context={self.tasks.clone()}>
                { for ctx.props().children.iter() }
            </ContextProvider<ActiveTasks>>
        }
    }
}

impl ActiveTasksProvider {
    fn try_update(&mut self, ctx: &Context<Self>, msg: Msg) -> Result<bool, Error> {
        match msg {
            Msg::Channel(result) => {
                self.channel = result?;
                self.load(ctx);
                Ok(false)
            }
            Msg::AppBroadcast(packet) => {
                let event = packet?.decode_event()?;

                if let api::AppEventKind::Resync = event.kind {
                    self.load(ctx);
                    return Ok(false);
                }

                let Some(change) = Change::of(&event.kind) else {
                    return Ok(false);
                };

                if let Some(loading) = &mut self.loading {
                    loading.push(change);
                }

                Ok(self.tasks.apply(change))
            }
            Msg::TasksLoaded(result) => {
                let resp = result
                    .context(Message::LoadingTasks)?
                    .decode()
                    .context(Message::LoadingTasks)?;

                let tasks = resp
                    .running
                    .iter()
                    .chain(&resp.pending)
                    .filter_map(|task| Some((SyncTarget::of(&task.kind)?, task.id)))
                    .collect();

                *self.tasks.tasks.borrow_mut() = tasks;

                for change in self.loading.take().into_iter().flatten() {
                    self.tasks.apply(change);
                }

                self.tasks.version += 1;
                self.tasks.changed = None;
                Ok(true)
            }
        }
    }

    fn load(&mut self, ctx: &Context<Self>) {
        if self.channel.id() == ws::ChannelId::NONE {
            return;
        }

        self.loading = Some(Vec::new());

        self._list_req = self
            .channel
            .request()
            .body(api::ListTasksRequest)
            .on_packet(ctx.link().callback(Msg::TasksLoaded))
            .send();
    }
}
