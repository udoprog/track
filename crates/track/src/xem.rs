//! The XEM API (thexem.info): numbering maps and alternative names per show,
//! addressed by an origin (`tvdb`, `anidb`, ...) and the show's id there.

use std::collections::{BTreeMap, HashMap, HashSet};
use std::hash::Hash;
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

/// The seasons of other numberings the show's `season` is linked to: the
/// targets of its ranges in episode order, or TheTVDB's same season.
pub(crate) fn season_targets(
    base: Option<api::RemoteSource>,
    numbering: Option<&api::Numbering>,
    season: u32,
) -> Vec<api::LinkedSeason> {
    let Some(n) = numbering.filter(|_| base != Some(api::RemoteSource::Tvdb)) else {
        return vec![api::LinkedSeason {
            system: "tvdb".to_owned(),
            season,
        }];
    };

    let mut ranges = n
        .ranges
        .iter()
        .filter(|r| r.season == season)
        .collect::<Vec<_>>();
    ranges.sort_by_key(|r| r.first);

    let mut out = Vec::<api::LinkedSeason>::new();

    for r in ranges {
        let target = api::LinkedSeason {
            system: r.system.clone(),
            season: r.target_season,
        };

        if !out.contains(&target) {
            out.push(target);
        }
    }

    out
}

/// The system XEM numbers a show's season names by: the origin of its XEM
/// remote (`<origin>/<id>`), or TheTVDB without one.
pub(crate) fn origin(remotes: &[api::RemoteEntry]) -> String {
    remotes
        .iter()
        .filter(|e| *e.remote.source() == api::RemoteSource::Xem)
        .find_map(|e| Some(e.remote.value().as_str()?.split_once('/')?.0.to_owned()))
        .unwrap_or_else(|| "tvdb".to_owned())
}

/// The other numberings of the episode `season`/`episode`, read from every
/// system's address of its XEM `entry`: the shown systems of `prefs` in their
/// order, leaving out a code equal to the episode's own.
pub(crate) fn alternatives(
    prefs: &[api::NumberingPref],
    season: u32,
    episode: u32,
    entry: &[Numbering],
) -> Vec<api::AltNumbering> {
    prefs
        .iter()
        .filter(|p| p.shown)
        .filter_map(|p| {
            let first = entry.iter().find(|n| n.system == p.system && n.part == 0)?;

            // A double episode's second address, when it is in the same season.
            let last = entry
                .iter()
                .find(|n| n.system == p.system && n.part == 1 && n.season == first.season)
                .map(|n| n.episode);

            if last.is_none() && (first.season, first.episode) == (season, episode) {
                return None;
            }

            Some(api::AltNumbering {
                system: p.system.clone(),
                season: first.season,
                episode: first.episode,
                last,
                absolute: first.absolute,
            })
        })
        .collect()
}

/// The names in `names` for any of `seasons` (`None` for the whole show),
/// each name once, ignoring case, and none of `titles`.
pub(crate) fn names_for<'a>(
    names: &[Name],
    seasons: &[Option<u32>],
    titles: impl IntoIterator<Item = &'a str>,
) -> Vec<api::AltName> {
    let mut seen = titles
        .into_iter()
        .map(str::to_lowercase)
        .collect::<HashSet<_>>();

    names
        .iter()
        .filter(|n| seasons.contains(&n.season))
        .filter(|n| seen.insert(n.name.to_lowercase()))
        .map(|n| api::AltName {
            name: n.name.clone(),
            language: n.language.clone(),
        })
        .collect()
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

/// A page of XEM's website, held as long as a havemap list. A failed fetch is
/// held as empty, so it is not retried sooner either.
struct Page<T> {
    checked: Instant,
    last_modified: Option<String>,
    value: Arc<T>,
}

/// The most show pages fetched to confirm one show's XEM id.
const MAX_CANDIDATES: usize = 3;

#[derive(Clone)]
pub(crate) struct Client {
    http: reqwest::Client,
    base: Arc<str>,
    havemaps: Arc<Mutex<HashMap<String, HaveMap>>>,
    shows: Arc<Mutex<HashMap<(), Page<Vec<(u32, String)>>>>>,
    show_pages: Arc<Mutex<HashMap<u32, Page<Vec<(String, String)>>>>>,
}

impl Client {
    pub(crate) fn new(http: reqwest::Client) -> Self {
        Self {
            http,
            base: BASE.into(),
            havemaps: Arc::default(),
            shows: Arc::default(),
            show_pages: Arc::default(),
        }
    }

    /// XEM's own id for the show it maps as `id` in `origin`, which only its
    /// website has. The show list is searched for `names`, and a candidate is
    /// taken only when its page links `origin`/`id`. Any failure is `None`.
    pub(crate) async fn show_id<'a>(
        &self,
        origin: &str,
        id: &str,
        names: impl IntoIterator<Item = &'a str>,
    ) -> Option<u32> {
        let shows = self.cached(&self.shows, (), "xem/shows", parse_shows).await;

        for xem_id in candidates(&shows, names) {
            let path = format!("xem/show/{xem_id}");
            let links = self
                .cached(&self.show_pages, xem_id, &path, parse_show_links)
                .await;

            if links.iter().any(|(o, i)| o == origin && i == id) {
                return Some(xem_id);
            }
        }

        None
    }

    /// The page at `path`, parsed and held in `cache` under `key`.
    async fn cached<K, T>(
        &self,
        cache: &Mutex<HashMap<K, Page<T>>>,
        key: K,
        path: &str,
        parse: impl FnOnce(&str) -> T,
    ) -> Arc<T>
    where
        K: Eq + Hash,
        T: Default,
    {
        let since = match cache.lock().get(&key) {
            Some(page) if page.checked.elapsed() < HAVEMAP_TTL => return page.value.clone(),
            Some(page) => page.last_modified.clone(),
            None => None,
        };

        let fetched = self.get_html(path, since.as_deref()).await;

        let mut cache = cache.lock();

        let (value, last_modified) = match fetched {
            Ok((Some(html), last_modified)) => (Arc::new(parse(&html)), last_modified),
            Ok((None, last_modified)) => (
                cache.get(&key).map(|p| p.value.clone()).unwrap_or_default(),
                last_modified,
            ),
            Err(e) => {
                tracing::warn!(path, "Fetching an XEM page: {e:#}");
                (Arc::default(), None)
            }
        };

        cache.insert(
            key,
            Page {
                checked: Instant::now(),
                last_modified,
                value: value.clone(),
            },
        );

        value
    }

    /// A page of XEM's website, or `None` for `304 Not Modified`.
    async fn get_html(
        &self,
        path: &str,
        since: Option<&str>,
    ) -> Result<(Option<String>, Option<String>)> {
        let mut req = self.http.get(format!("{}/{path}", self.base));

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
            return Ok((None, last_modified));
        }

        let res = res.error_for_status().context("Bad response status")?;
        let text = res.text().await.context("Reading response body")?;
        Ok((Some(text), last_modified))
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

/// The shows of XEM's list page (`/xem/shows`): each `<a href="/xem/show/<id>">`
/// with its name.
fn parse_shows(html: &str) -> Vec<(u32, String)> {
    const LINK: &str = "<a href=\"/xem/show/";

    let mut out = Vec::new();

    for rest in html.split(LINK).skip(1) {
        let Some((id, rest)) = rest.split_once("\">") else {
            continue;
        };

        let (Ok(id), Some((name, _))) = (id.parse(), rest.split_once("</a>")) else {
            continue;
        };

        out.push((id, decode_html(name.trim())));
    }

    out
}

/// The `(origin, id)` pairs a show page links: TheTVDB and AniDB, per season.
fn parse_show_links(html: &str) -> Vec<(String, String)> {
    const LINKS: &[(&str, &str)] = &[
        ("tvdb", "thetvdb.com/?tab=series&id="),
        ("anidb", "animedb.pl?show=anime&aid="),
    ];

    let html = html.replace("&amp;", "&");
    let mut out = Vec::new();

    for &(origin, prefix) in LINKS {
        for rest in html.split(prefix).skip(1) {
            let end = rest
                .find(|c: char| !c.is_ascii_digit())
                .unwrap_or(rest.len());
            let link = (origin.to_owned(), rest[..end].to_owned());

            if end > 0 && !out.contains(&link) {
                out.push(link);
            }
        }
    }

    out
}

fn decode_html(s: &str) -> String {
    s.replace("&quot;", "\"")
        .replace("&#039;", "'")
        .replace("&#39;", "'")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&amp;", "&")
}

/// A name as lowercase words, ignoring punctuation and apostrophes.
fn words(name: &str) -> Vec<String> {
    name.replace(['\'', '\u{2019}'], "")
        .split(|c: char| !c.is_alphanumeric())
        .filter(|w| !w.is_empty())
        .map(str::to_lowercase)
        .collect()
}

/// The XEM ids of `shows` whose name is one of `names`, then those whose name
/// starts one of them (XEM's `.hack//` for `.hack//Sign`), longest first; at
/// most [`MAX_CANDIDATES`].
fn candidates<'a>(shows: &[(u32, String)], names: impl IntoIterator<Item = &'a str>) -> Vec<u32> {
    let names = names
        .into_iter()
        .map(words)
        .filter(|w| !w.is_empty())
        .collect::<Vec<_>>();

    let mut found = shows
        .iter()
        .filter_map(|(id, name)| {
            let show = words(name);

            if show.is_empty() {
                return None;
            }

            let exact = names.iter().any(|n| *n == show);

            if !exact && !names.iter().any(|n| n.starts_with(&show)) {
                return None;
            }

            Some((!exact, std::cmp::Reverse(show.len()), *id))
        })
        .collect::<Vec<_>>();

    found.sort();

    let mut out = Vec::new();

    for (_, _, id) in found {
        if !out.contains(&id) {
            out.push(id);
        }

        if out.len() == MAX_CANDIDATES {
            break;
        }
    }

    out
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
    pub(crate) const SHOWS: &str = include_str!("xem/fixtures/shows.html");
    pub(crate) const SHOW_4162: &str = include_str!("xem/fixtures/show-4162.html");

    #[test]
    fn parses_the_show_list() {
        let shows = parse_shows(SHOWS);

        // The navigation's show picker has no links and is left out.
        assert_eq!(shows.len(), 7);
        assert_eq!(shows[0], (4162, ".hack//".to_owned()));
        assert!(shows.contains(&(6743, "Frieren: Beyond Journey's End".to_owned())));
        assert!(shows.contains(&(
            4420,
            "100 Sleeping Princes & The Kingdom of Dreams".to_owned()
        )));
    }

    #[test]
    fn parses_the_links_of_a_show_page() {
        let link = |origin: &str, id: &str| (origin.to_owned(), id.to_owned());

        assert_eq!(
            parse_show_links(SHOW_4162),
            [
                link("tvdb", "79099"),
                link("anidb", "24"),
                link("anidb", "447"),
                link("anidb", "4324"),
            ]
        );
    }

    #[test]
    fn candidates_match_names_then_prefixes() {
        let shows = parse_shows(SHOWS);

        assert_eq!(
            candidates(&shows, ["Frieren: Beyond Journey's End"]),
            [6743]
        );
        assert_eq!(
            candidates(&shows, ["Frieren: Beyond Journey\u{2019}s End"]),
            [6743]
        );
        assert_eq!(candidates(&shows, [".hack//Sign"]), [4162]);
        assert_eq!(candidates(&shows, ["Oshi no Ko (My Star)"]), [6744]);
        assert!(candidates(&shows, ["Show", ""]).is_empty());

        // An exact name comes before a shorter prefix.
        let shows = [
            (1, "Oshi".to_owned()),
            (2, "Oshi no Ko".to_owned()),
            (3, "Oshi no Ko (My Star)".to_owned()),
        ];
        assert_eq!(candidates(&shows, ["Oshi no Ko"]), [2, 1]);
        assert_eq!(candidates(&shows, ["Oshi no Ko: My Star"]), [3, 2, 1]);
    }

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

    fn n(system: &str, part: u32, season: u32, episode: u32, absolute: u32) -> Numbering {
        Numbering {
            system: system.to_owned(),
            part,
            season,
            episode,
            absolute: Some(absolute),
        }
    }

    fn codes(alts: &[api::AltNumbering]) -> Vec<(&str, String)> {
        alts.iter().map(|a| (a.system.as_str(), a.code())).collect()
    }

    #[test]
    fn alternatives_follow_the_order_and_leave_out_the_own_code() {
        let prefs = api::default_numberings();
        let all =
            success(parse::<Vec<BTreeMap<String, RawNumbering>>>(FRIEREN_ALL.as_bytes()).unwrap());
        let entries = all.into_iter().map(entry).collect::<Vec<_>>();

        // TMDB's S1E29 is S2E1 everywhere else.
        let alts = alternatives(&prefs, 1, 29, &entries[28]);
        assert_eq!(
            codes(&alts),
            [
                ("tvdb", "S02E01".to_owned()),
                ("scene", "S02E01".to_owned()),
                ("anidb", "S02E01".to_owned()),
            ]
        );
        assert_eq!(alts[0].absolute, Some(29));

        // S1E28 lines up, so it shows nothing.
        assert!(alternatives(&prefs, 1, 28, &entries[27]).is_empty());

        // Hidden systems are left out and the order is the setting's.
        let prefs = api::numbering_order(&[
            api::NumberingPref {
                system: "anidb".to_owned(),
                shown: true,
            },
            api::NumberingPref {
                system: "tvdb".to_owned(),
                shown: false,
            },
        ]);
        assert_eq!(
            codes(&alternatives(&prefs, 1, 29, &entries[28])),
            [
                ("anidb", "S02E01".to_owned()),
                ("scene", "S02E01".to_owned()),
            ]
        );
    }

    #[test]
    fn a_double_episode_is_one_code() {
        let prefs = api::default_numberings();
        let entry = [
            n("scene", 0, 1, 3, 3),
            n("tvdb", 0, 1, 3, 3),
            n("tvdb", 1, 1, 4, 4),
        ];

        // Its first half equals the episode's own code, but the pair does not.
        assert_eq!(
            codes(&alternatives(&prefs, 1, 3, &entry)),
            [("tvdb", "S01E03+04".to_owned())]
        );
    }

    #[test]
    fn season_targets_follow_the_ranges() {
        let tmdb = Some(api::RemoteSource::Tmdb);
        let n = frieren();
        let linked = |season| api::LinkedSeason {
            system: "tvdb".to_owned(),
            season,
        };

        assert_eq!(season_targets(tmdb, Some(&n), 1), [linked(1), linked(2)]);
        assert!(season_targets(tmdb, Some(&n), 2).is_empty());
        assert_eq!(season_targets(tmdb, None, 2), [linked(2)]);
        assert_eq!(
            season_targets(Some(api::RemoteSource::Tvdb), Some(&n), 1),
            [linked(1)]
        );
    }

    #[test]
    fn origin_is_read_from_the_xem_remote() {
        let entry = |remote| api::RemoteEntry {
            id: api::RemoteId::new(1),
            slug: None,
            remote,
            enabled: true,
            priority: 0,
            sync_kinds: None,
            cache: None,
        };

        let xem = api::Remote::new(
            api::RemoteSource::Xem,
            api::RemoteValue::Str("anidb/17617".into()),
        );

        assert_eq!(origin(&[entry(api::Remote::tvdb(1)), entry(xem)]), "anidb");
        assert_eq!(origin(&[entry(api::Remote::tvdb(1))]), "tvdb");
    }

    #[test]
    fn names_leave_out_titles_and_repeats() {
        let names = names(success(
            parse::<RawNames>(OSHI_NO_KO_NAMES.as_bytes()).unwrap(),
        ));

        let show = names_for(&names, &[None], ["oshi no ko mein star"]);
        assert_eq!(show.len(), 1);
        assert_eq!(show[0].name, "Oshi no Ko (My Star)");
        assert_eq!(show[0].language.as_deref(), Some("us"));

        let season = names_for(&names, &[Some(3)], []);
        let season = season.iter().map(|n| n.name.as_str()).collect::<Vec<_>>();
        assert_eq!(season, ["Oshi no Ko 3rd Season", "Oshi no Ko S3"]);
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
