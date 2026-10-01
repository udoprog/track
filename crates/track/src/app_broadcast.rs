use musli_web::api::ChannelId;
use tokio::sync::broadcast;

/// Whose sockets receive an event.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Audience {
    Everyone,
    User(api::UserId),
    Admins,
}

/// An event and who may see it.
#[derive(Clone)]
pub(crate) struct Broadcast {
    pub(crate) audience: Audience,
    pub(crate) event: api::AppEvent,
}

impl Broadcast {
    /// Whether a socket of `user`, an administrator or not, receives this event.
    pub(crate) fn reaches(&self, user: api::UserId, admin: bool) -> bool {
        match self.audience {
            Audience::Everyone => true,
            Audience::User(u) => u == user,
            Audience::Admins => admin,
        }
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
        self.send(Audience::Everyone, channel, kind);
    }

    /// Send an event about one user's own data to that user's sockets only.
    pub(crate) fn emit_to(
        &self,
        user: api::UserId,
        channel: ChannelId,
        kind: api::AppEventKind,
        _context: &str,
    ) {
        self.send(Audience::User(user), channel, kind);
    }

    /// Send an event about system configuration to administrators' sockets only.
    pub(crate) fn emit_to_admins(&self, channel: ChannelId, kind: api::AppEventKind) {
        self.send(Audience::Admins, channel, kind);
    }

    pub(crate) fn broadcast_event(&self, kind: api::AppEventKind) {
        self.emit(ChannelId::NONE, kind, "sync event");
    }

    fn send(&self, audience: Audience, channel: ChannelId, kind: api::AppEventKind) {
        let event = api::AppEvent { channel, kind };
        _ = self.tx.send(Broadcast { audience, event });
    }
}
