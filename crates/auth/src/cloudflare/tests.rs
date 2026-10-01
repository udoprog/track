use std::sync::atomic::{AtomicUsize, Ordering};

use aws_lc_rs::rand::SystemRandom;
use aws_lc_rs::rsa::{KeyPair, KeySize};
use aws_lc_rs::signature::{KeyPair as _, RSA_PKCS1_SHA256};
use http::HeaderValue;
use serde_json::{Value, json};

use super::*;

const TEAM: &str = "example.cloudflareaccess.com";
const AUD: &str = "aud-tag";

struct Signer {
    kid: String,
    key: KeyPair,
}

impl Signer {
    fn new(kid: &str) -> Self {
        Self {
            kid: kid.to_owned(),
            key: KeyPair::generate(KeySize::Rsa2048).unwrap(),
        }
    }

    fn jwk(&self) -> Jwk {
        let public = PublicKeyComponents::<Vec<u8>>::from(self.key.public_key());

        Jwk {
            kid: self.kid.clone(),
            n: public.n,
            e: public.e,
        }
    }

    fn sign(&self, claims: &Value) -> String {
        let header = json!({ "alg": "RS256", "kid": self.kid, "typ": "JWT" });
        let signed = format!(
            "{}.{}",
            URL_SAFE_NO_PAD.encode(header.to_string()),
            URL_SAFE_NO_PAD.encode(claims.to_string())
        );

        let mut signature = vec![0; self.key.public_modulus_len()];
        self.key
            .sign(
                &RSA_PKCS1_SHA256,
                &SystemRandom::new(),
                signed.as_bytes(),
                &mut signature,
            )
            .unwrap();

        format!("{signed}.{}", URL_SAFE_NO_PAD.encode(signature))
    }
}

#[derive(Default)]
struct FixedKeys {
    keys: Mutex<Vec<Jwk>>,
    fetches: AtomicUsize,
}

impl KeyFetcher for &FixedKeys {
    async fn fetch(&self, team_domain: &str) -> Result<Vec<Jwk>, Box<dyn StdError + Send + Sync>> {
        assert_eq!(team_domain, TEAM);
        self.fetches.fetch_add(1, Ordering::SeqCst);
        Ok(self.keys.lock().unwrap().clone())
    }
}

fn config(trust_email_header: bool, verify_jwt: bool) -> Config {
    Config {
        team_domain: TEAM.to_owned(),
        audience: AUD.to_owned(),
        trust_email_header,
        verify_jwt,
    }
}

fn now() -> Timestamp {
    "2026-10-01T12:00:00Z".parse().unwrap()
}

fn claims() -> Value {
    let now = now().as_second();

    json!({
        "aud": [AUD],
        "iss": format!("https://{TEAM}"),
        "email": "User@Example.com",
        "iat": now - 10,
        "nbf": now - 10,
        "exp": now + 3600,
        "sub": "user-id",
    })
}

fn headers(email: Option<&str>, jwt: Option<&str>) -> HeaderMap {
    let mut headers = HeaderMap::new();

    if let Some(email) = email {
        headers.insert(EMAIL_HEADER, HeaderValue::from_str(email).unwrap());
    }

    if let Some(jwt) = jwt {
        headers.insert(JWT_HEADER, HeaderValue::from_str(jwt).unwrap());
    }

    headers
}

async fn check(signer: &Signer, claims: Value) -> Result<String, AccessError> {
    let keys = FixedKeys::default();
    keys.keys.lock().unwrap().push(signer.jwk());
    let access = Access::with_fetcher(config(false, true), &keys);
    let jwt = signer.sign(&claims);
    access.email_at(&headers(None, Some(&jwt)), now()).await
}

#[tokio::test]
async fn valid_jwt() {
    let signer = Signer::new("k1");
    assert_eq!(check(&signer, claims()).await.unwrap(), "user@example.com");

    let mut single_aud = claims();
    single_aud["aud"] = json!(AUD);
    assert_eq!(
        check(&signer, single_aud).await.unwrap(),
        "user@example.com"
    );
}

#[tokio::test]
async fn wrong_audience() {
    let signer = Signer::new("k1");
    let mut claims = claims();
    claims["aud"] = json!(["other"]);
    assert!(matches!(
        check(&signer, claims).await,
        Err(AccessError::WrongAudience)
    ));
}

#[tokio::test]
async fn wrong_issuer() {
    let signer = Signer::new("k1");
    let mut claims = claims();
    claims["iss"] = json!("https://other.cloudflareaccess.com");
    assert!(matches!(
        check(&signer, claims).await,
        Err(AccessError::WrongIssuer)
    ));
}

#[tokio::test]
async fn expired() {
    let signer = Signer::new("k1");
    let mut claims = claims();
    claims["exp"] = json!(now().as_second() - LEEWAY_SECONDS - 1);
    assert!(matches!(
        check(&signer, claims).await,
        Err(AccessError::Expired)
    ));

    let mut claims = self::claims();
    claims["nbf"] = json!(now().as_second() + LEEWAY_SECONDS + 1);
    assert!(matches!(
        check(&signer, claims).await,
        Err(AccessError::NotYetValid)
    ));
}

#[tokio::test]
async fn bad_signature() {
    let signer = Signer::new("k1");
    let keys = FixedKeys::default();
    keys.keys.lock().unwrap().push(signer.jwk());
    let access = Access::with_fetcher(config(false, true), &keys);

    // Signed by a different key under the same kid.
    let forger = Signer::new("k1");
    let jwt = forger.sign(&claims());
    assert!(matches!(
        access.email_at(&headers(None, Some(&jwt)), now()).await,
        Err(AccessError::BadSignature)
    ));

    // Claims swapped after signing.
    let jwt = signer.sign(&claims());
    let mut other = claims();
    other["email"] = json!("admin@example.com");
    let (header, rest) = jwt.split_once('.').unwrap();
    let (_, signature) = rest.split_once('.').unwrap();
    let tampered = format!(
        "{header}.{}.{signature}",
        URL_SAFE_NO_PAD.encode(other.to_string())
    );
    assert!(matches!(
        access
            .email_at(&headers(None, Some(&tampered)), now())
            .await,
        Err(AccessError::BadSignature)
    ));
}

#[tokio::test]
async fn unsupported_algorithm() {
    let signer = Signer::new("k1");
    let jwt = signer.sign(&claims());
    let (_, rest) = jwt.split_once('.').unwrap();
    let header = URL_SAFE_NO_PAD.encode(json!({ "alg": "none", "kid": "k1" }).to_string());

    let keys = FixedKeys::default();
    let access = Access::with_fetcher(config(false, true), &keys);
    assert!(matches!(
        access
            .email_at(&headers(None, Some(&format!("{header}.{rest}"))), now())
            .await,
        Err(AccessError::UnsupportedAlgorithm)
    ));
}

#[tokio::test]
async fn header_and_jwt_emails() {
    let signer = Signer::new("k1");
    let keys = FixedKeys::default();
    keys.keys.lock().unwrap().push(signer.jwk());
    let access = Access::with_fetcher(config(true, true), &keys);
    let jwt = signer.sign(&claims());

    assert_eq!(
        access
            .email_at(&headers(Some(" USER@example.com"), Some(&jwt)), now())
            .await
            .unwrap(),
        "user@example.com"
    );

    assert!(matches!(
        access
            .email_at(&headers(Some("other@example.com"), Some(&jwt)), now())
            .await,
        Err(AccessError::EmailMismatch)
    ));

    assert!(matches!(
        access.email_at(&headers(None, Some(&jwt)), now()).await,
        Err(AccessError::MissingEmail)
    ));

    assert!(matches!(
        access
            .email_at(&headers(Some("user@example.com"), None), now())
            .await,
        Err(AccessError::MissingJwt)
    ));
}

#[tokio::test]
async fn header_only() {
    let keys = FixedKeys::default();
    let access = Access::with_fetcher(config(true, false), &keys);

    assert_eq!(
        access
            .email_at(&headers(Some("User@Example.com"), None), now())
            .await
            .unwrap(),
        "user@example.com"
    );
    assert_eq!(keys.fetches.load(Ordering::SeqCst), 0);

    let access = Access::with_fetcher(config(false, false), &keys);
    assert!(matches!(
        access
            .email_at(&headers(Some("user@example.com"), None), now())
            .await,
        Err(AccessError::Disabled)
    ));
}

#[tokio::test]
async fn jwt_from_cookie() {
    let signer = Signer::new("k1");
    let keys = FixedKeys::default();
    keys.keys.lock().unwrap().push(signer.jwk());
    let access = Access::with_fetcher(config(false, true), &keys);

    let mut headers = HeaderMap::new();
    let cookie = format!("other=1; {JWT_COOKIE}={}", signer.sign(&claims()));
    headers.insert(COOKIE, HeaderValue::from_str(&cookie).unwrap());

    assert_eq!(
        access.email_at(&headers, now()).await.unwrap(),
        "user@example.com"
    );
}

#[tokio::test]
async fn caches_and_refetches_on_unknown_kid() {
    let old = Signer::new("old");
    let new = Signer::new("new");
    let keys = FixedKeys::default();
    keys.keys.lock().unwrap().push(old.jwk());
    let access = Access::with_fetcher(config(false, true), &keys);

    let jwt = old.sign(&claims());
    for _ in 0..2 {
        access
            .email_at(&headers(None, Some(&jwt)), now())
            .await
            .unwrap();
    }
    assert_eq!(keys.fetches.load(Ordering::SeqCst), 1);

    // Rotation: an unknown kid is refetched, but not more than once per interval.
    keys.keys.lock().unwrap().push(new.jwk());
    access.cache.lock().unwrap().fetched_at = None;
    let jwt = new.sign(&claims());
    access
        .email_at(&headers(None, Some(&jwt)), now())
        .await
        .unwrap();
    assert_eq!(keys.fetches.load(Ordering::SeqCst), 2);

    let unknown = Signer::new("unknown").sign(&claims());
    assert!(matches!(
        access.email_at(&headers(None, Some(&unknown)), now()).await,
        Err(AccessError::UnknownKey)
    ));
    assert_eq!(keys.fetches.load(Ordering::SeqCst), 2);
}

#[test]
fn parses_key_set() {
    let body = br#"{
        "keys": [
            { "kid": "a", "kty": "RSA", "alg": "RS256", "use": "sig", "e": "AQAB", "n": "AKs" },
            { "kid": "b", "kty": "EC", "crv": "P-256", "x": "AA", "y": "AA" }
        ],
        "public_cert": { "kid": "a", "cert": "" }
    }"#;

    let keys = parse_jwks(body).unwrap();
    assert_eq!(keys.len(), 1);
    assert_eq!(keys[0].kid, "a");
    assert_eq!(keys[0].e, [1, 0, 1]);
    assert_eq!(keys[0].n, [0, 0xab]);
}
