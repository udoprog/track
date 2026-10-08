use tokio::sync::watch;

/// Where a running sync is. Updating it is cheap; the task queue samples it
/// and tells clients, so a sync may report every unit of work.
#[derive(Clone)]
pub(crate) struct Progress {
    tx: watch::Sender<Option<api::TaskProgress>>,
}

impl Default for Progress {
    fn default() -> Self {
        Self {
            tx: watch::Sender::new(None),
        }
    }
}

impl Progress {
    pub(crate) fn subscribe(&self) -> watch::Receiver<Option<api::TaskProgress>> {
        self.tx.subscribe()
    }

    /// Start `step`, fetching from `source`, with `total` units when it is known.
    pub(crate) fn step(
        &self,
        step: api::TaskStep,
        source: Option<api::RemoteSource>,
        total: Option<usize>,
    ) {
        self.tx.send_replace(Some(api::TaskProgress {
            step,
            source,
            done: 0,
            total: total.map(|n| u32::try_from(n).unwrap_or(u32::MAX)),
        }));
    }

    /// Finish one unit of the current step.
    pub(crate) fn advance(&self) {
        self.tx.send_modify(|p| {
            if let Some(p) = p {
                p.done = p.done.saturating_add(1);
            }
        });
    }

    /// Say that `done` units of the current step are finished.
    pub(crate) fn set_done(&self, done: usize) {
        self.tx.send_modify(|p| {
            if let Some(p) = p {
                p.done = u32::try_from(done).unwrap_or(u32::MAX);
            }
        });
    }
}
