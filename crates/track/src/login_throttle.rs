//! Limits failed sign-ins per client address and per login, in memory.

use std::collections::HashMap;
use std::net::IpAddr;
use std::time::{Duration, Instant};

use axum::http::HeaderMap;
use parking_lot::Mutex;

/// Failures count for this long after the first one.
const WINDOW: Duration = Duration::from_secs(15 * 60);
const MAX_PER_ADDRESS: u32 = 30;
const MAX_PER_LOGIN: u32 = 10;

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
enum Key {
    Address(IpAddr),
    Login(String),
}

struct Failures {
    count: u32,
    since: Instant,
}

#[derive(Default)]
pub(crate) struct LoginThrottle {
    /// Peers whose forwarded client address headers are believed.
    trusted_proxies: Vec<IpAddr>,
    failures: Mutex<HashMap<Key, Failures>>,
}

impl LoginThrottle {
    pub(crate) fn new(trusted_proxies: &[IpAddr]) -> Self {
        Self {
            trusted_proxies: trusted_proxies.iter().map(|ip| ip.to_canonical()).collect(),
            failures: Mutex::default(),
        }
    }

    /// The address a request from `peer` is counted against: the forwarded
    /// client address when `peer` is a trusted proxy, otherwise `peer` itself,
    /// since anyone else can send these headers.
    pub(crate) fn client_address(&self, peer: IpAddr, headers: &HeaderMap) -> IpAddr {
        if !self.trusted_proxies.contains(&peer.to_canonical()) {
            return peer;
        }

        let parse = |value: &str| value.trim().parse::<IpAddr>().ok();

        let cloudflare = headers
            .get("cf-connecting-ip")
            .and_then(|v| v.to_str().ok())
            .and_then(parse);

        // The rightmost entry is the one the trusted proxy appended; entries
        // to its left come from the client.
        let forwarded = || {
            headers
                .get_all("x-forwarded-for")
                .iter()
                .next_back()
                .and_then(|v| v.to_str().ok())
                .and_then(|v| v.rsplit(',').next())
                .and_then(parse)
        };

        cloudflare.or_else(forwarded).unwrap_or(peer)
    }

    /// Whether a sign-in from `address` as `login` may be attempted.
    pub(crate) fn allows(&self, address: IpAddr, login: &str, now: Instant) -> bool {
        let failures = self.failures.lock();

        let over = |key: Key, max: u32| {
            failures
                .get(&key)
                .is_some_and(|f| now.duration_since(f.since) < WINDOW && f.count >= max)
        };

        !over(address_key(address), MAX_PER_ADDRESS) && !over(login_key(login), MAX_PER_LOGIN)
    }

    pub(crate) fn fail(&self, address: IpAddr, login: &str, now: Instant) {
        let mut failures = self.failures.lock();
        failures.retain(|_, f| now.duration_since(f.since) < WINDOW);

        for key in [address_key(address), login_key(login)] {
            failures
                .entry(key)
                .or_insert(Failures {
                    count: 0,
                    since: now,
                })
                .count += 1;
        }
    }

    /// Forgets the failures of a login that signed in.
    pub(crate) fn succeed(&self, login: &str) {
        self.failures.lock().remove(&login_key(login));
    }
}

/// IPv6 clients usually hold a whole /64, so they count as one.
fn address_key(address: IpAddr) -> Key {
    match address.to_canonical() {
        IpAddr::V6(v6) => Key::Address(IpAddr::V6((u128::from(v6) & (!0u128 << 64)).into())),
        v4 => Key::Address(v4),
    }
}

fn login_key(login: &str) -> Key {
    Key::Login(auth::normalize_email(login))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn limits_logins_and_addresses() {
        let throttle = LoginThrottle::default();
        let now = Instant::now();
        let a: IpAddr = "192.0.2.1".parse().unwrap();
        let b: IpAddr = "192.0.2.2".parse().unwrap();

        for _ in 0..MAX_PER_LOGIN {
            assert!(throttle.allows(a, "root", now));
            throttle.fail(a, "Root ", now);
        }

        assert!(!throttle.allows(a, "root", now));
        assert!(!throttle.allows(b, "ROOT", now));
        assert!(throttle.allows(a, "alice", now));
        assert!(throttle.allows(a, "root", now + WINDOW));

        throttle.succeed("root");
        assert!(throttle.allows(b, "root", now));

        for i in MAX_PER_LOGIN..MAX_PER_ADDRESS {
            throttle.fail(a, &format!("user{i}"), now);
        }

        assert!(!throttle.allows(a, "bob", now));
        assert!(throttle.allows(b, "bob", now));
    }

    #[test]
    fn ipv6_counts_per_64() {
        let throttle = LoginThrottle::default();
        let now = Instant::now();
        let a: IpAddr = "2001:db8::1".parse().unwrap();
        let b: IpAddr = "2001:db8::ffff".parse().unwrap();
        let c: IpAddr = "2001:db8:0:1::1".parse().unwrap();

        for i in 0..MAX_PER_ADDRESS {
            throttle.fail(if i % 2 == 0 { a } else { b }, &format!("user{i}"), now);
        }

        assert!(!throttle.allows(a, "x", now));
        assert!(!throttle.allows(b, "x", now));
        assert!(throttle.allows(c, "x", now));
    }

    fn headers(pairs: &[(&'static str, &'static str)]) -> HeaderMap {
        let mut headers = HeaderMap::new();

        for (name, value) in pairs {
            headers.append(*name, value.parse().unwrap());
        }

        headers
    }

    #[test]
    fn untrusted_peer_headers_are_ignored() {
        let peer: IpAddr = "192.0.2.1".parse().unwrap();
        let spoofed = headers(&[
            ("cf-connecting-ip", "198.51.100.7"),
            ("x-forwarded-for", "198.51.100.8"),
        ]);

        let throttle = LoginThrottle::default();
        assert_eq!(throttle.client_address(peer, &spoofed), peer);

        let throttle = LoginThrottle::new(&["192.0.2.9".parse().unwrap()]);
        assert_eq!(throttle.client_address(peer, &spoofed), peer);
    }

    #[test]
    fn trusted_proxy_forwards_client_address() {
        let proxy: IpAddr = "192.0.2.1".parse().unwrap();
        let throttle = LoginThrottle::new(&[proxy]);
        let ip = |s: &str| s.parse::<IpAddr>().unwrap();

        let cf = headers(&[
            ("cf-connecting-ip", " 198.51.100.7 "),
            ("x-forwarded-for", "198.51.100.8"),
        ]);
        assert_eq!(throttle.client_address(proxy, &cf), ip("198.51.100.7"));

        let xff = headers(&[
            ("x-forwarded-for", "203.0.113.1, 198.51.100.8"),
            ("x-forwarded-for", "203.0.113.2, 2001:db8::1"),
        ]);
        assert_eq!(throttle.client_address(proxy, &xff), ip("2001:db8::1"));

        let invalid_cf = headers(&[
            ("cf-connecting-ip", "nonsense"),
            ("x-forwarded-for", "198.51.100.8"),
        ]);
        assert_eq!(
            throttle.client_address(proxy, &invalid_cf),
            ip("198.51.100.8")
        );

        let invalid = headers(&[("x-forwarded-for", "198.51.100.8, nonsense")]);
        assert_eq!(throttle.client_address(proxy, &invalid), proxy);
        assert_eq!(throttle.client_address(proxy, &HeaderMap::new()), proxy);

        // A proxy connecting over IPv6 with a mapped IPv4 address still matches.
        let mapped: IpAddr = "::ffff:192.0.2.1".parse().unwrap();
        assert_eq!(throttle.client_address(mapped, &cf), ip("198.51.100.7"));
    }
}
