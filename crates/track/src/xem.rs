//! The XEM API (thexem.info): numbering maps and alternative names per show,
//! addressed by an origin (`tvdb`, `anidb`, ...) and the show's id there.

use std::collections::{BTreeMap, HashMap, HashSet};
use std::sync::Arc;
use std::time::{Duration, Instant};

use anyhow::{Context as _, Result};
use parking_lot::Mutex;
use serde::Deserialize;
use sqll::Row;

const BASE: &str = "https://thexem.info";

/// XEM's responses are cached for an hour, so a havemap list is not asked for
/// more often than that.
const HAVEMAP_TTL: Duration = Duration::from_secs(3600);

/// One system's address of a map/all entry. `part` is 1 for a double
/// episode's second address (XEM's `tvdb_2`), otherwise 0.
#[derive(Debug, Clone, PartialEq, Eq, Row)]
pub(crate) struct Numbering {
    pub(crate) system: String,
    pub(crate) part: u32,
    pub(crate) season: u32,
    pub(crate) episode: u32,
    pub(crate) absolute: Option<u32>,
}

/// An alternative name for the show (`season` is `None`) or one of its seasons.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Row)]
pub(crate) struct Name {
    pub(crate) season: Option<u32>,
    pub(crate) language: Option<String>,
    pub(crate) name: String,
}

/// Where a show's episode `season`/`episode` sits in XEM: TheTVDB's same code
/// when TheTVDB provides the episodes (`base`) or the show has no manual
/// `numbering`, otherwise the range covering it, if any.
#[allow(dead_code, reason = "the numbering display reads it")]
pub(crate) fn link_target(
    base: Option<api::RemoteSource>,
    numbering: Option<&api::Numbering>,
    season: u32,
    episode: u32,
) -> Option<api::NumberingTarget> {
    match numbering {
        Some(n) if base != Some(api::RemoteSource::Tvdb) => n.target(season, episode),
        _ => Some(api::NumberingTarget {
            system: "tvdb".to_owned(),
            season,
            episode,
        }),
    }
}

/// The answer to one conditional request, with the `Last-Modified` it carried.
pub(crate) struct Fetched<T> {
    pub(crate) last_modified: Option<String>,
    pub(crate) body: Body<T>,
}

pub(crate) enum Body<T> {
    /// `304 Not Modified`.
    NotModified,
    /// XEM answered `{"result":"failure"}`: it has nothing for this id.
    Failure,
    Success(T),
}

struct HaveMap {
    checked: Instant,
    last_modified: Option<String>,
    ids: Arc<HashSet<String>>,
}

#[derive(Clone)]
pub(crate) struct Client {
    http: reqwest::Client,
    base: Arc<str>,
    havemaps: Arc<Mutex<HashMap<String, HaveMap>>>,
}

impl Client {
    pub(crate) fn new(http: reqwest::Client) -> Self {
        Self {
            http,
            base: BASE.into(),
            havemaps: Arc::default(),
        }
    }

    #[cfg(test)]
    pub(crate) fn with_base(http: reqwest::Client, base: &str) -> Self {
        Self {
            base: base.trim_end_matches('/').into(),
            ..Self::new(http)
        }
    }

    /// Whether XEM maps the show with `id` in `origin`.
    pub(crate) async fn has_map(&self, origin: &str, id: &str) -> Result<bool> {
        let since = {
            let havemaps = self.havemaps.lock();

            match havemaps.get(origin) {
                Some(map) if map.checked.elapsed() < HAVEMAP_TTL => {
                    return Ok(map.ids.contains(id));
                }
                Some(map) => map.last_modified.clone(),
                None => None,
            }
        };

        let fetched = self
            .get::<Vec<String>>("map/havemap", &[("origin", origin)], since.as_deref())
            .await?;

        let mut havemaps = self.havemaps.lock();

        let ids = match fetched.body {
            Body::NotModified => match havemaps.get(origin) {
                Some(map) => map.ids.clone(),
                None => Arc::default(),
            },
            Body::Failure => Arc::default(),
            Body::Success(ids) => Arc::new(ids.into_iter().collect()),
        };

        let found = ids.contains(id);

        havemaps.insert(
            origin.to_owned(),
            HaveMap {
                checked: Instant::now(),
                last_modified: fetched.last_modified,
                ids,
            },
        );

        Ok(found)
    }

    /// The show's numbering map: one list of addresses per episode entry.
    pub(crate) async fn map_all(
        &self,
        origin: &str,
        id: &str,
        since: Option<&str>,
    ) -> Result<Fetched<Vec<Vec<Numbering>>>> {
        let fetched = self
            .get::<Vec<BTreeMap<String, RawNumbering>>>(
                "map/all",
                &[("id", id), ("origin", origin)],
                since,
            )
            .await?;

        Ok(fetched.map(|entries| entries.into_iter().map(entry).collect()))
    }

    /// The show's alternative names, deduplicated and sorted.
    pub(crate) async fn names(
        &self,
        origin: &str,
        id: &str,
        since: Option<&str>,
    ) -> Result<Fetched<Vec<Name>>> {
        let fetched = self
            .get::<RawNames>("map/names", &[("id", id), ("origin", origin)], since)
            .await?;

        Ok(fetched.map(names))
    }

    async fn get<T>(
        &self,
        path: &str,
        query: &[(&str, &str)],
        since: Option<&str>,
    ) -> Result<Fetched<T>>
    where
        T: for<'de> Deserialize<'de>,
    {
        let mut req = self.http.get(format!("{}/{path}", self.base)).query(query);

        if let Some(since) = since {
            req = req.header(reqwest::header::IF_MODIFIED_SINCE, since);
        }

        let res = req.send().await.context("Sending request")?;

        let last_modified = res
            .headers()
            .get(reqwest::header::LAST_MODIFIED)
            .and_then(|v| v.to_str().ok())
            .map(str::to_owned);

        if res.status() == reqwest::StatusCode::NOT_MODIFIED {
            return Ok(Fetched {
                last_modified,
                body: Body::NotModified,
            });
        }

        let res = res.error_for_status().context("Bad response status")?;
        let bytes = res.bytes().await.context("Reading response body")?;

        Ok(Fetched {
            last_modified,
            body: parse(&bytes).with_context(|| format!("Parsing XEM {path}"))?,
        })
    }
}

impl<T> Fetched<T> {
    fn map<U>(self, f: impl FnOnce(T) -> U) -> Fetched<U> {
        Fetched {
            last_modified: self.last_modified,
            body: match self.body {
                Body::NotModified => Body::NotModified,
                Body::Failure => Body::Failure,
                Body::Success(value) => Body::Success(f(value)),
            },
        }
    }
}

/// The later of two HTTP dates. XEM's cache answers `304` to any
/// `If-Modified-Since` at or after a response's `Last-Modified`, so one stored
/// date serves both of a show's requests.
pub(crate) fn latest(a: Option<String>, b: Option<String>) -> Option<String> {
    let parse = |s: &str| jiff::fmt::rfc2822::parse(s).ok().map(|z| z.timestamp());

    match (a, b) {
        (Some(a), Some(b)) => match (parse(&a), parse(&b)) {
            (Some(x), Some(y)) if y > x => Some(b),
            (Some(_), _) => Some(a),
            (None, _) => Some(b),
        },
        (a, b) => a.or(b),
    }
}

/// XEM's envelope: `{"result": "success" | "failure", "data": ..., "message": ...}`.
fn parse<T>(bytes: &[u8]) -> Result<Body<T>>
where
    T: for<'de> Deserialize<'de>,
{
    #[derive(Deserialize)]
    struct Envelope {
        result: String,
        #[serde(default)]
        data: serde_json::Value,
        #[serde(default)]
        message: String,
    }

    let envelope: Envelope = serde_json::from_slice(bytes)?;

    match envelope.result.as_str() {
        "success" => Ok(Body::Success(serde_json::from_value(envelope.data)?)),
        "failure" => Ok(Body::Failure),
        other => anyhow::bail!("Unexpected result {other:?}: {}", envelope.message),
    }
}

#[derive(Deserialize)]
struct RawNumbering {
    season: u32,
    episode: u32,
    #[serde(default)]
    absolute: Option<u32>,
}

/// A system key with a numeric suffix (`tvdb_2`) is the second address of a
/// double episode in that system.
fn entry(raw: BTreeMap<String, RawNumbering>) -> Vec<Numbering> {
    raw.into_iter()
        .map(|(key, n)| {
            let (system, part) = match key.rsplit_once('_') {
                Some((system, suffix)) => match suffix.parse::<u32>() {
                    Ok(n) if n >= 2 => (system.to_owned(), n - 1),
                    _ => (key, 0),
                },
                None => (key, 0),
            };

            Numbering {
                system,
                part,
                season: n.season,
                episode: n.episode,
                absolute: n.absolute,
            }
        })
        .collect()
}

/// map/names data: `{"<season>" | "all": {"<lang>": [names] | name}}`, or an
/// empty list when there are none.
#[derive(Deserialize)]
#[serde(untagged)]
enum RawNames {
    Map(BTreeMap<String, BTreeMap<String, OneOrMany>>),
    Empty(#[allow(dead_code)] Vec<serde_json::Value>),
}

#[derive(Deserialize)]
#[serde(untagged)]
enum OneOrMany {
    One(String),
    Many(Vec<String>),
}

fn names(raw: RawNames) -> Vec<Name> {
    let RawNames::Map(seasons) = raw else {
        return Vec::new();
    };

    let mut out = Vec::new();

    for (season, languages) in seasons {
        let season = match season.as_str() {
            "all" => None,
            s => match s.parse() {
                Ok(n) => Some(n),
                Err(_) => continue,
            },
        };

        for (language, names) in languages {
            let names = match names {
                OneOrMany::One(name) => vec![name],
                OneOrMany::Many(names) => names,
            };

            let language = (!language.is_empty()).then_some(language);

            for name in names {
                let name = name.trim();

                if !name.is_empty() {
                    out.push(Name {
                        season,
                        language: language.clone(),
                        name: name.to_owned(),
                    });
                }
            }
        }
    }

    out.sort();
    out.dedup();
    out
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    pub(crate) const FRIEREN_ALL: &str = include_str!("xem/fixtures/all-424536.json");
    pub(crate) const OSHI_NO_KO_ALL: &str = include_str!("xem/fixtures/all-421069.json");
    pub(crate) const OSHI_NO_KO_NAMES: &str = include_str!("xem/fixtures/names-421069.json");
    pub(crate) const FAILURE: &str = include_str!("xem/fixtures/failure.json");

    fn success<T>(body: Body<T>) -> T {
        match body {
            Body::Success(value) => value,
            _ => panic!("expected success"),
        }
    }

    #[test]
    fn parses_map_all() {
        let raw =
            success(parse::<Vec<BTreeMap<String, RawNumbering>>>(FRIEREN_ALL.as_bytes()).unwrap());
        let entries = raw.into_iter().map(entry).collect::<Vec<_>>();
        assert_eq!(entries.len(), 38);

        // TMDB's S1E29 is TheTVDB's S2E1.
        let tvdb = |e: &Vec<Numbering>| {
            e.iter()
                .find(|n| n.system == "tvdb")
                .map(|n| (n.season, n.episode, n.absolute))
        };
        assert_eq!(tvdb(&entries[0]), Some((1, 1, Some(1))));
        assert_eq!(tvdb(&entries[28]), Some((2, 1, Some(29))));
    }

    #[test]
    fn double_episodes_are_a_second_part() {
        let json = r#"{"result":"success","data":[{"scene":{"season":1,"episode":3,"absolute":3},"tvdb":{"season":1,"episode":3,"absolute":3},"tvdb_2":{"season":1,"episode":4,"absolute":4}}],"message":""}"#;
        let raw = success(parse::<Vec<BTreeMap<String, RawNumbering>>>(json.as_bytes()).unwrap());
        let entries = raw.into_iter().map(entry).collect::<Vec<_>>();

        let parts = entries[0]
            .iter()
            .map(|n| (n.system.as_str(), n.part, n.episode))
            .collect::<Vec<_>>();
        assert_eq!(parts, [("scene", 0, 3), ("tvdb", 0, 3), ("tvdb", 1, 4)]);
    }

    #[test]
    fn parses_names() {
        let names = names(success(
            parse::<RawNames>(OSHI_NO_KO_NAMES.as_bytes()).unwrap(),
        ));
        let name = |season: Option<u32>, language: &str, name: &str| Name {
            season,
            language: Some(language.to_owned()),
            name: name.to_owned(),
        };

        assert_eq!(
            names,
            [
                name(None, "de", "Oshi no Ko Mein Star"),
                name(None, "us", "Oshi no Ko (My Star)"),
                name(Some(1), "jp", "Oshi no Ko"),
                name(Some(2), "jp", "Oshi no Ko 2nd Season"),
                name(Some(3), "jp", "Oshi no Ko 3rd Season"),
                name(Some(3), "jp", "Oshi no Ko S3"),
            ]
        );
    }

    #[test]
    fn empty_names_and_failure() {
        let empty = r#"{"result":"success","data":[],"message":""}"#;
        assert!(names(success(parse::<RawNames>(empty.as_bytes()).unwrap())).is_empty());
        assert!(matches!(
            parse::<RawNames>(FAILURE.as_bytes()).unwrap(),
            Body::Failure
        ));
    }

    fn frieren() -> api::Numbering {
        let range = |first, last, target_season| api::NumberingRange {
            season: 1,
            first,
            last,
            system: "tvdb".to_owned(),
            target_season,
            target_first: 1,
        };

        api::Numbering {
            ranges: vec![range(1, 28, 1), range(29, 38, 2)],
        }
    }

    fn target(
        base: Option<api::RemoteSource>,
        numbering: Option<&api::Numbering>,
        season: u32,
        episode: u32,
    ) -> Option<(String, u32, u32)> {
        link_target(base, numbering, season, episode).map(|t| (t.system, t.season, t.episode))
    }

    #[test]
    fn manual_ranges_link_frieren_to_its_tvdb_seasons() {
        let tmdb = Some(api::RemoteSource::Tmdb);
        let n = frieren();
        let tvdb = |s, e| Some(("tvdb".to_owned(), s, e));

        assert_eq!(target(tmdb, Some(&n), 1, 1), tvdb(1, 1));
        assert_eq!(target(tmdb, Some(&n), 1, 28), tvdb(1, 28));
        assert_eq!(target(tmdb, Some(&n), 1, 29), tvdb(2, 1));
        assert_eq!(target(tmdb, Some(&n), 1, 38), tvdb(2, 10));

        // Outside every range there is no target, specials included.
        assert_eq!(target(tmdb, Some(&n), 1, 39), None);
        assert_eq!(target(tmdb, Some(&n), 0, 1), None);

        // Every target is an episode XEM maps.
        let all =
            success(parse::<Vec<BTreeMap<String, RawNumbering>>>(FRIEREN_ALL.as_bytes()).unwrap());
        let mapped: Vec<_> = all
            .into_iter()
            .flat_map(entry)
            .filter(|n| n.system == "tvdb")
            .map(|n| (n.season, n.episode))
            .collect();

        for e in 1..=38 {
            let (_, s, e) = target(tmdb, Some(&n), 1, e).unwrap();
            assert!(mapped.contains(&(s, e)), "S{s}E{e} is not in XEM");
        }
    }

    #[test]
    fn automatic_and_tvdb_base_are_identity() {
        let n = frieren();
        let tvdb = Some(("tvdb".to_owned(), 1, 29));

        assert_eq!(target(Some(api::RemoteSource::Tmdb), None, 1, 29), tvdb);
        assert_eq!(target(None, None, 1, 29), tvdb);
        assert_eq!(target(Some(api::RemoteSource::Tvdb), Some(&n), 1, 29), tvdb);
    }

    #[test]
    fn latest_picks_the_later_date() {
        let a = Some("Thu, 08 Oct 2026 00:07:43 GMT".to_owned());
        let b = Some("Thu, 08 Oct 2026 00:37:57 GMT".to_owned());
        assert_eq!(latest(a.clone(), b.clone()), b);
        assert_eq!(latest(b.clone(), a.clone()), b);
        assert_eq!(latest(None, a.clone()), a);
        assert_eq!(latest(None, None), None);
    }
}
