use musli_web::api::ChannelId;
use tokio::sync::broadcast;

#[derive(Clone)]
pub(crate) struct Broadcaster {
    tx: broadcast::Sender<api::AppEvent>,
}

impl Broadcaster {
    pub(crate) fn new(tx: broadcast::Sender<api::AppEvent>) -> Self {
        Self { tx }
    }

    pub(crate) fn subscribe(&self) -> broadcast::Receiver<api::AppEvent> {
        self.tx.subscribe()
    }

    pub(crate) fn emit(&self, channel: ChannelId, kind: api::AppEventKind, context: &str) {
        let event = api::AppEvent { channel, kind };

        if let Err(error) = self.tx.send(event) {
            tracing::warn!(%error, %context, "broadcast failed");
        }
    }

    pub(crate) fn broadcast_event(&self, kind: api::AppEventKind) {
        self.emit(ChannelId::NONE, kind, "sync event");
    }
}
