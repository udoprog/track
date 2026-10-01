use base64::Engine as _;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use jiff::{SignedDuration, Timestamp};

pub const SESSION_LIFETIME_DAYS: i64 = 30;
pub const SESSION_LIFETIME: SignedDuration = SignedDuration::from_hours(SESSION_LIFETIME_DAYS * 24);
pub const LOGIN_TOKEN_LIFETIME: SignedDuration = SignedDuration::from_hours(7 * 24);

fn random_id() -> String {
    URL_SAFE_NO_PAD.encode(rand::random::<[u8; 16]>())
}

/// A new session id: 16 random bytes, base64url without padding, so it never
/// contains the `.` that separates the cookie signature.
pub fn new_session_id() -> String {
    random_id()
}

/// A new single-use login token, in the same form as a session id.
pub fn new_login_token() -> String {
    random_id()
}

fn expiry(now: Timestamp, lifetime: SignedDuration) -> Timestamp {
    now.saturating_add(lifetime)
        .expect("saturating addition of a SignedDuration cannot fail")
}

pub fn session_expiry(now: Timestamp) -> Timestamp {
    expiry(now, SESSION_LIFETIME)
}

pub fn login_token_expiry(now: Timestamp) -> Timestamp {
    expiry(now, LOGIN_TOKEN_LIFETIME)
}

pub fn is_expired(expires_at: Timestamp, now: Timestamp) -> bool {
    expires_at <= now
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ids_are_16_bytes_base64url() {
        let a = new_session_id();
        let b = new_login_token();
        assert_eq!(a.len(), 22);
        assert_eq!(URL_SAFE_NO_PAD.decode(&a).unwrap().len(), 16);
        assert_eq!(URL_SAFE_NO_PAD.decode(&b).unwrap().len(), 16);
        assert_ne!(a, new_session_id());
    }

    #[test]
    fn expiry() {
        let now: Timestamp = "2026-10-01T00:00:00Z".parse().unwrap();

        let session = session_expiry(now);
        assert_eq!(
            session,
            "2026-10-31T00:00:00Z".parse::<Timestamp>().unwrap()
        );
        assert!(!is_expired(session, now));
        assert!(!is_expired(session, session - SignedDuration::from_secs(1)));
        assert!(is_expired(session, session));

        let token = login_token_expiry(now);
        assert_eq!(token, "2026-10-08T00:00:00Z".parse::<Timestamp>().unwrap());
        assert!(!is_expired(token, now));
        assert!(is_expired(token, token + SignedDuration::from_secs(1)));
    }
}
