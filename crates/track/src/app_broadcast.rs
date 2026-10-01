use musli_web::api::ChannelId;
use tokio::sync::broadcast;

/// An event and who may see it.
#[derive(Clone)]
pub(crate) struct Broadcast {
    /// The only user whose sockets receive the event, or `None` for everyone.
    pub(crate) user: Option<api::UserId>,
    pub(crate) event: api::AppEvent,
}

impl Broadcast {
    /// Whether a socket of `user` receives this event.
    pub(crate) fn reaches(&self, user: api::UserId) -> bool {
        self.user.is_none_or(|u| u == user)
    }
}

#[derive(Clone)]
pub(crate) struct Broadcaster {
    tx: broadcast::Sender<Broadcast>,
}

impl Broadcaster {
    pub(crate) fn new(tx: broadcast::Sender<Broadcast>) -> Self {
        Self { tx }
    }

    pub(crate) fn subscribe(&self) -> broadcast::Receiver<Broadcast> {
        self.tx.subscribe()
    }

    /// Send an event about shared data to everyone.
    pub(crate) fn emit(&self, channel: ChannelId, kind: api::AppEventKind, _context: &str) {
        self.send(None, channel, kind);
    }

    /// Send an event about one user's own data to that user's sockets only.
    pub(crate) fn emit_to(
        &self,
        user: api::UserId,
        channel: ChannelId,
        kind: api::AppEventKind,
        _context: &str,
    ) {
        self.send(Some(user), channel, kind);
    }

    pub(crate) fn broadcast_event(&self, kind: api::AppEventKind) {
        self.emit(ChannelId::NONE, kind, "sync event");
    }

    fn send(&self, user: Option<api::UserId>, channel: ChannelId, kind: api::AppEventKind) {
        let event = api::AppEvent { channel, kind };
        _ = self.tx.send(Broadcast { user, event });
    }
}
