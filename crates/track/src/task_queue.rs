use std::collections::{HashMap, VecDeque};
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

use anyhow::Result;
use musli_web::api::ChannelId;
use tokio::sync::{Mutex, Notify};
use tokio::time::Instant;
use tracing::{error, info};

use crate::app_broadcast::Broadcaster;
use crate::db::Database;
use crate::remote::RemoteClients;
use crate::sync;

const TASK_DELAY: Duration = Duration::from_secs(5);

struct ScheduledTask {
    run_at: Instant,
    task: api::Task,
}

impl ScheduledTask {
    /// Build a client-facing task with the wall-clock time it is expected to
    /// run, derived from the remaining delay measured against `now`.
    fn to_api(&self, now: Instant) -> api::Task {
        let mut task = self.task.clone();
        task.run_at = Some(api::Timestamp::from_now(
            self.run_at.saturating_duration_since(now),
        ));
        task
    }
}

struct Inner {
    pending: VecDeque<ScheduledTask>,
    running: Option<api::Task>,
    completed: VecDeque<api::CompletedTask>,
    show_pending: HashMap<api::ShowId, api::TaskId>,
    movie_pending: HashMap<api::MovieId, api::TaskId>,
}

#[derive(Clone)]
pub(crate) struct TaskQueue {
    inner: Arc<Mutex<Inner>>,
    notify: Arc<Notify>,
    next_id: Arc<AtomicU64>,
}

impl TaskQueue {
    pub(crate) fn new() -> Self {
        Self {
            inner: Arc::new(Mutex::new(Inner {
                pending: VecDeque::new(),
                running: None,
                completed: VecDeque::new(),
                show_pending: HashMap::new(),
                movie_pending: HashMap::new(),
            })),
            notify: Arc::new(Notify::new()),
            next_id: Arc::new(AtomicU64::new(1)),
        }
    }

    fn next_id(&self) -> api::TaskId {
        api::TaskId::new(self.next_id.fetch_add(1, Ordering::Relaxed))
    }

    pub(crate) async fn push(
        &self,
        kind: api::TaskKind,
        immediate: bool,
        broadcast: &Broadcaster,
    ) -> bool {
        let mut inner = self.inner.lock().await;

        let already_queued = match &kind {
            api::TaskKind::SyncShow { show_id, .. } => {
                inner.show_pending.contains_key(show_id)
                    || inner
                        .running
                        .as_ref()
                        .is_some_and(|t| matches!(&t.kind, api::TaskKind::SyncShow { show_id: id, .. } if id == show_id))
            }
            api::TaskKind::SyncMovie { movie_id, .. } => {
                inner.movie_pending.contains_key(movie_id)
                    || inner
                        .running
                        .as_ref()
                        .is_some_and(|t| matches!(&t.kind, api::TaskKind::SyncMovie { movie_id: id, .. } if id == movie_id))
            }
        };

        if already_queued {
            // A user-initiated (immediate) request for an already-queued task
            // bumps the existing pending entry to the top so it runs next. A
            // task that is only running (not in the pending deque) is already
            // executing, so there is nothing to bump.
            if immediate {
                let pos = inner
                    .pending
                    .iter()
                    .position(|s| match (&s.task.kind, &kind) {
                        (
                            api::TaskKind::SyncShow { show_id: a, .. },
                            api::TaskKind::SyncShow { show_id: b, .. },
                        ) => a == b,
                        (
                            api::TaskKind::SyncMovie { movie_id: a, .. },
                            api::TaskKind::SyncMovie { movie_id: b, .. },
                        ) => a == b,
                        _ => false,
                    });

                if let Some(pos) = pos {
                    self.bump_at(&mut inner, pos, broadcast);
                }
            }

            return false;
        }

        info!(task_kind = ?kind, "Task queued");

        let run_at = if immediate {
            Instant::now()
        } else {
            inner
                .pending
                .back()
                .map(|t| t.run_at + TASK_DELAY)
                .unwrap_or_else(|| Instant::now() + TASK_DELAY)
        };

        let id = self.next_id();

        match &kind {
            api::TaskKind::SyncShow { show_id, .. } => {
                inner.show_pending.insert(*show_id, id);
            }
            api::TaskKind::SyncMovie { movie_id, .. } => {
                inner.movie_pending.insert(*movie_id, id);
            }
        }

        let task = api::Task {
            id,
            kind,
            status: api::TaskStatus::Pending,
            run_at: None,
        };

        let scheduled = ScheduledTask {
            run_at,
            task: task.clone(),
        };

        let emitted = scheduled.to_api(Instant::now());
        inner.pending.push_back(scheduled);

        broadcast.emit(
            ChannelId::NONE,
            api::AppEventKind::TaskAdded { task: emitted },
            "task queue task added",
        );

        self.notify.notify_one();
        true
    }

    /// Remove a pending task from the queue. Running tasks are not removable.
    pub(crate) async fn remove(&self, id: api::TaskId, broadcast: &Broadcaster) -> bool {
        let mut inner = self.inner.lock().await;

        let Some(pos) = inner.pending.iter().position(|s| s.task.id == id) else {
            return false;
        };

        let removed = inner.pending.remove(pos).expect("position is valid");

        match &removed.task.kind {
            api::TaskKind::SyncShow { show_id, .. } => {
                inner.show_pending.remove(show_id);
            }
            api::TaskKind::SyncMovie { movie_id, .. } => {
                inner.movie_pending.remove(movie_id);
            }
        }

        info!(task_id = ?id, "Task removed");

        broadcast.emit(
            ChannelId::NONE,
            api::AppEventKind::TaskRemoved { task_id: id },
            "task queue task removed",
        );

        // The front entry may have changed, so wake the worker to recompute its
        // sleep deadline.
        self.notify.notify_one();
        true
    }

    /// Bump a pending task to the top of the queue so it runs immediately.
    /// Running tasks are already executing and cannot be bumped.
    pub(crate) async fn bump(&self, id: api::TaskId, broadcast: &Broadcaster) -> bool {
        let mut inner = self.inner.lock().await;

        let Some(pos) = inner.pending.iter().position(|s| s.task.id == id) else {
            return false;
        };

        self.bump_at(&mut inner, pos, broadcast);
        true
    }

    /// Move the pending entry at `pos` to the front, reset its run time to now,
    /// notify the worker, and broadcast the bump. Caller holds the lock.
    fn bump_at(&self, inner: &mut Inner, pos: usize, broadcast: &Broadcaster) {
        let mut scheduled = inner.pending.remove(pos).expect("position is valid");
        scheduled.run_at = Instant::now();
        let emitted = scheduled.to_api(scheduled.run_at);
        inner.pending.push_front(scheduled);

        info!(task_id = ?emitted.id, "Task bumped");

        broadcast.emit(
            ChannelId::NONE,
            api::AppEventKind::TaskBumped { task: emitted },
            "task queue task bumped",
        );

        self.notify.notify_one();
    }

    pub(crate) async fn list(&self) -> api::ListTasksResponse {
        let inner = self.inner.lock().await;
        let now = Instant::now();

        api::ListTasksResponse {
            pending: inner.pending.iter().map(|s| s.to_api(now)).collect(),
            running: inner.running.iter().cloned().collect(),
            completed: inner.completed.iter().cloned().collect(),
        }
    }

    #[tracing::instrument(skip_all)]
    pub(crate) async fn run(
        self,
        db: Database,
        remote: RemoteClients,
        broadcast: Broadcaster,
        pending: crate::pending::PendingSystem,
        shutdown: crate::shutdown::Shutdown,
    ) {
        loop {
            // Determine how long to sleep until the next task is ready.
            let sleep_until = {
                let inner = self.inner.lock().await;

                match inner.pending.front() {
                    None => Instant::now() + Duration::from_secs(3600),
                    Some(t) => t.run_at,
                }
            };

            tokio::select! {
                _ = tokio::time::sleep_until(sleep_until) => {}
                _ = self.notify.notified() => { continue; }
                _ = shutdown.cancelled() => { break; }
            }

            // Pop the next task if it's due.
            let task = {
                let mut inner = self.inner.lock().await;
                let now = Instant::now();

                if inner.pending.front().is_some_and(|t| t.run_at <= now) {
                    let mut t = inner.pending.pop_front().unwrap().task;
                    t.status = api::TaskStatus::Running;
                    inner.running = Some(t.clone());
                    Some(t)
                } else {
                    None
                }
            };

            let Some(task) = task else { continue };

            broadcast.emit(
                ChannelId::NONE,
                api::AppEventKind::TaskStarted { task: task.clone() },
                "task queue task started",
            );

            info!(task_id = ?task.id, task_kind = ?task.kind, "Task started");
            let start = Instant::now();
            let result = execute(&task, &db, &remote, &broadcast, &pending).await;

            match result {
                Ok(()) => {
                    info!(?task.id, elapsed_ms = start.elapsed().as_millis(), "Task completed");

                    match &task.kind {
                        api::TaskKind::SyncShow { show_id, .. } => {
                            if let Ok(Some(show)) = db.show_by_id(*show_id).await {
                                broadcast.emit(
                                    ChannelId::NONE,
                                    api::AppEventKind::ShowChanged { show },
                                    "task queue show changed",
                                );
                            }

                            broadcast.emit(
                                ChannelId::NONE,
                                api::AppEventKind::PendingChanged,
                                "task queue pending changed",
                            );
                        }
                        api::TaskKind::SyncMovie { movie_id, .. } => {
                            if let Ok(Some(movie)) = db.movie_by_id(*movie_id).await {
                                broadcast.emit(
                                    ChannelId::NONE,
                                    api::AppEventKind::MovieChanged { movie },
                                    "task queue movie changed",
                                );
                            }
                            broadcast.emit(
                                ChannelId::NONE,
                                api::AppEventKind::PendingChanged,
                                "task queue pending changed",
                            );
                        }
                    }
                }
                Err(e) => {
                    error!(task_id = ?task.id, "Task failed: {e:#}");
                }
            }

            let completed = api::CompletedTask {
                id: task.id,
                kind: task.kind.clone(),
                completed_at: api::Timestamp::now(),
            };

            {
                let mut inner = self.inner.lock().await;
                inner.running = None;
                match &task.kind {
                    api::TaskKind::SyncShow { show_id, .. } => {
                        inner.show_pending.remove(show_id);
                    }
                    api::TaskKind::SyncMovie { movie_id, .. } => {
                        inner.movie_pending.remove(movie_id);
                    }
                }
                inner.completed.push_front(completed.clone());
            }

            broadcast.emit(
                ChannelId::NONE,
                api::AppEventKind::TaskCompleted { task: completed },
                "task queue task completed",
            );

            if shutdown.is_cancelled() {
                break;
            }
        }
    }
}

async fn execute(
    task: &api::Task,
    db: &Database,
    remote: &RemoteClients,
    broadcast: &Broadcaster,
    pending: &crate::pending::PendingSystem,
) -> Result<()> {
    match &task.kind {
        api::TaskKind::SyncShow { show_id, .. } => {
            sync::sync_show(*show_id, db, remote, broadcast, pending).await
        }
        api::TaskKind::SyncMovie { movie_id, .. } => {
            sync::sync_movie(*movie_id, db, remote, broadcast).await
        }
    }
}
