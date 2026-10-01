//! Identifying users behind Cloudflare Access.
//!
//! Access forwards the authenticated user's email in a header and a signed
//! JWT (RS256) in a header and the `CF_Authorization` cookie. The JWT is
//! verified against the team's signing keys published at
//! `https://<team_domain>/cdn-cgi/access/certs`.

use std::collections::HashMap;
use std::error::Error as StdError;
use std::sync::Mutex;
use std::time::{Duration, Instant};

use aws_lc_rs::rsa::PublicKeyComponents;
use aws_lc_rs::signature::RSA_PKCS1_2048_8192_SHA256;
use base64::Engine as _;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use cookie::Cookie;
use http::HeaderMap;
use http::header::COOKIE;
use jiff::Timestamp;
use serde::Deserialize;

use crate::normalize_email;

pub const EMAIL_HEADER: &str = "cf-access-authenticated-user-email";
pub const JWT_HEADER: &str = "cf-access-jwt-assertion";
pub const JWT_COOKIE: &str = "CF_Authorization";

/// Tolerated clock skew when checking `exp` and `nbf`.
const LEEWAY_SECONDS: i64 = 60;

/// Unknown key ids trigger a refetch at most this often, so forged tokens
/// cannot make every request fetch the keys.
const MIN_REFETCH_INTERVAL: Duration = Duration::from_secs(60);

#[derive(Debug, Clone)]
pub struct Config {
    /// The team's host, such as `example.cloudflareaccess.com`.
    pub team_domain: String,
    /// The Access application's audience (AUD) tag.
    pub audience: String,
    /// Accept the email header that Access sets.
    pub trust_email_header: bool,
    /// Require and verify the Access JWT.
    pub verify_jwt: bool,
}

#[derive(Debug, thiserror::Error)]
pub enum AccessError {
    #[error("neither the email header nor the JWT is enabled")]
    Disabled,
    #[error("missing {EMAIL_HEADER} header")]
    MissingEmail,
    #[error("missing Access JWT")]
    MissingJwt,
    #[error("malformed Access JWT")]
    Malformed,
    #[error("unsupported JWT algorithm")]
    UnsupportedAlgorithm,
    #[error("unknown JWT signing key")]
    UnknownKey,
    #[error("invalid JWT signature")]
    BadSignature,
    #[error("JWT audience does not match")]
    WrongAudience,
    #[error("JWT issuer does not match")]
    WrongIssuer,
    #[error("JWT has expired")]
    Expired,
    #[error("JWT is not yet valid")]
    NotYetValid,
    #[error("JWT has no email")]
    MissingJwtEmail,
    #[error("email header does not match the JWT")]
    EmailMismatch,
    #[error("failed to fetch Access signing keys")]
    Fetch(#[source] Box<dyn StdError + Send + Sync>),
}

/// An RSA signing key, with `n` and `e` as big-endian bytes.
#[derive(Debug, Clone)]
pub struct Jwk {
    pub kid: String,
    pub n: Vec<u8>,
    pub e: Vec<u8>,
}

/// Fetches a team's current signing keys.
pub trait KeyFetcher: Send + Sync {
    fn fetch(
        &self,
        team_domain: &str,
    ) -> impl Future<Output = Result<Vec<Jwk>, Box<dyn StdError + Send + Sync>>> + Send;
}

/// Fetches signing keys from Cloudflare over HTTPS.
#[derive(Debug, Clone, Default)]
pub struct HttpKeyFetcher {
    client: reqwest::Client,
}

impl HttpKeyFetcher {
    pub fn new(client: reqwest::Client) -> Self {
        Self { client }
    }
}

impl KeyFetcher for HttpKeyFetcher {
    async fn fetch(&self, team_domain: &str) -> Result<Vec<Jwk>, Box<dyn StdError + Send + Sync>> {
        let url = format!("https://{team_domain}/cdn-cgi/access/certs");
        let body = self
            .client
            .get(url)
            .send()
            .await?
            .error_for_status()?
            .bytes()
            .await?;
        Ok(parse_jwks(&body)?)
    }
}

#[derive(Debug, thiserror::Error)]
pub enum JwksError {
    #[error("malformed key set")]
    Json(#[from] serde_json::Error),
    #[error("malformed key component")]
    Base64(#[from] base64::DecodeError),
}

/// Parses a JSON Web Key Set, keeping its RSA keys.
pub fn parse_jwks(body: &[u8]) -> Result<Vec<Jwk>, JwksError> {
    #[derive(Deserialize)]
    struct Set {
        keys: Vec<Key>,
    }

    #[derive(Deserialize)]
    struct Key {
        kty: String,
        kid: Option<String>,
        n: Option<String>,
        e: Option<String>,
    }

    let set: Set = serde_json::from_slice(body)?;
    let mut keys = Vec::new();

    for key in set.keys {
        let (Some(kid), Some(n), Some(e)) = (key.kid, key.n, key.e) else {
            continue;
        };

        if key.kty != "RSA" {
            continue;
        }

        keys.push(Jwk {
            kid,
            n: URL_SAFE_NO_PAD.decode(n)?,
            e: URL_SAFE_NO_PAD.decode(e)?,
        });
    }

    Ok(keys)
}

#[derive(Default)]
struct KeyCache {
    keys: HashMap<String, PublicKeyComponents<Vec<u8>>>,
    fetched_at: Option<Instant>,
}

/// Identifies the user of a request that came through Cloudflare Access.
pub struct Access<F = HttpKeyFetcher> {
    config: Config,
    fetcher: F,
    cache: Mutex<KeyCache>,
}

impl Access<HttpKeyFetcher> {
    pub fn new(config: Config) -> Self {
        Self::with_fetcher(config, HttpKeyFetcher::default())
    }
}

impl<F> Access<F>
where
    F: KeyFetcher,
{
    pub fn with_fetcher(config: Config, fetcher: F) -> Self {
        Self {
            config,
            fetcher,
            cache: Mutex::new(KeyCache::default()),
        }
    }

    /// Returns the normalized email of the request's user.
    ///
    /// With both sources enabled, the header's email must match the JWT's.
    pub async fn email(&self, headers: &HeaderMap) -> Result<String, AccessError> {
        self.email_at(headers, Timestamp::now()).await
    }

    async fn email_at(&self, headers: &HeaderMap, now: Timestamp) -> Result<String, AccessError> {
        let header_email = if self.config.trust_email_header {
            let email = headers
                .get(EMAIL_HEADER)
                .and_then(|v| v.to_str().ok())
                .map(normalize_email)
                .filter(|e| !e.is_empty())
                .ok_or(AccessError::MissingEmail)?;
            Some(email)
        } else {
            None
        };

        if !self.config.verify_jwt {
            return header_email.ok_or(AccessError::Disabled);
        }

        let token = jwt_from_headers(headers).ok_or(AccessError::MissingJwt)?;
        let jwt_email = self.verify_jwt(&token, now).await?;

        if let Some(header_email) = header_email
            && header_email != jwt_email
        {
            return Err(AccessError::EmailMismatch);
        }

        Ok(jwt_email)
    }

    /// Verifies an Access JWT and returns its normalized email.
    async fn verify_jwt(&self, token: &str, now: Timestamp) -> Result<String, AccessError> {
        #[derive(Deserialize)]
        struct Header {
            alg: String,
            kid: String,
        }

        #[derive(Deserialize)]
        #[serde(untagged)]
        enum Audience {
            One(String),
            Many(Vec<String>),
        }

        #[derive(Deserialize)]
        struct Claims {
            aud: Audience,
            iss: String,
            exp: i64,
            nbf: Option<i64>,
            email: Option<String>,
        }

        let mut parts = token.split('.');

        let (Some(header), Some(claims), Some(signature), None) =
            (parts.next(), parts.next(), parts.next(), parts.next())
        else {
            return Err(AccessError::Malformed);
        };

        let signed = &token[..header.len() + 1 + claims.len()];
        let header: Header = decode_part(header)?;

        if header.alg != "RS256" {
            return Err(AccessError::UnsupportedAlgorithm);
        }

        let signature = URL_SAFE_NO_PAD
            .decode(signature)
            .map_err(|_| AccessError::Malformed)?;
        let key = self.key(&header.kid).await?;

        key.verify(&RSA_PKCS1_2048_8192_SHA256, signed.as_bytes(), &signature)
            .map_err(|_| AccessError::BadSignature)?;

        let claims: Claims = decode_part(claims)?;

        let audience_matches = match &claims.aud {
            Audience::One(aud) => *aud == self.config.audience,
            Audience::Many(auds) => auds.contains(&self.config.audience),
        };

        if !audience_matches {
            return Err(AccessError::WrongAudience);
        }

        if claims.iss != format!("https://{}", self.config.team_domain) {
            return Err(AccessError::WrongIssuer);
        }

        let now = now.as_second();

        if claims.exp + LEEWAY_SECONDS <= now {
            return Err(AccessError::Expired);
        }

        if claims.nbf.is_some_and(|nbf| nbf - LEEWAY_SECONDS > now) {
            return Err(AccessError::NotYetValid);
        }

        let email = claims
            .email
            .as_deref()
            .map(normalize_email)
            .filter(|e| !e.is_empty())
            .ok_or(AccessError::MissingJwtEmail)?;

        Ok(email)
    }

    /// Looks up a signing key, refetching the key set when `kid` is unknown.
    async fn key(&self, kid: &str) -> Result<PublicKeyComponents<Vec<u8>>, AccessError> {
        {
            let cache = self.cache.lock().unwrap();

            if let Some(key) = cache.keys.get(kid) {
                return Ok(key.clone());
            }

            if cache
                .fetched_at
                .is_some_and(|at| at.elapsed() < MIN_REFETCH_INTERVAL)
            {
                return Err(AccessError::UnknownKey);
            }
        }

        let keys = self
            .fetcher
            .fetch(&self.config.team_domain)
            .await
            .map_err(AccessError::Fetch)?;

        let mut cache = self.cache.lock().unwrap();
        cache.fetched_at = Some(Instant::now());
        cache.keys = keys
            .into_iter()
            .map(|k| {
                (
                    k.kid,
                    PublicKeyComponents {
                        n: strip_leading_zeros(k.n),
                        e: strip_leading_zeros(k.e),
                    },
                )
            })
            .collect();

        cache.keys.get(kid).cloned().ok_or(AccessError::UnknownKey)
    }
}

/// Key components must be minimal big-endian integers.
fn strip_leading_zeros(mut bytes: Vec<u8>) -> Vec<u8> {
    let zeros = bytes.iter().take_while(|&&b| b == 0).count();
    bytes.drain(..zeros);
    bytes
}

fn decode_part<T>(part: &str) -> Result<T, AccessError>
where
    T: for<'de> Deserialize<'de>,
{
    let bytes = URL_SAFE_NO_PAD
        .decode(part)
        .map_err(|_| AccessError::Malformed)?;
    serde_json::from_slice(&bytes).map_err(|_| AccessError::Malformed)
}

fn jwt_from_headers(headers: &HeaderMap) -> Option<String> {
    if let Some(token) = headers.get(JWT_HEADER).and_then(|v| v.to_str().ok()) {
        return Some(token.to_owned());
    }

    headers
        .get_all(COOKIE)
        .iter()
        .filter_map(|v| v.to_str().ok())
        .flat_map(Cookie::split_parse)
        .filter_map(Result::ok)
        .find(|c| c.name() == JWT_COOKIE)
        .map(|c| c.value().to_owned())
}

#[cfg(test)]
mod tests;
