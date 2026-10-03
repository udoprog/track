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
/// Completed tasks kept for the queue page, newest first.
const COMPLETED_HISTORY: usize = 500;

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
    /// An immediate request for the running task arrived while it ran, so it
    /// runs again once it finishes: the request may follow an edit the
    /// running sync has not seen.
    rerun: bool,
    completed: VecDeque<api::CompletedTask>,
    show_pending: HashMap<api::ShowId, api::TaskId>,
    movie_pending: HashMap<api::MovieId, api::TaskId>,
    episode_pending: HashMap<api::EpisodeId, api::TaskId>,
    person_pending: HashMap<api::PersonId, api::TaskId>,
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
                rerun: false,
                completed: VecDeque::new(),
                show_pending: HashMap::new(),
                movie_pending: HashMap::new(),
                episode_pending: HashMap::new(),
                person_pending: HashMap::new(),
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
            api::TaskKind::SyncEpisode { episode_id, .. } => {
                inner.episode_pending.contains_key(episode_id)
                    || inner
                        .running
                        .as_ref()
                        .is_some_and(|t| matches!(&t.kind, api::TaskKind::SyncEpisode { episode_id: id, .. } if id == episode_id))
            }
            api::TaskKind::SyncPerson { person_id, .. } => {
                inner.person_pending.contains_key(person_id)
                    || inner
                        .running
                        .as_ref()
                        .is_some_and(|t| matches!(&t.kind, api::TaskKind::SyncPerson { person_id: id, .. } if id == person_id))
            }
            // Singleton task: at most one queued or running at a time.
            api::TaskKind::RefreshTopLanguages => {
                inner
                    .pending
                    .iter()
                    .any(|s| matches!(s.task.kind, api::TaskKind::RefreshTopLanguages))
                    || inner
                        .running
                        .as_ref()
                        .is_some_and(|t| matches!(t.kind, api::TaskKind::RefreshTopLanguages))
            }
        };

        if already_queued {
            // A user-initiated (immediate) request for an already-queued task
            // bumps the existing pending entry to the top so it runs next, or,
            // if the task is only running, runs it again after it finishes.
            if immediate {
                let pos = inner
                    .pending
                    .iter()
                    .position(|s| same_target(&s.task.kind, &kind));

                if let Some(pos) = pos {
                    self.bump_at(&mut inner, pos, broadcast);
                } else if inner
                    .running
                    .as_ref()
                    .is_some_and(|t| same_target(&t.kind, &kind))
                {
                    info!(task_kind = ?kind, "Task re-run after the running one");
                    inner.rerun = true;
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
            api::TaskKind::SyncEpisode { episode_id, .. } => {
                inner.episode_pending.insert(*episode_id, id);
            }
            api::TaskKind::SyncPerson { person_id, .. } => {
                inner.person_pending.insert(*person_id, id);
            }
            // Deduped by scanning pending/running, not via an id map.
            api::TaskKind::RefreshTopLanguages => {}
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

        // An immediate task (such as the first sync of a newly tracked show)
        // runs next, ahead of everything already waiting.
        if immediate {
            inner.pending.push_front(scheduled);
        } else {
            inner.pending.push_back(scheduled);
        }

        broadcast.emit(
            ChannelId::NONE,
            api::AppEventKind::TaskAdded {
                task: emitted.clone(),
            },
            "task queue task added",
        );

        // Listeners place added tasks last; say that this one goes first.
        if immediate {
            broadcast.emit(
                ChannelId::NONE,
                api::AppEventKind::TaskBumped { task: emitted },
                "task queue task queued first",
            );
        }

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
            api::TaskKind::SyncEpisode { episode_id, .. } => {
                inner.episode_pending.remove(episode_id);
            }
            api::TaskKind::SyncPerson { person_id, .. } => {
                inner.person_pending.remove(person_id);
            }
            api::TaskKind::RefreshTopLanguages => {}
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
                    t.run_at = Some(api::Timestamp::now());
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
            let result = tokio::select! {
                result = execute(&task, &db, &remote, &broadcast, &pending, &shutdown) => result,
                _ = shutdown.cancelled() => {
                    info!(task_id = ?task.id, "Task interrupted by shutdown");
                    break;
                }
            };
            let elapsed = start.elapsed();

            let error = match result {
                Ok(()) => {
                    info!(?task.id, elapsed_ms = elapsed.as_millis(), "Task completed");

                    match &task.kind {
                        api::TaskKind::SyncShow { show_id, .. } => {
                            if let Ok(Some(show)) = db.show_by_id(None, *show_id).await {
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
                            if let Ok(Some(movie)) = db.movie_by_id(None, *movie_id).await {
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
                        // `execute` already broadcasts EpisodeChanged / EpisodesChanged /
                        // PendingChanged, PersonChanged, and TopLanguagesChanged, respectively.
                        api::TaskKind::SyncEpisode { .. }
                        | api::TaskKind::SyncPerson { .. }
                        | api::TaskKind::RefreshTopLanguages => {}
                    }

                    None
                }
                Err(e) => {
                    error!(task_id = ?task.id, "Task failed: {e:#}");
                    Some(format!("{e:#}"))
                }
            };

            let completed = api::CompletedTask {
                id: task.id,
                kind: task.kind.clone(),
                completed_at: api::Timestamp::now(),
                duration: api::Duration::from_millis(
                    i64::try_from(elapsed.as_millis()).unwrap_or(i64::MAX),
                ),
                error,
            };

            let rerun = {
                let mut inner = self.inner.lock().await;
                inner.running = None;
                match &task.kind {
                    api::TaskKind::SyncShow { show_id, .. } => {
                        inner.show_pending.remove(show_id);
                    }
                    api::TaskKind::SyncMovie { movie_id, .. } => {
                        inner.movie_pending.remove(movie_id);
                    }
                    api::TaskKind::SyncEpisode { episode_id, .. } => {
                        inner.episode_pending.remove(episode_id);
                    }
                    api::TaskKind::SyncPerson { person_id, .. } => {
                        inner.person_pending.remove(person_id);
                    }
                    api::TaskKind::RefreshTopLanguages => {}
                }
                inner.completed.push_front(completed.clone());
                inner.completed.truncate(COMPLETED_HISTORY);
                std::mem::take(&mut inner.rerun)
            };

            broadcast.emit(
                ChannelId::NONE,
                api::AppEventKind::TaskCompleted { task: completed },
                "task queue task completed",
            );

            if rerun {
                self.push(task.kind, true, &broadcast).await;
            }

            if shutdown.is_cancelled() {
                break;
            }
        }
    }
}

/// Whether two tasks sync the same thing.
fn same_target(a: &api::TaskKind, b: &api::TaskKind) -> bool {
    match (a, b) {
        (
            api::TaskKind::SyncShow { show_id: a, .. },
            api::TaskKind::SyncShow { show_id: b, .. },
        ) => a == b,
        (
            api::TaskKind::SyncMovie { movie_id: a, .. },
            api::TaskKind::SyncMovie { movie_id: b, .. },
        ) => a == b,
        (
            api::TaskKind::SyncEpisode { episode_id: a, .. },
            api::TaskKind::SyncEpisode { episode_id: b, .. },
        ) => a == b,
        (
            api::TaskKind::SyncPerson { person_id: a, .. },
            api::TaskKind::SyncPerson { person_id: b, .. },
        ) => a == b,
        (api::TaskKind::RefreshTopLanguages, api::TaskKind::RefreshTopLanguages) => true,
        _ => false,
    }
}

async fn execute(
    task: &api::Task,
    db: &Database,
    remote: &RemoteClients,
    broadcast: &Broadcaster,
    pending: &crate::pending::PendingSystem,
    shutdown: &crate::shutdown::Shutdown,
) -> Result<()> {
    match &task.kind {
        api::TaskKind::SyncShow { show_id, .. } => {
            sync::sync_show(*show_id, db, remote, broadcast, pending, shutdown).await
        }
        api::TaskKind::SyncMovie { movie_id, .. } => {
            sync::sync_movie(*movie_id, db, remote, broadcast, shutdown).await
        }
        api::TaskKind::SyncEpisode {
            show_id,
            episode_id,
            ..
        } => {
            sync::sync_episode(
                *show_id,
                *episode_id,
                db,
                remote,
                broadcast,
                pending,
                shutdown,
            )
            .await
        }
        api::TaskKind::SyncPerson { person_id, .. } => {
            sync::sync_person(*person_id, db, remote, broadcast, shutdown).await
        }
        api::TaskKind::RefreshTopLanguages => {
            crate::background::refresh_top_languages(db, broadcast).await
        }
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use tokio::sync::broadcast;

    use super::{COMPLETED_HISTORY, TaskQueue};
    use crate::app_broadcast::Broadcaster;
    use crate::db::{Database, OpenMode};
    use crate::pending::PendingSystem;
    use crate::remote::RemoteClients;
    use crate::shutdown::Shutdown;

    struct Running {
        queue: TaskQueue,
        broadcast: Broadcaster,
        shutdown: Shutdown,
        worker: tokio::task::JoinHandle<()>,
        _dir: tempfile::TempDir,
    }

    impl Running {
        fn start() -> Running {
            let dir = tempfile::tempdir().unwrap();
            let db = Database::open(dir.path().join("test.db"), OpenMode::Bulk, 1).unwrap();
            let (tx, _) = broadcast::channel(4096);
            let broadcast = Broadcaster::new(tx);
            let queue = TaskQueue::new();
            let shutdown = Shutdown::new();

            let worker = tokio::spawn(queue.clone().run(
                db.clone(),
                RemoteClients::new(reqwest::Client::new(), reqwest::Client::new()),
                broadcast.clone(),
                PendingSystem::new(db),
                shutdown.clone(),
            ));

            Running {
                queue,
                broadcast,
                shutdown,
                worker,
                _dir: dir,
            }
        }

        /// Queue a sync of a show that does not exist, which fails without
        /// touching the network.
        async fn push_failing(&self) {
            let kind = api::TaskKind::SyncShow {
                show_id: api::ShowId::random(),
                title: None,
            };

            assert!(self.queue.push(kind, true, &self.broadcast).await);
        }

        async fn stop(self) {
            self.shutdown.cancel();
            self.worker.await.unwrap();
        }
    }

    #[tokio::test]
    async fn failed_task_reports_its_error() {
        let running = Running::start();
        let mut events = running.broadcast.subscribe();
        running.push_failing().await;

        let completed = tokio::time::timeout(Duration::from_secs(10), async {
            loop {
                if let api::AppEventKind::TaskCompleted { task } =
                    events.recv().await.unwrap().event.kind
                {
                    break task;
                }
            }
        })
        .await
        .unwrap();

        let error = completed.error.expect("the task failed");
        assert!(error.contains("Expected show to exist"), "{error}");

        let list = running.queue.list().await;
        assert_eq!(list.completed.len(), 1);
        assert!(list.completed[0].error.is_some());
        running.stop().await;
    }

    #[tokio::test]
    async fn immediate_tasks_run_before_waiting_ones() {
        let queue = TaskQueue::new();
        let (tx, _) = broadcast::channel(16);
        let broadcast = Broadcaster::new(tx);

        let waiting = api::TaskKind::SyncShow {
            show_id: api::ShowId::random(),
            title: Some("Waiting".into()),
        };
        let tracked = api::TaskKind::SyncShow {
            show_id: api::ShowId::random(),
            title: Some("Just tracked".into()),
        };

        assert!(queue.push(waiting, false, &broadcast).await);
        assert!(queue.push(tracked, true, &broadcast).await);

        let list = queue.list().await;
        assert_eq!(list.pending.len(), 2);
        assert!(matches!(
            &list.pending[0].kind,
            api::TaskKind::SyncShow { title: Some(title), .. } if title == "Just tracked"
        ));
    }

    #[tokio::test]
    async fn completed_history_is_capped() {
        let running = Running::start();

        for _ in 0..COMPLETED_HISTORY + 5 {
            running.push_failing().await;
        }

        tokio::time::timeout(Duration::from_secs(60), async {
            loop {
                let list = running.queue.list().await;

                if list.pending.is_empty() && list.running.is_empty() {
                    break;
                }

                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        })
        .await
        .unwrap();

        assert_eq!(
            running.queue.list().await.completed.len(),
            COMPLETED_HISTORY
        );
        running.stop().await;
    }

    /// A user's request for the running task schedules it again for after the
    /// current run, since it may follow an edit the run has not seen; the
    /// background poll's request does not.
    #[tokio::test]
    async fn immediate_request_for_running_task_reruns_it() {
        let queue = TaskQueue::new();
        let (tx, _) = broadcast::channel(16);
        let broadcast = Broadcaster::new(tx);

        let kind = api::TaskKind::SyncShow {
            show_id: api::ShowId::random(),
            title: None,
        };

        queue.inner.lock().await.running = Some(api::Task {
            id: api::TaskId::new(0),
            kind: kind.clone(),
            status: api::TaskStatus::Running,
            run_at: None,
        });

        assert!(!queue.push(kind.clone(), false, &broadcast).await);
        assert!(!queue.inner.lock().await.rerun);

        assert!(!queue.push(kind, true, &broadcast).await);
        let inner = queue.inner.lock().await;
        assert!(inner.rerun);
        assert!(inner.pending.is_empty());
    }
}
