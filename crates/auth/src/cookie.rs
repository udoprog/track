use base64::Engine as _;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use cookie::{Cookie, SameSite};
use hmac::{Hmac, KeyInit, Mac};
use sha2::Sha256;

use crate::SESSION_LIFETIME_DAYS;

/// A new 32-byte key for signing session cookies.
pub fn generate_session_key() -> [u8; 32] {
    rand::random()
}

fn mac(key: &[u8; 32], session_id: &str) -> Hmac<Sha256> {
    let mut mac = Hmac::<Sha256>::new_from_slice(key).expect("HMAC accepts any key size");
    mac.update(session_id.as_bytes());
    mac
}

/// Builds the session cookie named `name`, whose value is
/// `{session_id}.{base64url(HMAC-SHA256(key, session_id))}`. `secure` limits
/// it to HTTPS.
pub fn session_cookie(
    name: &str,
    key: &[u8; 32],
    session_id: &str,
    secure: bool,
) -> Cookie<'static> {
    let signature = URL_SAFE_NO_PAD.encode(mac(key, session_id).finalize().into_bytes());

    Cookie::build((name.to_owned(), format!("{session_id}.{signature}")))
        .http_only(true)
        .same_site(SameSite::Strict)
        .secure(secure)
        .path("/")
        .max_age(cookie::time::Duration::days(SESSION_LIFETIME_DAYS))
        .build()
}

/// Checks a session cookie's signature and returns its session id.
///
/// Session ids are base64url and never contain `.`, so the first `.` separates
/// the signature.
pub fn verify_session_cookie<'a>(key: &[u8; 32], value: &'a str) -> Option<&'a str> {
    let (session_id, signature) = value.split_once('.')?;
    let signature = URL_SAFE_NO_PAD.decode(signature).ok()?;
    mac(key, session_id).verify_slice(&signature).ok()?;
    Some(session_id)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sign_and_verify() {
        let key = generate_session_key();
        let cookie = session_cookie("track_session", &key, "abc_-123", false);

        assert_eq!(cookie.name(), "track_session");
        assert_eq!(cookie.http_only(), Some(true));
        assert_eq!(cookie.secure(), Some(false));
        assert_eq!(cookie.same_site(), Some(SameSite::Strict));
        assert_eq!(cookie.path(), Some("/"));
        assert_eq!(cookie.max_age(), Some(cookie::time::Duration::days(30)));

        assert_eq!(
            verify_session_cookie(&key, cookie.value()),
            Some("abc_-123")
        );

        let cookie = session_cookie("track_session", &key, "abc_-123", true);
        assert_eq!(cookie.secure(), Some(true));
    }

    #[test]
    fn rejects_tampering() {
        let key = generate_session_key();
        let cookie = session_cookie("s", &key, "abc", false);
        let (_, signature) = cookie.value().split_once('.').unwrap();

        assert_eq!(
            verify_session_cookie(&key, &format!("abd.{signature}")),
            None
        );
        assert_eq!(verify_session_cookie(&key, "abc"), None);
        assert_eq!(verify_session_cookie(&key, "abc."), None);
        assert_eq!(verify_session_cookie(&key, "abc.!!!"), None);
        assert_eq!(verify_session_cookie(&[0; 32], cookie.value()), None);

        let mut truncated = cookie.value().to_owned();
        truncated.pop();
        assert_eq!(verify_session_cookie(&key, &truncated), None);
    }
}
