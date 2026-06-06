use std::collections::{HashMap, VecDeque};
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

use anyhow::Result;
use musli_web::api::ChannelId;
use tokio::sync::{Mutex, Notify, broadcast};
use tokio::time::Instant;

use crate::remote::RemoteClients;
use crate::sync;
use db::Database;

const TASK_DELAY: Duration = Duration::from_secs(5);
const MAX_COMPLETED: usize = 20;

struct ScheduledTask {
    run_at: Instant,
    task: api::Task,
}

struct Inner {
    pending: VecDeque<ScheduledTask>,
    running: Option<api::Task>,
    completed: VecDeque<api::CompletedTask>,
    series_pending: HashMap<api::SeriesId, api::TaskId>,
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
                series_pending: HashMap::new(),
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
        broadcast: &broadcast::Sender<api::AppEvent>,
    ) -> bool {
        let mut inner = self.inner.lock().await;

        // Deduplication check
        let already_queued = match &kind {
            api::TaskKind::SyncSeries { series_id, .. } => {
                inner.series_pending.contains_key(series_id)
                    || inner
                        .running
                        .as_ref()
                        .is_some_and(|t| matches!(&t.kind, api::TaskKind::SyncSeries { series_id: id, .. } if id == series_id))
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
            return false;
        }

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
            api::TaskKind::SyncSeries { series_id, .. } => {
                inner.series_pending.insert(*series_id, id);
            }
            api::TaskKind::SyncMovie { movie_id, .. } => {
                inner.movie_pending.insert(*movie_id, id);
            }
        }

        let task = api::Task {
            id,
            kind,
            status: api::TaskStatus::Pending,
        };

        inner.pending.push_back(ScheduledTask {
            run_at,
            task: task.clone(),
        });

        let _ = broadcast.send(api::AppEvent {
            channel: ChannelId::NONE,
            kind: api::AppEventKind::TaskAdded { task },
        });

        self.notify.notify_one();
        true
    }

    pub(crate) async fn list(&self) -> api::ListTasksResponse {
        let inner = self.inner.lock().await;
        api::ListTasksResponse {
            pending: inner.pending.iter().map(|s| s.task.clone()).collect(),
            running: inner.running.iter().cloned().collect(),
            completed: inner.completed.iter().cloned().collect(),
        }
    }

    pub(crate) async fn run(
        self,
        db: Database,
        remote: RemoteClients,
        broadcast: broadcast::Sender<api::AppEvent>,
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

            let _ = broadcast.send(api::AppEvent {
                channel: ChannelId::NONE,
                kind: api::AppEventKind::TaskStarted { task: task.clone() },
            });

            let result = execute(&task, &db, &remote, &broadcast).await;
            if let Err(e) = result {
                tracing::error!(task_id = ?task.id, error = %e, "task failed");
            }

            let completed = api::CompletedTask {
                id: task.id,
                kind: task.kind.clone(),
            };

            {
                let mut inner = self.inner.lock().await;
                inner.running = None;
                match &task.kind {
                    api::TaskKind::SyncSeries { series_id, .. } => {
                        inner.series_pending.remove(series_id);
                    }
                    api::TaskKind::SyncMovie { movie_id, .. } => {
                        inner.movie_pending.remove(movie_id);
                    }
                }
                inner.completed.push_front(completed.clone());
                inner.completed.truncate(MAX_COMPLETED);
            }

            let _ = broadcast.send(api::AppEvent {
                channel: ChannelId::NONE,
                kind: api::AppEventKind::TaskCompleted { task: completed },
            });
        }
    }
}

async fn execute(
    task: &api::Task,
    db: &Database,
    remote: &RemoteClients,
    broadcast: &broadcast::Sender<api::AppEvent>,
) -> Result<()> {
    match &task.kind {
        api::TaskKind::SyncSeries { series_id, .. } => {
            sync::sync_series(*series_id, db, remote, broadcast).await
        }
        api::TaskKind::SyncMovie { movie_id, .. } => {
            sync::sync_movie(*movie_id, db, remote, broadcast).await
        }
    }
}
