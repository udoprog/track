use core::fmt;
use core::num::NonZero;
use std::collections::BTreeSet;

use musli_core::{Decode, Encode};
use musli_web::api::{self, ChannelId};

#[cfg(feature = "yew")]
use implicit_clone::unsync::IString;
#[cfg(feature = "yew")]
use yew::html::IntoPropValue;
#[cfg(feature = "yew")]
use yew::virtual_dom::VNode;

#[cfg(test)]
mod tests;

mod macros;

mod language;

pub use self::language::{Language, ParseLanguageErr};

mod country;
pub use self::country::{Country, ParseCountryErr};

mod locale;
pub use self::locale::{Locale, ParseLocaleErr};

mod translations;
pub use self::translations::Translations;

mod sync_kind;
pub use self::sync_kind::{SyncKind, SyncKindSet};

mod time;
pub use self::time::{
    Date, HumanDate, HumanDateTime, TimeInfo, TimeOfDay, TimeZone, Timestamp, Weekday,
};

mod duration;
pub use self::duration::{Duration, DurationUnit, HumanDuration, ParseDurationUnitErr};

macros::define_id!(ShowId);
macros::define_id!(SeasonId);
macros::define_id!(EpisodeId);
macros::define_id!(MovieId);
macros::define_id!(WatchedId);
macros::define_id!(TaskId);
macros::define_id!(ImageId);
macros::define_id!(PendingId);
macros::define_id!(RemoteId);
macros::define_id!(PersonId);
macros::define_id!(CreditId);

/// The source of a remote identifier.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Hash, Encode, Decode, serde::Serialize, serde::Deserialize,
)]
#[musli(crate = musli_core)]
#[serde(rename_all = "lowercase")]
pub enum RemoteSource {
    Tvdb,
    Tmdb,
    Imdb,
    Tvmaze,
    Unknown,
}

impl RemoteSource {
    /// All known remote sources, in arbitrary but deterministic order.
    pub const ALL: &[Self] = &[Self::Tvdb, Self::Tmdb, Self::Imdb, Self::Tvmaze];

    /// Whether this source is unknown.
    pub fn is_unknown(&self) -> bool {
        matches!(self, Self::Unknown)
    }

    pub fn as_label(&self) -> &'static str {
        match self {
            Self::Tvdb => "TheTVDB",
            Self::Tmdb => "TMDB",
            Self::Imdb => "IMDb",
            Self::Tvmaze => "TVmaze",
            Self::Unknown => "Unknown",
        }
    }

    pub fn as_id(&self) -> &'static str {
        match self {
            Self::Tvdb => "tvdb",
            Self::Tmdb => "tmdb",
            Self::Imdb => "imdb",
            Self::Tvmaze => "tvmaze",
            Self::Unknown => "unknown",
        }
    }

    pub fn from_id(s: &str) -> Self {
        match s {
            "tvdb" => Self::Tvdb,
            "tmdb" => Self::Tmdb,
            "imdb" => Self::Imdb,
            "tvmaze" => Self::Tvmaze,
            _ => Self::Unknown,
        }
    }

    /// The kinds of data this source can contribute during a layered sync. See
    /// [`SyncKind`] for how a layered sync uses these.
    pub fn sync_kinds(&self) -> &'static [SyncKind] {
        match self {
            Self::Tmdb => &[SyncKind::Base, SyncKind::Dates, SyncKind::Credits],
            Self::Tvdb => &[SyncKind::Base, SyncKind::Dates],
            Self::Tvmaze => &[SyncKind::Dates],
            Self::Imdb | Self::Unknown => &[],
        }
    }

    /// Whether this source contributes accumulating graphics (show-level art such
    /// as posters, backdrops and banners) that merge across every enabled source.
    pub fn has_graphics(&self) -> bool {
        matches!(self, Self::Tmdb | Self::Tvdb)
    }

    /// The capability ceiling: every kind this source can possibly provide, as a
    /// set. Configured selections are always clamped to this.
    pub fn default_sync_kinds(&self) -> SyncKindSet {
        SyncKindSet::from_kinds(self.sync_kinds().iter().copied())
    }
}

/// A global, per-source selection of which [`SyncKind`]s that source contributes,
/// stored in [`Config::sync_kinds`].
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode, serde::Serialize, serde::Deserialize)]
#[musli(crate = musli_core)]
pub struct SourceSyncKinds {
    pub source: RemoteSource,
    pub kinds: SyncKindSet,
}

impl fmt::Display for RemoteSource {
    #[inline]
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_label())
    }
}

#[cfg(feature = "sqll")]
impl ::sqll::FromColumn<'_> for RemoteSource {
    type Type = ::sqll::ty::Integer;

    fn from_column(stmt: &::sqll::Statement, index: ::sqll::ty::Integer) -> ::sqll::Result<Self> {
        let s = u32::from_column(stmt, index)?;

        match s {
            1 => Ok(RemoteSource::Tvdb),
            2 => Ok(RemoteSource::Tmdb),
            3 => Ok(RemoteSource::Imdb),
            4 => Ok(RemoteSource::Tvmaze),
            _ => Ok(RemoteSource::Unknown),
        }
    }
}

#[cfg(feature = "sqll")]
impl ::sqll::BindValue for RemoteSource {
    fn bind_value(&self, stmt: &mut ::sqll::Statement, index: ::sqll::Index) -> ::sqll::Result<()> {
        let n: u32 = match self {
            RemoteSource::Unknown => 0,
            RemoteSource::Tvdb => 1,
            RemoteSource::Tmdb => 2,
            RemoteSource::Imdb => 3,
            RemoteSource::Tvmaze => 4,
        };

        n.bind_value(stmt, index)
    }
}

/// The value part of a remote identifier either an integer or a string.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode, serde::Serialize, serde::Deserialize)]
#[musli(crate = musli_core)]
#[serde(untagged)]
pub enum RemoteValue {
    Int(u32),
    Str(String),
}

impl RemoteValue {
    pub fn as_u32(&self) -> Option<u32> {
        match self {
            Self::Int(n) => Some(*n),
            Self::Str(_) => None,
        }
    }

    pub fn as_str(&self) -> Option<&str> {
        match self {
            Self::Str(s) => Some(s.as_str()),
            Self::Int(_) => None,
        }
    }

    fn parse(s: &str) -> Self {
        match s.parse::<u32>() {
            Ok(n) => Self::Int(n),
            Err(_) => Self::Str(s.to_owned()),
        }
    }
}

impl fmt::Display for RemoteValue {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Int(n) => write!(f, "{n}"),
            Self::Str(s) => f.write_str(s),
        }
    }
}

#[cfg(feature = "sqll")]
impl ::sqll::FromColumn<'_> for RemoteValue {
    type Type = ::sqll::ty::Any;

    fn from_column(stmt: &::sqll::Statement, index: ::sqll::ty::Any) -> ::sqll::Result<Self> {
        let value = ::sqll::Value::from_column(stmt, index)?;

        if let Some(n) = value.as_integer() {
            return Ok(RemoteValue::Int(n as u32));
        }

        if let Some(s) = value.as_text() {
            let s = s
                .to_str()
                .map_err(|e| ::sqll::Error::new(::sqll::Code::MISMATCH, e))?;
            return Ok(RemoteValue::Str(s.to_owned()));
        }

        Err(::sqll::Error::new(
            ::sqll::Code::MISMATCH,
            "remote value must be an integer or text",
        ))
    }
}

#[cfg(feature = "sqll")]
impl ::sqll::BindValue for RemoteValue {
    fn bind_value(&self, stmt: &mut ::sqll::Statement, index: ::sqll::Index) -> ::sqll::Result<()> {
        match self {
            RemoteValue::Int(n) => n.bind_value(stmt, index),
            RemoteValue::Str(s) => s.as_str().bind_value(stmt, index),
        }
    }
}

/// Remote identifier: "tvdb:123", "tmdb:456", "imdb:tt0001234".
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode, serde::Serialize, serde::Deserialize)]
#[musli(crate = musli_core)]
#[serde(from = "String", into = "String")]
pub struct Remote {
    source: RemoteSource,
    value: RemoteValue,
}

impl Remote {
    pub const fn new(source: RemoteSource, value: RemoteValue) -> Self {
        Self { source, value }
    }

    pub fn tvdb(id: u32) -> Self {
        Self {
            source: RemoteSource::Tvdb,
            value: RemoteValue::Int(id),
        }
    }

    pub fn tmdb(id: u32) -> Self {
        Self {
            source: RemoteSource::Tmdb,
            value: RemoteValue::Int(id),
        }
    }

    pub fn imdb(s: &str) -> Self {
        Self {
            source: RemoteSource::Imdb,
            value: RemoteValue::Str(s.to_owned()),
        }
    }

    pub fn tvmaze(id: u32) -> Self {
        Self {
            source: RemoteSource::Tvmaze,
            value: RemoteValue::Int(id),
        }
    }

    pub fn from_raw(s: &str) -> Self {
        match s.split_once(':') {
            Some((src, val)) => Self {
                source: RemoteSource::from_id(src),
                value: RemoteValue::parse(val),
            },
            None => Self {
                source: RemoteSource::Unknown,
                value: RemoteValue::Str(s.to_owned()),
            },
        }
    }

    pub fn source(&self) -> &RemoteSource {
        &self.source
    }

    pub fn value(&self) -> &RemoteValue {
        &self.value
    }

    pub fn show_url(&self, slug: Option<&str>) -> Option<String> {
        match (&self.source, slug) {
            (RemoteSource::Tvdb, Some(slug)) => Some(format!("https://thetvdb.com/series/{slug}")),
            (RemoteSource::Tvdb, None) => {
                Some(format!("https://thetvdb.com/search?query={}", self.value))
            }
            (RemoteSource::Tmdb, _) => {
                Some(format!("https://www.themoviedb.org/tv/{}", self.value))
            }
            (RemoteSource::Imdb, _) => Some(format!("https://www.imdb.com/title/{}/", self.value)),
            (RemoteSource::Tvmaze, _) => {
                Some(format!("https://www.tvmaze.com/shows/{}", self.value))
            }
            _ => None,
        }
    }

    pub fn movie_url(&self) -> Option<String> {
        match &self.source {
            RemoteSource::Tvdb => Some(format!("https://thetvdb.com/search?query={}", self.value)),
            RemoteSource::Tmdb => Some(format!("https://www.themoviedb.org/movie/{}", self.value)),
            RemoteSource::Imdb => Some(format!("https://www.imdb.com/title/{}/", self.value)),
            _ => None,
        }
    }

    pub fn person_url(&self, slug: Option<&str>) -> Option<String> {
        match &self.source {
            // TMDB's canonical person URL is `/person/{id}-{slug}`; the bare id also
            // resolves, so the slug is only appended when known.
            RemoteSource::Tmdb => Some(match slug {
                Some(slug) if !slug.is_empty() => {
                    format!("https://www.themoviedb.org/person/{}-{slug}", self.value)
                }
                _ => format!("https://www.themoviedb.org/person/{}", self.value),
            }),
            RemoteSource::Imdb => Some(format!("https://www.imdb.com/name/{}/", self.value)),
            _ => None,
        }
    }
}

impl fmt::Display for Remote {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}:{}", self.source, self.value)
    }
}

impl From<String> for Remote {
    fn from(s: String) -> Self {
        Self::from_raw(&s)
    }
}

impl From<Remote> for String {
    fn from(r: Remote) -> String {
        r.to_string()
    }
}

/// A stored remote belonging to a show or movie: its database identifier paired
/// with the logical value. The `id` lets the client reference a specific remote
/// (for editing or removal) without matching on its source/value.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
#[musli(crate = musli_core)]
pub struct RemoteEntry {
    pub id: RemoteId,
    pub slug: Option<String>,
    pub remote: Remote,
    /// Whether this remote contributes to merged data (air dates, and metadata sync).
    pub enabled: bool,
    /// Merge priority; lower numbers win. See [`enabled_sources_by_priority`].
    pub priority: i32,
    /// Per-remote override of which kinds this remote contributes; `None` inherits
    /// the global default for its source. See [`effective_remote_sync_kinds`].
    pub sync_kinds: Option<SyncKindSet>,
    /// Source-specific conditional-request state from the last sync, used to skip
    /// re-fetching an unchanged remote. `None` when never synced or unparsable.
    pub cache: Option<RemoteCache>,
}

/// Per-remote cache validators captured during sync and replayed on the next one
/// to detect "nothing changed". Stored as JSON in the `*_remotes.cache` column;
/// the fields are per-source and optional, so older or other-shaped payloads
/// deserialize tolerantly (missing → `None`) and new sources can add fields
/// without breaking existing rows.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode, serde::Serialize, serde::Deserialize)]
#[musli(crate = musli_core)]
pub struct RemoteCache {
    /// TMDB `ETag`, replayed via `If-None-Match` (a `304` means unchanged).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub etag: Option<String>,
    /// TVDB `lastUpdated`; an equal value on the extended record means unchanged.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_updated: Option<String>,
    /// The sync kinds that were actually fetched and persisted under this
    /// validator. A cache hit is only honored for a layer whose needed kinds are a
    /// subset of this set, so e.g. an air-date-only validator never short-circuits
    /// a later Base fetch (when a remote is re-prioritized). Defaults to empty for
    /// rows written before this field existed, forcing a one-time re-fetch.
    #[serde(default, skip_serializing_if = "SyncKindSet::is_empty")]
    pub kinds: SyncKindSet,
    /// Sub-requests that failed on the last sync, so they are not retried on every
    /// run. A remote that 404s (an episode a source simply doesn't carry) would
    /// otherwise cost one wasted call per sync forever.
    ///
    /// An entry suppresses its sub-request until it ages past
    /// [`RemoteErrorKind::ttl`], then it is retried. Empty for rows written before
    /// this field existed.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub errors: Vec<RemoteError>,
}

impl RemoteCache {
    /// The recorded failure for `key`, if any.
    pub fn error(&self, key: &str) -> Option<&RemoteError> {
        self.errors.iter().find(|e| e.key == key)
    }

    /// Whether any recorded failure is old enough to be retried. Such a cache must
    /// not short-circuit the layer (see `usable_cache`), or the sub-request that
    /// failed would never get another chance.
    pub fn has_expired_errors(&self, now: Timestamp) -> bool {
        self.errors.iter().any(|e| !e.is_live(now))
    }
}

/// Why a sub-request failed, which decides how long the failure is trusted.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Encode, Decode, serde::Serialize, serde::Deserialize,
)]
#[musli(crate = musli_core)]
#[serde(rename_all = "snake_case")]
pub enum RemoteErrorKind {
    /// The remote does not carry this entity (a `404`). A stable fact, so it is
    /// trusted for a long time rather than re-probed every sync.
    Missing,
    /// A transient failure - a `5xx`, a timeout, a malformed body. Trusted only
    /// briefly, so a blip does not freeze an entity's metadata for a day.
    Transient,
}

impl RemoteErrorKind {
    /// How long a failure of this kind suppresses its retry.
    pub fn ttl(&self) -> Duration {
        match self {
            Self::Missing => Duration::from_hours(24),
            Self::Transient => Duration::from_hours(1),
        }
    }

    pub fn as_label(&self) -> &'static str {
        match self {
            Self::Missing => "Missing",
            Self::Transient => "Transient",
        }
    }
}

/// A sub-request that failed during a sync, recorded on the owning remote's cache so
/// it can be suppressed until it expires - and shown to the user, so a silently
/// half-synced entity is visible rather than mysterious.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode, serde::Serialize, serde::Deserialize)]
#[musli(crate = musli_core)]
pub struct RemoteError {
    /// Stable identifier for the sub-request, so the next sync can tell whether
    /// *this* call failed before (e.g. `"season/3/episodes"`, `"episode/S02E05"`).
    pub key: String,
    /// The failure, rendered for display.
    pub message: String,
    pub kind: RemoteErrorKind,
    /// When it was recorded. Retried once `now` passes `at + kind.ttl()`.
    pub at: Timestamp,
}

impl RemoteError {
    /// Whether this failure is still trusted, i.e. its sub-request stays suppressed.
    pub fn is_live(&self, now: Timestamp) -> bool {
        now < self.at.saturating_add(self.kind.ttl())
    }

    /// When this failure stops being trusted and its sub-request is retried.
    pub fn expires_at(&self) -> Timestamp {
        self.at.saturating_add(self.kind.ttl())
    }
}

/// The kinds a remote actually contributes during a sync: its per-remote override
/// if set, otherwise the global default for its source ([`Config::sync_kinds_for`]),
/// in either case clamped to the source's capability.
pub fn effective_remote_sync_kinds(entry: &RemoteEntry, config: &Config) -> SyncKindSet {
    let source = *entry.remote.source();
    entry
        .sync_kinds
        .unwrap_or_else(|| config.sync_kinds_for(source))
        .intersect(source.default_sync_kinds())
}

/// The union of [`SyncKind`]s that at least one enabled remote is configured to
/// contribute. A kind absent from this set is excluded across every remote, so its
/// derived data should be cleared on sync rather than kept; a kind present here but
/// not produced during a given sync is a transient fetch failure, and the existing
/// data is kept. See [`air_date_sources_by_priority`] for the air-date-specific,
/// priority-ordered form of the same eligibility notion.
pub fn eligible_sync_kinds(remotes: &[RemoteEntry], config: &Config) -> SyncKindSet {
    let mut set = SyncKindSet::empty();

    for entry in remotes.iter().filter(|e| e.enabled) {
        for kind in effective_remote_sync_kinds(entry, config) {
            set.insert(kind);
        }
    }

    set
}

/// Sources of enabled remotes whose effective sync kinds include [`SyncKind::AirDate`],
/// ordered by priority (lowest number = highest priority), de-duplicated keeping the
/// highest-priority occurrence of each source. A source with AirDate disabled no longer
/// contributes air dates even if it has stale stored releases.
pub fn air_date_sources_by_priority(remotes: &[RemoteEntry], config: &Config) -> Vec<RemoteSource> {
    let mut entries: Vec<&RemoteEntry> = remotes
        .iter()
        .filter(|e| e.enabled && effective_remote_sync_kinds(e, config).contains(SyncKind::Dates))
        .collect();
    entries.sort_by_key(|e| e.priority);

    let mut out = Vec::new();

    for e in entries {
        let source = *e.remote.source();

        if !out.contains(&source) {
            out.push(source);
        }
    }

    out
}

/// Sources of the enabled remotes ordered by priority (lowest number = highest
/// priority), de-duplicated keeping the highest-priority occurrence of each source.
pub fn enabled_sources_by_priority(remotes: &[RemoteEntry]) -> Vec<RemoteSource> {
    let mut entries: Vec<&RemoteEntry> = remotes.iter().filter(|e| e.enabled).collect();
    entries.sort_by_key(|e| e.priority);

    let mut out = Vec::new();

    for e in entries {
        let source = *e.remote.source();

        if !out.contains(&source) {
            out.push(source);
        }
    }

    out
}

/// The remote source that drives full metadata sync: the highest-priority enabled
/// remote whose source supports full sync ([`RemoteSource::Tmdb`] or
/// [`RemoteSource::Tvdb`]).
pub fn primary_sync_source(remotes: &[RemoteEntry]) -> Option<RemoteSource> {
    remotes
        .iter()
        .filter(|e| {
            e.enabled && matches!(e.remote.source(), RemoteSource::Tmdb | RemoteSource::Tvdb)
        })
        .max_by_key(|e| e.priority)
        .map(|r| *r.remote.source())
}

/// Image reference: "tvdb:/banners/abc.jpg", "tmdb:/xy.jpg".
#[derive(Debug, Clone, PartialEq, Encode, Decode, serde::Serialize, serde::Deserialize)]
#[musli(crate = musli_core)]
pub struct ImageKey {
    source: ImageSource,
    path: String,
}

impl ImageKey {
    pub fn tmdb(path: impl AsRef<str>) -> Self {
        Self::new(ImageSource::Tmdb, path)
    }

    pub fn tvdb(path: impl AsRef<str>) -> Self {
        Self::new(ImageSource::Tvdb, path)
    }

    pub fn new(source: ImageSource, path: impl AsRef<str>) -> Self {
        Self {
            source,
            path: path.as_ref().trim_start_matches('/').to_owned(),
        }
    }

    pub fn source(&self) -> &ImageSource {
        &self.source
    }

    pub fn path(&self) -> &str {
        &self.path
    }
}

#[derive(Debug, PartialEq, Clone, Encode, Decode, serde::Serialize, serde::Deserialize)]
#[musli(crate = musli_core)]
pub struct Image {
    path: ImageKey,
    width: u32,
    height: u32,
}

impl Image {
    pub fn new(source: ImageSource, path: &str) -> Self {
        Self {
            path: ImageKey::new(source, path),
            width: 0,
            height: 0,
        }
    }

    pub fn new_with_dims(source: ImageSource, path: &str, width: u32, height: u32) -> Self {
        Self {
            path: ImageKey::new(source, path),
            width,
            height,
        }
    }

    pub fn key(&self) -> &ImageKey {
        &self.path
    }

    pub fn tvdb(path: &str) -> Self {
        Self {
            path: ImageKey::new(ImageSource::Tvdb, path),
            width: 0,
            height: 0,
        }
    }

    pub fn tmdb(path: &str) -> Self {
        Self {
            path: ImageKey::new(ImageSource::Tmdb, path),
            width: 0,
            height: 0,
        }
    }

    pub fn from_raw(s: impl AsRef<str>) -> Self {
        let s = s.as_ref();

        let path = match s.split_once(':') {
            Some((src, path)) => ImageKey::new(ImageSource::parse(src), path),
            None => ImageKey::new(ImageSource::Unknown, s),
        };

        Self {
            path,
            width: 0,
            height: 0,
        }
    }

    pub fn width(&self) -> u32 {
        self.width
    }

    pub fn height(&self) -> u32 {
        self.height
    }

    pub fn proxy_url(&self) -> String {
        format!("/api/image/{}/{}", self.path.source, self.path.path)
    }
}

impl fmt::Display for Image {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}:{}", self.path.source, self.path.path)
    }
}

impl From<ImageKey> for Image {
    #[inline]
    fn from(path: ImageKey) -> Self {
        Self {
            path,
            width: 0,
            height: 0,
        }
    }
}

/// Season number: Specials (stored as 0) or a regular numbered season.
#[derive(
    Clone,
    Copy,
    PartialEq,
    Eq,
    PartialOrd,
    Ord,
    Hash,
    Encode,
    Decode,
    serde::Serialize,
    serde::Deserialize,
)]
#[musli(crate = musli_core)]
#[serde(untagged)]
pub enum SeasonNumber {
    Specials,
    Number(NonZero<u32>),
}

impl SeasonNumber {
    /// The first regular season, i.e. `SeasonNumber::Number(1)`.
    pub const FIRST: Self = Self::from_ordinal(1);

    #[inline]
    pub const fn from_ordinal(n: u32) -> Self {
        if let Some(n) = NonZero::new(n) {
            Self::Number(n)
        } else {
            Self::Specials
        }
    }

    #[inline]
    pub fn short(&self) -> impl fmt::Display + '_ {
        fmt::from_fn(|f| match self {
            Self::Specials => write!(f, "S00"),
            Self::Number(n) => write!(f, "S{n:02}"),
        })
    }

    #[inline]
    pub fn long(&self) -> impl fmt::Display + '_ {
        fmt::from_fn(|f| match self {
            Self::Specials => write!(f, "Specials"),
            Self::Number(n) => write!(f, "Season {n}"),
        })
    }

    #[inline]
    pub fn ordinal(&self) -> impl fmt::Display + '_ {
        fmt::from_fn(|f| match self {
            Self::Specials => write!(f, "0"),
            Self::Number(n) => write!(f, "{}", n.get()),
        })
    }

    #[inline]
    pub fn is_special(&self) -> bool {
        matches!(self, SeasonNumber::Specials)
    }
}

impl Default for SeasonNumber {
    #[inline]
    fn default() -> Self {
        Self::FIRST
    }
}

impl fmt::Debug for SeasonNumber {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Specials => f.write_str("Specials"),
            Self::Number(n) => n.fmt(f),
        }
    }
}

#[cfg(feature = "sqll")]
impl ::sqll::FromColumn<'_> for SeasonNumber {
    type Type = ::sqll::ty::Integer;

    #[inline]
    fn from_column(stmt: &::sqll::Statement, index: ::sqll::ty::Integer) -> ::sqll::Result<Self> {
        let n = u32::from_column(stmt, index)?;
        Ok(SeasonNumber::from_ordinal(n))
    }
}

#[cfg(feature = "sqll")]
impl ::sqll::BindValue for SeasonNumber {
    #[inline]
    fn bind_value(&self, stmt: &mut ::sqll::Statement, index: ::sqll::Index) -> ::sqll::Result<()> {
        let n = match self {
            SeasonNumber::Specials => 0,
            SeasonNumber::Number(n) => n.get(),
        };

        n.bind_value(stmt, index)
    }
}

#[derive(
    Debug,
    Clone,
    Copy,
    PartialEq,
    Eq,
    Hash,
    PartialOrd,
    Ord,
    Encode,
    Decode,
    serde::Serialize,
    serde::Deserialize,
)]
#[musli(crate = musli_core)]
#[serde(rename_all = "lowercase")]
pub enum ImageKind {
    Poster,
    Banner,
    Backdrop,
    Screenshot,
    Profile,
    Unknown,
}

impl ImageKind {
    pub fn title(self) -> &'static str {
        match self {
            ImageKind::Poster => "Poster",
            ImageKind::Banner => "Banner",
            ImageKind::Backdrop => "Backdrop",
            ImageKind::Screenshot => "Screenshot",
            ImageKind::Profile => "Profile",
            ImageKind::Unknown => "Unknown",
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            ImageKind::Poster => "poster",
            ImageKind::Banner => "banner",
            ImageKind::Backdrop => "backdrop",
            ImageKind::Screenshot => "screenshot",
            ImageKind::Profile => "profile",
            ImageKind::Unknown => "unknown",
        }
    }
}

impl fmt::Display for ImageKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

#[cfg(feature = "sqll")]
impl ::sqll::FromColumn<'_> for ImageKind {
    type Type = ::sqll::ty::Integer;

    fn from_column(stmt: &::sqll::Statement, index: ::sqll::ty::Integer) -> ::sqll::Result<Self> {
        match u32::from_column(stmt, index)? {
            1 => Ok(ImageKind::Poster),
            2 => Ok(ImageKind::Banner),
            3 => Ok(ImageKind::Backdrop),
            4 => Ok(ImageKind::Screenshot),
            5 => Ok(ImageKind::Profile),
            _ => Ok(ImageKind::Unknown),
        }
    }
}

#[cfg(feature = "sqll")]
impl ::sqll::BindValue for ImageKind {
    fn bind_value(&self, stmt: &mut ::sqll::Statement, index: ::sqll::Index) -> ::sqll::Result<()> {
        let n: u32 = match self {
            ImageKind::Poster => 1,
            ImageKind::Banner => 2,
            ImageKind::Backdrop => 3,
            ImageKind::Screenshot => 4,
            ImageKind::Profile => 5,
            ImageKind::Unknown => 0,
        };

        n.bind_value(stmt, index)
    }
}

/// The kind of a translated string stored in a `*_strings` table. The owning
/// table supplies the context, so a show/movie `title` and an episode/season
/// `name` both use [`StringKind::Title`].
#[derive(
    Debug,
    Clone,
    Copy,
    PartialEq,
    Eq,
    Hash,
    PartialOrd,
    Ord,
    Encode,
    Decode,
    serde::Serialize,
    serde::Deserialize,
)]
#[musli(crate = musli_core)]
#[serde(rename_all = "lowercase")]
pub enum StringKind {
    Title,
    Overview,
    Character,
    Unknown,
}

impl StringKind {
    pub fn as_str(self) -> &'static str {
        match self {
            StringKind::Title => "title",
            StringKind::Overview => "overview",
            StringKind::Character => "character",
            StringKind::Unknown => "unknown",
        }
    }
}

impl fmt::Display for StringKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

#[cfg(feature = "sqll")]
impl ::sqll::FromColumn<'_> for StringKind {
    type Type = ::sqll::ty::Integer;

    fn from_column(stmt: &::sqll::Statement, index: ::sqll::ty::Integer) -> ::sqll::Result<Self> {
        match u32::from_column(stmt, index)? {
            1 => Ok(StringKind::Title),
            2 => Ok(StringKind::Overview),
            3 => Ok(StringKind::Character),
            _ => Ok(StringKind::Unknown),
        }
    }
}

#[cfg(feature = "sqll")]
impl ::sqll::BindValue for StringKind {
    fn bind_value(&self, stmt: &mut ::sqll::Statement, index: ::sqll::Index) -> ::sqll::Result<()> {
        let n: u32 = match self {
            StringKind::Title => 1,
            StringKind::Overview => 2,
            StringKind::Character => 3,
            StringKind::Unknown => 0,
        };

        n.bind_value(stmt, index)
    }
}

/// Whether a [`Credit`] is a cast (acting) or a crew role.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Hash, Encode, Decode, serde::Serialize, serde::Deserialize,
)]
#[musli(crate = musli_core)]
#[serde(rename_all = "lowercase")]
pub enum CreditKind {
    Cast,
    Crew,
}

#[cfg(feature = "sqll")]
impl ::sqll::FromColumn<'_> for CreditKind {
    type Type = ::sqll::ty::Integer;

    fn from_column(stmt: &::sqll::Statement, index: ::sqll::ty::Integer) -> ::sqll::Result<Self> {
        match u32::from_column(stmt, index)? {
            2 => Ok(CreditKind::Crew),
            _ => Ok(CreditKind::Cast),
        }
    }
}

#[cfg(feature = "sqll")]
impl ::sqll::BindValue for CreditKind {
    fn bind_value(&self, stmt: &mut ::sqll::Statement, index: ::sqll::Index) -> ::sqll::Result<()> {
        let n: u32 = match self {
            CreditKind::Cast => 1,
            CreditKind::Crew => 2,
        };

        n.bind_value(stmt, index)
    }
}

#[derive(
    Debug,
    PartialEq,
    Eq,
    Hash,
    PartialOrd,
    Ord,
    Clone,
    Copy,
    Encode,
    Decode,
    serde::Serialize,
    serde::Deserialize,
)]
#[musli(crate = musli_core)]
#[serde(rename_all = "lowercase")]
pub enum ImageSource {
    Tvdb,
    Tmdb,
    Unknown,
}

impl ImageSource {
    pub fn parse(s: &str) -> Self {
        match s {
            "tvdb" => Self::Tvdb,
            "tmdb" => Self::Tmdb,
            _ => Self::Unknown,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            ImageSource::Tvdb => "tvdb",
            ImageSource::Tmdb => "tmdb",
            ImageSource::Unknown => "unknown",
        }
    }
}

impl fmt::Display for ImageSource {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

#[cfg(feature = "sqll")]
impl ::sqll::FromColumn<'_> for ImageSource {
    type Type = ::sqll::ty::Integer;

    fn from_column(stmt: &::sqll::Statement, index: ::sqll::ty::Integer) -> ::sqll::Result<Self> {
        let s = u32::from_column(stmt, index)?;

        match s {
            1 => Ok(ImageSource::Tvdb),
            2 => Ok(ImageSource::Tmdb),
            _ => Ok(ImageSource::Unknown),
        }
    }
}

#[cfg(feature = "sqll")]
impl ::sqll::BindValue for ImageSource {
    fn bind_value(&self, stmt: &mut ::sqll::Statement, index: ::sqll::Index) -> ::sqll::Result<()> {
        let n: u32 = match self {
            ImageSource::Unknown => 0,
            ImageSource::Tvdb => 1,
            ImageSource::Tmdb => 2,
        };

        n.bind_value(stmt, index)
    }
}

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Default, Encode, Decode, serde::Serialize, serde::Deserialize,
)]
#[musli(crate = musli_core)]
#[serde(rename_all = "lowercase")]
pub enum ThemeType {
    #[default]
    Dark,
    Light,
    /// Follow the browser's `prefers-color-scheme`.
    System,
}

impl ThemeType {
    fn as_str(self) -> &'static str {
        match self {
            ThemeType::Dark => "dark",
            ThemeType::Light => "light",
            ThemeType::System => "system",
        }
    }
}

impl fmt::Display for ThemeType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

#[cfg(feature = "sqll")]
impl ::sqll::FromColumn<'_> for ThemeType {
    type Type = ::sqll::ty::Text;

    fn from_column(stmt: &::sqll::Statement, index: ::sqll::ty::Text) -> ::sqll::Result<Self> {
        let s = String::from_column(stmt, index)?;
        match s.as_str() {
            "dark" => Ok(ThemeType::Dark),
            "light" => Ok(ThemeType::Light),
            "system" => Ok(ThemeType::System),
            other => Err(::sqll::Error::custom(format!("unknown theme: {other}"))),
        }
    }
}

#[cfg(feature = "sqll")]
impl ::sqll::BindValue for ThemeType {
    fn bind_value(&self, stmt: &mut ::sqll::Statement, index: ::sqll::Index) -> ::sqll::Result<()> {
        self.as_str().bind_value(stmt, index)
    }
}

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Hash, Encode, Decode, serde::Serialize, serde::Deserialize,
)]
#[musli(crate = musli_core)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum ReleaseType {
    Unknown,
    Premiere,
    TheatricalLimited,
    Theatrical,
    Digital,
    Physical,
    Tv,
}

impl ReleaseType {
    pub fn as_u32(self) -> u32 {
        match self {
            Self::Unknown => 0,
            Self::Premiere => 1,
            Self::TheatricalLimited => 2,
            Self::Theatrical => 3,
            Self::Digital => 4,
            Self::Physical => 5,
            Self::Tv => 6,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Unknown => "Unknown",
            Self::Premiere => "Premiere",
            Self::TheatricalLimited => "Limited",
            Self::Theatrical => "Theatrical",
            Self::Digital => "Digital",
            Self::Physical => "Physical",
            Self::Tv => "TV",
        }
    }
}

#[cfg(feature = "sqll")]
impl ::sqll::FromColumn<'_> for ReleaseType {
    type Type = ::sqll::ty::Integer;

    #[inline]
    fn from_column(stmt: &::sqll::Statement, index: ::sqll::ty::Integer) -> ::sqll::Result<Self> {
        match u32::from_column(stmt, index)? {
            1 => Ok(Self::Premiere),
            2 => Ok(Self::TheatricalLimited),
            3 => Ok(Self::Theatrical),
            4 => Ok(Self::Digital),
            5 => Ok(Self::Physical),
            6 => Ok(Self::Tv),
            _ => Ok(Self::Unknown),
        }
    }
}

#[cfg(feature = "sqll")]
impl ::sqll::BindValue for ReleaseType {
    #[inline]
    fn bind_value(&self, stmt: &mut ::sqll::Statement, index: ::sqll::Index) -> ::sqll::Result<()> {
        self.as_u32().bind_value(stmt, index)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
#[musli(crate = musli_core)]
pub struct MovieRelease {
    pub source: RemoteSource,
    pub country: Country,
    pub release_type: ReleaseType,
    pub timestamp: Timestamp,
}

/// A single predicate within a [`FilterRule`]: a set of one kind, matched as an
/// OR within the set (the release's value must be in the set). The
/// `ReleaseTypes` variant applies to movie releases and `Networks` to episode
/// releases; the other two apply to both.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode, serde::Serialize, serde::Deserialize)]
#[musli(crate = musli_core)]
#[serde(rename_all = "snake_case")]
pub enum FilterPredicate {
    Sources(Vec<RemoteSource>),
    Countries(Vec<Country>),
    ReleaseTypes(Vec<ReleaseType>),
    Networks(Vec<String>),
}

impl FilterPredicate {
    /// Whether this predicate accepts a movie release.
    fn matches_movie(&self, release: &MovieRelease) -> bool {
        match self {
            Self::Sources(sources) => sources.contains(&release.source),
            Self::Countries(countries) => countries.contains(&release.country),
            Self::ReleaseTypes(types) => types.contains(&release.release_type),
            Self::Networks(_) => false,
        }
    }

    /// Whether this predicate accepts an episode release.
    fn matches_episode(&self, release: &EpisodeRelease) -> bool {
        match self {
            Self::Sources(sources) => sources.contains(&release.source),
            Self::Countries(countries) => countries.contains(&release.country),
            Self::ReleaseTypes(_) => false,
            Self::Networks(networks) => networks
                .iter()
                .any(|n| n.eq_ignore_ascii_case(&release.network)),
        }
    }

    /// Whether this predicate's set is empty (no values). An empty predicate
    /// matches nothing; the editor treats it as "no constraint of this kind".
    pub fn is_empty(&self) -> bool {
        match self {
            Self::Sources(v) => v.is_empty(),
            Self::Countries(v) => v.is_empty(),
            Self::ReleaseTypes(v) => v.is_empty(),
            Self::Networks(v) => v.is_empty(),
        }
    }

    /// The kind of this predicate, used by the editor to group and label.
    pub fn kind(&self) -> PredicateKind {
        match self {
            Self::Sources(_) => PredicateKind::Sources,
            Self::Countries(_) => PredicateKind::Countries,
            Self::ReleaseTypes(_) => PredicateKind::ReleaseTypes,
            Self::Networks(_) => PredicateKind::Networks,
        }
    }
}

/// The kind of a [`FilterPredicate`], independent of its set contents.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PredicateKind {
    Sources,
    Countries,
    ReleaseTypes,
    Networks,
}

impl PredicateKind {
    pub fn label(self) -> &'static str {
        match self {
            Self::Sources => "Sources",
            Self::Countries => "Countries",
            Self::ReleaseTypes => "Release types",
            Self::Networks => "Networks",
        }
    }

    /// An empty predicate of this kind, used when the editor adds one.
    pub fn empty(self) -> FilterPredicate {
        match self {
            Self::Sources => FilterPredicate::Sources(Vec::new()),
            Self::Countries => FilterPredicate::Countries(Vec::new()),
            Self::ReleaseTypes => FilterPredicate::ReleaseTypes(Vec::new()),
            Self::Networks => FilterPredicate::Networks(Vec::new()),
        }
    }
}

/// A rule: a conjunction (AND) of [`FilterPredicate`]s. An empty rule (no
/// predicates) matches every release. See [`FilterRules`] for how a collection
/// of rules is combined.
#[derive(
    Debug, Clone, Default, PartialEq, Eq, Encode, Decode, serde::Serialize, serde::Deserialize,
)]
#[musli(crate = musli_core)]
pub struct FilterRule {
    /// Optional human-readable label shown in the editor. Purely informational.
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub predicates: Vec<FilterPredicate>,
}

impl FilterRule {
    /// Whether all predicates accept the given movie release.
    pub fn matches_movie(&self, release: &MovieRelease) -> bool {
        self.predicates.iter().all(|p| p.matches_movie(release))
    }

    /// Whether all predicates accept the given episode release.
    pub fn matches_episode(&self, release: &EpisodeRelease) -> bool {
        self.predicates.iter().all(|p| p.matches_episode(release))
    }
}

/// A collection of [`FilterRule`]s applied together. A release is accepted only
/// when the list is non-empty and *every* rule matches (rules are AND'd); an
/// empty list accepts nothing.
#[derive(
    Debug, Clone, Default, PartialEq, Eq, Encode, Decode, serde::Serialize, serde::Deserialize,
)]
#[musli(crate = musli_core, transparent)]
#[serde(transparent)]
pub struct FilterRules {
    rules: Vec<FilterRule>,
}

impl FilterRules {
    /// The default set of release rules: a single rule accepting Digital,
    /// Physical and Tv releases (from any source/country).
    pub fn default_release_rules() -> Self {
        Self {
            rules: vec![FilterRule {
                name: String::new(),
                predicates: vec![FilterPredicate::ReleaseTypes(vec![
                    ReleaseType::Digital,
                    ReleaseType::Physical,
                    ReleaseType::Tv,
                ])],
            }],
        }
    }

    /// Whether a movie release is accepted: the list must be non-empty and every
    /// rule must match.
    pub fn release_accepted(&self, release: &MovieRelease) -> bool {
        !self.rules.is_empty() && self.rules.iter().all(|rule| rule.matches_movie(release))
    }

    /// The earliest timestamp among `releases` accepted by these rules.
    pub fn earliest_release(&self, releases: &[MovieRelease]) -> Option<Timestamp> {
        releases
            .iter()
            .filter(|r| self.release_accepted(r))
            .map(|r| r.timestamp)
            .min()
    }

    /// The effective air date for an episode: the earliest qualifying release
    /// from the highest-priority source. Only sources present in `priority` (the
    /// eligible, AirDate-enabled sources) contribute - a release from any other
    /// source is ignored, so excluding a source's air dates drops its dates
    /// entirely and an empty `priority` yields `None`. Among eligible releases,
    /// these rules decide which qualify. Returns `None` when nothing qualifies.
    pub fn effective_aired(
        &self,
        releases: &[EpisodeRelease],
        priority: &[RemoteSource],
    ) -> Option<Timestamp> {
        let mut merged = Prioritized::new();

        for r in releases
            .iter()
            .filter(|r| self.air_date_considered(r, priority))
        {
            merged.push(r.source, r.timestamp);
        }

        merged.best(priority).into_iter().min()
    }

    /// Whether `release` is considered for an episode's effective air date: its
    /// source must be present in `priority` (an eligible, AirDate-enabled source)
    /// and every rule must match (an empty list accepts nothing). Priority
    /// between sources is the media's remote order, not part of the rules.
    pub fn air_date_considered(&self, release: &EpisodeRelease, priority: &[RemoteSource]) -> bool {
        priority.contains(&release.source)
            && !self.rules.is_empty()
            && self.rules.iter().all(|rule| rule.matches_episode(release))
    }
}

impl core::ops::Deref for FilterRules {
    type Target = Vec<FilterRule>;

    fn deref(&self) -> &Self::Target {
        &self.rules
    }
}

impl core::ops::DerefMut for FilterRules {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.rules
    }
}

impl From<Vec<FilterRule>> for FilterRules {
    fn from(rules: Vec<FilterRule>) -> Self {
        Self { rules }
    }
}

impl FromIterator<FilterRule> for FilterRules {
    fn from_iter<I: IntoIterator<Item = FilterRule>>(iter: I) -> Self {
        Self {
            rules: iter.into_iter().collect(),
        }
    }
}

impl IntoIterator for FilterRules {
    type Item = FilterRule;
    type IntoIter = std::vec::IntoIter<FilterRule>;

    fn into_iter(self) -> Self::IntoIter {
        self.rules.into_iter()
    }
}

/// A candidate value contributed by a specific remote source.
pub struct Sourced<T> {
    pub source: RemoteSource,
    pub value: T,
}

/// Values contributed by several remotes, resolved against a priority order
/// (a slice of [`RemoteSource`], lowest index = highest priority). This is the
/// shared merge primitive: air dates use it now, image merging can reuse it.
pub struct Prioritized<T> {
    items: Vec<Sourced<T>>,
}

impl<T> Default for Prioritized<T> {
    fn default() -> Self {
        Self { items: Vec::new() }
    }
}

impl<T> Prioritized<T>
where
    T: Copy,
{
    pub fn new() -> Self {
        Self::default()
    }

    pub fn push(&mut self, source: RemoteSource, value: T) {
        self.items.push(Sourced { source, value });
    }

    /// Rank of a source in the priority order; sources absent from the order
    /// rank last (so they only contribute as a fallback).
    fn rank(priority: &[RemoteSource], source: RemoteSource) -> usize {
        priority
            .iter()
            .position(|s| *s == source)
            .unwrap_or(usize::MAX)
    }

    /// Values contributed by the single highest-priority source present.
    pub fn best(&self, priority: &[RemoteSource]) -> Vec<T> {
        let Some(min_rank) = self
            .items
            .iter()
            .map(|s| Self::rank(priority, s.source))
            .min()
        else {
            return Vec::new();
        };

        self.items
            .iter()
            .filter(|s| Self::rank(priority, s.source) == min_rank)
            .map(|s| s.value)
            .collect()
    }
}

/// A known air date for an episode, attributed to the remote `source` it came
/// from and optionally the `country`/`network` it aired on.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
#[musli(crate = musli_core)]
pub struct EpisodeRelease {
    pub source: RemoteSource,
    pub country: Country,
    pub network: String,
    pub timestamp: Timestamp,
}

/// Default air-date source priority: TVmaze (exact airtimes) over TMDB over TVDB.
pub fn default_air_date_priority() -> Vec<RemoteSource> {
    vec![RemoteSource::Tvmaze, RemoteSource::Tmdb, RemoteSource::Tvdb]
}

/// Resolve the configured [`Config::sync_languages`] against an entity's own
/// `original` locale into the concrete set of locales a sync should populate
/// strings for: each language-default entry takes `original`'s language,
/// concrete entries stay as-is, anything whose language is still unresolved is
/// dropped, and the `BTreeSet` deduplicates.
pub fn expand_sync_languages(sync_languages: &[Locale], original: Locale) -> BTreeSet<Locale> {
    sync_languages
        .iter()
        .map(|l| l.or(original))
        .filter(|l| !l.language().is_default())
        .collect()
}

#[derive(Debug, Clone, Encode, Decode)]
#[musli(crate = musli_core)]
pub struct MediaImage {
    pub id: ImageId,
    pub kind: ImageKind,
    pub source: ImageSource,
    pub image: Image,
    /// Raw remote score used to sort graphics within a single remote. Scores
    /// are not comparable across remotes and are absent for owners we don't
    /// score (seasons/episodes).
    pub score: Option<f64>,
}

/// A single cast or crew credit on a show or movie. `character` carries the
/// per-language character name (cast only, resolved via the display locale);
/// `department`/`job` are the English crew role (crew only).
#[derive(Debug, Clone, Encode, Decode)]
#[musli(crate = musli_core)]
pub struct Credit {
    pub person_id: PersonId,
    /// The person's localized name, resolved via the display locale.
    pub name: Translations,
    pub profile: Option<Image>,
    pub kind: CreditKind,
    pub character: Translations,
    pub department: Option<String>,
    pub job: Option<String>,
    pub episode_count: Option<u32>,
    pub order: Option<u32>,
}

/// A fully-loaded person, mirroring [`Show`]/[`Movie`]: localized name and
/// biography, the best profile image, and the person's own remotes (identity +
/// per-remote [`RemoteCache`] state) for the remote editor.
#[derive(Debug, Clone, Encode, Decode)]
#[musli(crate = musli_core)]
pub struct Person {
    pub id: PersonId,
    /// Localized name (kind=Title); resolve with [`Translations::title`].
    pub name: Translations,
    /// Localized biography (kind=Overview); resolve with [`Translations::overview`].
    pub biography: Translations,
    pub profile: Option<Image>,
    pub department: Option<String>,
    pub remotes: Vec<RemoteEntry>,
    pub last_synced_at: Option<Timestamp>,
}

impl Person {
    pub fn remote_by_source(&self, source: RemoteSource) -> Option<&Remote> {
        self.remotes
            .iter()
            .map(|e| &e.remote)
            .find(|r| *r.source() == source)
    }
}

/// A slim person row for the people list view - only what's needed to filter and
/// render a list entry.
#[derive(Debug, Clone, Encode, Decode)]
#[musli(crate = musli_core)]
pub struct PersonItem {
    pub id: PersonId,
    /// Localized name strings; filter across locales with [`Translations::texts`].
    pub name: Translations,
    pub profile: Option<Image>,
    pub department: Option<String>,
    /// Number of show + movie credits, shown on the card.
    pub credit_count: u32,
}

/// One work a person is credited on, for the person detail page. Points back at
/// the owning show/movie via [`CreditOwner`] so the card can link to it.
#[derive(Debug, Clone, Encode, Decode)]
#[musli(crate = musli_core)]
pub struct PersonCredit {
    pub owner: CreditOwner,
    /// The owner's localized title.
    pub title: Translations,
    pub poster: Option<Image>,
    pub kind: CreditKind,
    /// The character played, when a cast credit.
    pub character: Translations,
    pub department: Option<String>,
    pub job: Option<String>,
    pub episode_count: Option<u32>,
    pub order: Option<u32>,
    /// The owner's release/first-air date, for sorting the filmography.
    pub date: Option<Timestamp>,
}

#[derive(Debug, Clone, Encode, Decode)]
#[musli(crate = musli_core)]
pub struct Show {
    pub id: ShowId,
    pub strings: Translations,
    pub first_air_date: Option<Timestamp>,
    pub tracked: bool,
    pub auto_sync: bool,
    pub remotes: Vec<RemoteEntry>,
    pub images: Vec<MediaImage>,
    pub poster: Option<Image>,
    pub banner: Option<Image>,
    pub backdrop: Option<Image>,
    /// Kinds whose selected graphic was explicitly chosen by the user (and are
    /// therefore protected from sync overwriting them).
    pub user_selected: Vec<ImageKind>,
    pub last_synced_at: Option<Timestamp>,
    pub language: Locale,
    pub include_specials: IncludeSpecials,
    pub air_date_filters: Option<FilterRules>,
}

impl Show {
    pub fn effective_include_specials(&self, default: bool) -> bool {
        self.include_specials.unwrap_or(default)
    }

    /// The air-date filters in effect for this show, falling back to `default`.
    pub fn effective_air_date_filters<'a>(&'a self, default: &'a FilterRules) -> &'a FilterRules {
        self.air_date_filters.as_ref().unwrap_or(default)
    }

    pub fn remote_by_source(&self, source: RemoteSource) -> Option<&Remote> {
        self.remotes
            .iter()
            .map(|e| &e.remote)
            .find(|r| *r.source() == source)
    }

    /// The remote source that drives full metadata sync for this media.
    pub fn primary_sync_source(&self) -> Option<RemoteSource> {
        primary_sync_source(&self.remotes)
    }

    pub fn is_selected(&self, kind: ImageKind, key: &ImageKey) -> bool {
        match kind {
            ImageKind::Poster => self
                .poster
                .as_ref()
                .map(|i| i.key() == key)
                .unwrap_or(false),
            ImageKind::Banner => self
                .banner
                .as_ref()
                .map(|i| i.key() == key)
                .unwrap_or(false),
            ImageKind::Backdrop => self
                .backdrop
                .as_ref()
                .map(|i| i.key() == key)
                .unwrap_or(false),
            _ => false,
        }
    }

    /// Whether the selected graphic for `kind` was explicitly chosen by the user.
    pub fn is_user_selected(&self, kind: ImageKind) -> bool {
        self.user_selected.contains(&kind)
    }
}

#[derive(Debug, Clone, PartialEq, Encode, Decode)]
#[musli(crate = musli_core)]
pub struct Season {
    pub id: SeasonId,
    pub show_id: ShowId,
    pub season: SeasonNumber,
    pub air_date: Option<Timestamp>,
    pub strings: Translations,
    pub poster: Option<Image>,
    pub watched_count: u32,
    pub total_count: u32,
}

impl Season {
    pub fn is_selected(&self, kind: ImageKind, key: &ImageKey) -> bool {
        match kind {
            ImageKind::Poster => self
                .poster
                .as_ref()
                .map(|i| i.key() == key)
                .unwrap_or(false),
            _ => false,
        }
    }
}

#[derive(Debug, Clone, Encode, Decode)]
#[musli(crate = musli_core)]
pub struct Episode {
    pub id: EpisodeId,
    pub show_id: ShowId,
    pub season: SeasonNumber,
    pub episode: u32,
    pub absolute_number: Option<u32>,
    pub strings: Translations,
    pub aired: Option<Timestamp>,
    pub pending: Option<Timestamp>,
    pub watched_count: u32,
    pub screenshot: Option<Image>,
}

impl Episode {
    #[inline]
    pub fn code(&self) -> Code {
        Code {
            season: self.season,
            episode: self.episode,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Encode, Decode)]
#[musli(crate = musli_core)]
pub struct Code {
    pub season: SeasonNumber,
    pub episode: u32,
}

impl Code {
    /// Construct a code from a season and episode number. Matches the value
    /// returned by [`Episode::code`], so it can be used to target an episode's
    /// rendered element (its `id`).
    #[inline]
    pub fn new(season: SeasonNumber, episode: u32) -> Self {
        Self { season, episode }
    }
}

impl fmt::Display for Code {
    #[inline]
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}E{:02}", self.season.short(), self.episode)
    }
}

#[cfg(feature = "yew")]
impl IntoPropValue<Option<IString>> for Code {
    #[inline]
    fn into_prop_value(self) -> Option<IString> {
        Some(self.to_string().into())
    }
}

#[cfg(feature = "yew")]
impl IntoPropValue<VNode> for Code {
    #[inline]
    fn into_prop_value(self) -> VNode {
        self.to_string().into()
    }
}

#[derive(Debug, Clone, Encode, Decode)]
#[musli(crate = musli_core)]
pub struct WatchedEpisode {
    pub id: WatchedId,
    pub timestamp: Timestamp,
    pub season: SeasonNumber,
    pub number: u32,
    pub episode_id: EpisodeId,
}

#[derive(Debug, Clone, Encode, Decode)]
#[musli(crate = musli_core)]
pub struct Movie {
    pub id: MovieId,
    pub strings: Translations,
    pub release_date: Option<Timestamp>,
    pub remotes: Vec<RemoteEntry>,
    pub tracked: bool,
    /// Whether the background loop automatically refreshes this movie.
    pub auto_sync: bool,
    pub pending: Option<Timestamp>,
    pub images: Vec<MediaImage>,
    pub poster: Option<Image>,
    pub banner: Option<Image>,
    pub backdrop: Option<Image>,
    /// Kinds whose selected graphic was explicitly chosen by the user (and are
    /// therefore protected from sync overwriting them).
    pub user_selected: Vec<ImageKind>,
    pub last_synced_at: Option<Timestamp>,
    pub releases: Vec<MovieRelease>,
    pub language: Locale,
    pub release_filters: Option<FilterRules>,
}

impl Movie {
    /// The release filters in effect for this movie, falling back to the global `default`.
    pub fn effective_release_filters<'a>(&'a self, default: &'a FilterRules) -> &'a FilterRules {
        self.release_filters.as_ref().unwrap_or(default)
    }

    /// The effective release timestamp used to determine when this movie becomes pending, picking
    /// the earliest release matching the effective filters.
    pub fn pending_release(&self, default: &FilterRules) -> Option<Timestamp> {
        self.effective_release_filters(default)
            .earliest_release(&self.releases)
    }

    pub fn remote_by_source(&self, source: RemoteSource) -> Option<&Remote> {
        self.remotes
            .iter()
            .map(|e| &e.remote)
            .find(|r| *r.source() == source)
    }

    /// The remote source that drives full metadata sync for this media.
    pub fn primary_sync_source(&self) -> Option<RemoteSource> {
        primary_sync_source(&self.remotes)
    }

    pub fn is_selected(&self, kind: ImageKind, key: &ImageKey) -> bool {
        match kind {
            ImageKind::Poster => self
                .poster
                .as_ref()
                .map(|i| i.key() == key)
                .unwrap_or(false),
            ImageKind::Banner => self
                .banner
                .as_ref()
                .map(|i| i.key() == key)
                .unwrap_or(false),
            ImageKind::Backdrop => self
                .backdrop
                .as_ref()
                .map(|i| i.key() == key)
                .unwrap_or(false),
            _ => false,
        }
    }

    /// Whether the selected graphic for `kind` was explicitly chosen by the user.
    pub fn is_user_selected(&self, kind: ImageKind) -> bool {
        self.user_selected.contains(&kind)
    }
}

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Encode, Decode, serde::Serialize, serde::Deserialize,
)]
#[musli(crate = musli_core)]
pub enum ImageOwner {
    Show(ShowId),
    Movie(MovieId),
    Season(SeasonId),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Encode, Decode)]
#[musli(crate = musli_core)]
pub enum WatchedKind {
    Episode { show: ShowId, episode: EpisodeId },
    Movie { movie: MovieId },
}

impl WatchedKind {
    pub fn into_event(self) -> WatchedEvent {
        match self {
            WatchedKind::Episode { show, episode } => WatchedEvent::Episode { show, episode },
            WatchedKind::Movie { movie } => WatchedEvent::Movie { movie },
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Encode, Decode)]
#[musli(crate = musli_core)]
pub enum WatchedEvent {
    Episode { show: ShowId, episode: EpisodeId },
    RemainingSeason { show: ShowId, season: SeasonNumber },
    Movie { movie: MovieId },
}

#[derive(Debug, Clone, Encode, Decode)]
#[musli(crate = musli_core)]
pub struct Watched {
    pub id: WatchedId,
    pub timestamp: Timestamp,
    pub kind: WatchedKind,
}

#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
#[musli(crate = musli_core)]
pub enum PendingInfo {
    Episode {
        show_id: ShowId,
        episode_id: EpisodeId,
        show: Option<String>,
        episode: Option<String>,
        season: SeasonNumber,
        number: u32,
    },
    Movie {
        movie: MovieId,
        title: Option<String>,
    },
}

impl PendingInfo {
    /// The lightweight identity ([`PendingKind`]) carried by this entry.
    pub fn kind(&self) -> PendingKind {
        match self {
            PendingInfo::Episode {
                show_id,
                episode_id,
                ..
            } => PendingKind::Episode {
                show: *show_id,
                episode: *episode_id,
            },
            PendingInfo::Movie { movie, .. } => PendingKind::Movie { movie: *movie },
        }
    }

    /// Which media kind this entry is, for media-kind filtering.
    pub fn media_kind(&self) -> MediaKind {
        match self {
            PendingInfo::Episode { .. } => MediaKind::Shows,
            PendingInfo::Movie { .. } => MediaKind::Movies,
        }
    }
}

/// Denormalized pending item for dashboard/queue rendering.
#[derive(Debug, Clone, Encode, Decode)]
#[musli(crate = musli_core)]
pub struct Pending {
    pub info: PendingInfo,
    pub aired: Option<Timestamp>,
    pub timestamp: Timestamp,
    pub poster: Option<Image>,
    pub banner: Option<Image>,
    pub season_poster: Option<Image>,
    pub season_banner: Option<Image>,
    /// Backdrop used as the page background when this entry is hovered.
    pub backdrop: Option<Image>,
}

/// Implemented by types that carry both a civil air date and an optional
/// precise timestamp. `display_at` picks the most precise value available
/// and formats it in the given time zone.
pub trait Timed {
    fn aired(&self) -> Option<Timestamp>;

    fn human_date_time(&self, time: TimeInfo) -> Option<HumanDateTime> {
        let ts = self.aired()?;
        Some(ts.human_date_time(time))
    }

    fn date(&self, time: TimeInfo) -> Option<Date> {
        let ts = self.aired()?;
        Some(ts.date(time))
    }
}

impl Timed for Episode {
    #[inline]
    fn aired(&self) -> Option<Timestamp> {
        self.aired
    }
}

impl Timed for Season {
    #[inline]
    fn aired(&self) -> Option<Timestamp> {
        self.air_date
    }
}

impl Timed for Pending {
    #[inline]
    fn aired(&self) -> Option<Timestamp> {
        self.aired
    }
}

/// Sparse episode shown in the schedule/calendar grid. The calendar only renders
/// the `SxxEyy` code and uses the season for navigation, so it deliberately omits
/// the heavyweight fields of [`Episode`].
#[derive(Debug, Clone, Encode, Decode)]
#[musli(crate = musli_core)]
pub struct ScheduleEpisode {
    pub season: SeasonNumber,
    pub episode: u32,
    /// When the episode airs/becomes available, rendered as a local time of day.
    pub aired: Timestamp,
}

impl ScheduleEpisode {
    pub fn code(&self) -> Code {
        Code {
            season: self.season,
            episode: self.episode,
        }
    }
}

#[derive(Debug, Clone, Encode, Decode)]
#[musli(crate = musli_core)]
pub struct ScheduledEntry {
    pub show_id: ShowId,
    pub show_title: String,
    pub episodes: Vec<ScheduleEpisode>,
    /// Backdrop used as the page background when this entry is hovered.
    pub backdrop: Option<Image>,
    /// Poster shown in the schedule's side rail when this entry is hovered.
    pub poster: Option<Image>,
}

/// Sparse movie shown in the schedule/calendar grid on its release date.
#[derive(Debug, Clone, Encode, Decode)]
#[musli(crate = musli_core)]
pub struct ScheduleMovie {
    pub movie_id: MovieId,
    pub title: String,
    /// When the movie releases/becomes available, rendered as a local time of day.
    pub released: Timestamp,
    /// Backdrop used as the page background when this entry is hovered.
    pub backdrop: Option<Image>,
    /// Poster shown in the schedule's side rail when this entry is hovered.
    pub poster: Option<Image>,
}

#[derive(Debug, Clone, Encode, Decode)]
#[musli(crate = musli_core)]
pub struct ScheduledDay {
    pub date: Date,
    pub shows: Vec<ScheduledEntry>,
    pub movies: Vec<ScheduleMovie>,
}

/// A single block in a day's schedule timeline: either a show (with its grouped
/// episodes) or a movie.
pub enum ScheduleItem<'a> {
    Show(&'a ScheduledEntry),
    Movie(&'a ScheduleMovie),
}

impl ScheduleItem<'_> {
    /// Which media kind this item is, for media-kind filtering.
    pub fn kind(&self) -> MediaKind {
        match self {
            ScheduleItem::Show(..) => MediaKind::Shows,
            ScheduleItem::Movie(..) => MediaKind::Movies,
        }
    }

    /// Sort key: a show sorts by its earliest episode's air time, a movie by
    /// its release time. Show entries always have episodes; an empty one sorts
    /// last so it never masks a real time.
    fn timestamp(&self) -> Option<Timestamp> {
        match self {
            ScheduleItem::Show(e) => e.episodes.iter().map(|ep| ep.aired).min(),
            ScheduleItem::Movie(m) => Some(m.released),
        }
    }
}

impl ScheduledDay {
    /// Shows and movies interleaved and ordered by air/release time.
    pub fn items(&self) -> Vec<ScheduleItem<'_>> {
        let mut items: Vec<ScheduleItem<'_>> = self
            .shows
            .iter()
            .map(ScheduleItem::Show)
            .chain(self.movies.iter().map(ScheduleItem::Movie))
            .collect();
        // None (episode-less entries) sorts last via the (is_none, ts) key.
        items.sort_by_key(|i| (i.timestamp().is_none(), i.timestamp()));
        items
    }
}

#[derive(Debug, Clone, Encode, Decode)]
#[musli(crate = musli_core)]
pub struct Config {
    pub theme: ThemeType,
    pub tvdb_api_key: String,
    pub tvdb_pin: Option<String>,
    pub tmdb_api_key: String,
    pub dashboard_page: u32,
    /// How far into the future pending items surface on the dashboard. A
    /// pending item is shown once its timestamp falls within this window.
    pub dashboard_lookahead: Duration,
    /// Number of weeks shown in the dashboard schedule (always at least 1).
    pub schedule_weeks: u32,
    /// Number of days shown in the dashboard's upcoming-days strip (always at
    /// least 1).
    pub schedule_range_days: u32,
    pub auto_sync_enabled: bool,
    pub auto_sync_interval_hours: u32,
    /// The site/page title. Empty (or whitespace-only) means the default
    /// `"Track"` is used.
    pub page_title: String,
    pub timezone: String,
    /// The default display locale. [`Locale::DEFAULT`] means "use each
    /// show's/movie's own original language".
    pub language: Locale,
    pub include_specials: bool,
    /// Default rules that determine which releases set a movie's release date.
    pub release_filters: FilterRules,
    /// Default air-date qualification rules for episodes (empty = none qualify).
    pub air_date_filters: FilterRules,
    /// Global per-source selection of which kinds each source contributes during
    /// sync. A source absent here uses its full capability. Per-remote overrides
    /// take precedence. See [`Config::sync_kinds_for`].
    pub sync_kinds: Vec<SourceSyncKinds>,
    /// Which locales the sync path populates translations for.
    /// [`Locale::DEFAULT`] stands for each media's own original language.
    pub sync_languages: Vec<Locale>,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            theme: ThemeType::Dark,
            tvdb_api_key: String::new(),
            tvdb_pin: None,
            tmdb_api_key: String::new(),
            dashboard_page: 12,
            dashboard_lookahead: Duration::from_hours(24),
            schedule_weeks: 4,
            schedule_range_days: 3,
            auto_sync_enabled: false,
            auto_sync_interval_hours: 24,
            page_title: String::new(),
            timezone: String::new(),
            language: Locale::DEFAULT,
            include_specials: false,
            release_filters: FilterRules::default_release_rules(),
            air_date_filters: FilterRules::default(),
            sync_kinds: Vec::new(),
            sync_languages: vec![
                Locale::DEFAULT,
                Locale::new(Language::ENG, Country::DEFAULT),
            ],
        }
    }
}

impl Config {
    /// The global default sync kinds for `source`: the configured entry if
    /// present, otherwise the source's full capability - in both cases clamped
    /// to capability.
    pub fn sync_kinds_for(&self, source: RemoteSource) -> SyncKindSet {
        self.sync_kinds
            .iter()
            .find(|s| s.source == source)
            .map(|s| s.kinds)
            .unwrap_or_else(|| source.default_sync_kinds())
            .intersect(source.default_sync_kinds())
    }
}

#[derive(Debug, Encode, Decode)]
#[musli(crate = musli_core)]
pub struct Empty;

#[derive(Debug, Clone, Encode, Decode)]
#[musli(crate = musli_core)]
pub struct SearchShow {
    pub remote: Remote,
    pub slug: Option<String>,
    pub title: Option<String>,
    pub poster: Option<Image>,
    pub banner: Option<Image>,
    pub backdrop: Option<Image>,
    pub overview: Option<String>,
    pub first_air_date: Option<Date>,
    pub already_tracked: Option<ShowId>,
}

#[derive(Debug, Clone, Encode, Decode)]
#[musli(crate = musli_core)]
pub struct SearchMovie {
    pub remote: Remote,
    pub title: Option<String>,
    pub poster: Option<Image>,
    pub banner: Option<Image>,
    pub backdrop: Option<Image>,
    pub overview: Option<String>,
    pub release_date: Option<Date>,
    pub already_tracked: Option<MovieId>,
}

/// Whether a media item is a show or a movie.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Encode, Decode)]
#[musli(crate = musli_core)]
pub enum MediaKind {
    #[default]
    Shows,
    Movies,
}

/// A slim row used by the movies/shows list views - only the fields needed to
/// filter, reorder, and render a list entry.
#[derive(Debug, Clone, Encode, Decode)]
#[musli(crate = musli_core)]
pub struct MediaItem {
    /// Raw id; the consumer rebuilds `ShowId`/`MovieId` based on `kind`.
    pub id: u64,
    /// Whether this item is a show or a movie.
    pub kind: MediaKind,
    /// All localized title/overview strings; resolve with [`Translations::get`]
    /// and filter across locales with [`Translations::texts`].
    pub strings: Translations,
    /// Release date (movie) or first-air date (show).
    pub date: Option<Timestamp>,
    pub poster: Option<Image>,
    pub banner: Option<Image>,
    /// Backdrop image, used to set the page background.
    pub backdrop: Option<Image>,
    pub tracked: bool,
    pub last_watched_at: Option<Timestamp>,
    /// Remote entries, used to render external links in the list.
    pub remotes: Vec<RemoteEntry>,
}

#[derive(Debug, Clone, Encode, Decode)]
#[musli(crate = musli_core)]
pub enum TaskKind {
    SyncShow {
        show_id: ShowId,
        title: Option<String>,
    },
    SyncMovie {
        movie_id: MovieId,
        title: Option<String>,
    },
    /// Sync a single episode, scheduled hourly around its air date (when
    /// remotes tend to correct episode metadata) and triggerable by hand.
    SyncEpisode {
        show_id: ShowId,
        episode_id: EpisodeId,
        code: Code,
        title: Option<String>,
    },
    /// Sync a single person's own data (localized name/biography, profile images),
    /// scheduled by the background poller independently of shows and movies.
    SyncPerson {
        person_id: PersonId,
        title: Option<String>,
    },
    /// Recompute the most-used custom languages across shows and movies.
    RefreshTopLanguages,
}

impl TaskKind {
    #[inline]
    pub fn title(&self) -> Option<&str> {
        match self {
            TaskKind::SyncShow { title, .. }
            | TaskKind::SyncMovie { title, .. }
            | TaskKind::SyncEpisode { title, .. }
            | TaskKind::SyncPerson { title, .. } => title.as_deref(),
            TaskKind::RefreshTopLanguages => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Encode, Decode)]
#[musli(crate = musli_core)]
pub enum TaskStatus {
    Pending,
    Running,
}

#[derive(Debug, Clone, Encode, Decode)]
#[musli(crate = musli_core)]
pub struct Task {
    pub id: TaskId,
    pub kind: TaskKind,
    pub status: TaskStatus,
    /// Wall-clock time the task is expected to start running, or `None` when it
    /// is already running.
    pub run_at: Option<Timestamp>,
}

#[derive(Debug, Clone, Encode, Decode)]
#[musli(crate = musli_core)]
pub struct CompletedTask {
    pub id: TaskId,
    pub kind: TaskKind,
    /// Wall-clock time the task finished.
    pub completed_at: Timestamp,
}

#[derive(Debug, Encode, Decode)]
#[musli(crate = musli_core)]
pub struct ListMediaRequest;

#[derive(Debug, Encode, Decode)]
#[musli(crate = musli_core)]
pub struct ListMediaResponse {
    pub items: Vec<MediaItem>,
}

#[derive(Debug, Encode, Decode)]
#[musli(crate = musli_core)]
pub struct GetShowRequest {
    pub id: ShowId,
}

/// The entity whose translations are being requested. A single endpoint serves
/// all four entity kinds via this type-safe target.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Encode, Decode)]
#[musli(crate = musli_core)]
pub enum TranslationTarget {
    Show(ShowId),
    Season(SeasonId),
    Episode(EpisodeId),
    Movie(MovieId),
}

/// The owner a set of [`Credit`]s belongs to.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Encode, Decode)]
#[musli(crate = musli_core)]
pub enum CreditOwner {
    Show(ShowId),
    Movie(MovieId),
}

#[derive(Debug, Encode, Decode)]
#[musli(crate = musli_core)]
pub struct GetTranslationsRequest {
    pub target: TranslationTarget,
}

/// A single translated string for an entity.
#[derive(Debug, Clone, Encode, Decode)]
#[musli(crate = musli_core)]
pub struct Translation {
    pub language: Locale,
    pub kind: StringKind,
    pub text: String,
}

#[derive(Debug, Encode, Decode)]
#[musli(crate = musli_core)]
pub struct GetTranslationsResponse {
    pub translations: Vec<Translation>,
}

#[derive(Debug, Encode, Decode)]
#[musli(crate = musli_core)]
pub struct ListSeasonsRequest {
    pub show_id: ShowId,
}

#[derive(Debug, Encode, Decode)]
#[musli(crate = musli_core)]
pub struct ListSeasonsResponse {
    pub seasons: Vec<Season>,
}

#[derive(Debug, Encode, Decode)]
#[musli(crate = musli_core)]
pub struct ListCreditsRequest {
    pub owner: CreditOwner,
}

#[derive(Debug, Encode, Decode)]
#[musli(crate = musli_core)]
pub struct ListCreditsResponse {
    pub credits: Vec<Credit>,
}

#[derive(Debug, Encode, Decode)]
#[musli(crate = musli_core)]
pub struct ListPersonsRequest;

#[derive(Debug, Encode, Decode)]
#[musli(crate = musli_core)]
pub struct ListPersonsResponse {
    pub persons: Vec<PersonItem>,
}

#[derive(Debug, Encode, Decode)]
#[musli(crate = musli_core)]
pub struct GetPersonRequest {
    pub id: PersonId,
}

#[derive(Debug, Encode, Decode)]
#[musli(crate = musli_core)]
pub struct ListPersonCreditsRequest {
    pub id: PersonId,
}

#[derive(Debug, Encode, Decode)]
#[musli(crate = musli_core)]
pub struct ListPersonCreditsResponse {
    pub credits: Vec<PersonCredit>,
}

#[derive(Debug, Encode, Decode)]
#[musli(crate = musli_core)]
pub struct AddPersonRemoteRequest {
    pub id: PersonId,
    pub slug: Option<String>,
    pub remote: Remote,
}

#[derive(Debug, Encode, Decode)]
#[musli(crate = musli_core)]
pub struct RemovePersonRemoteRequest {
    pub id: PersonId,
    pub remote_id: RemoteId,
}

#[derive(Debug, Encode, Decode)]
#[musli(crate = musli_core)]
pub struct UpdatePersonRemoteRequest {
    pub id: PersonId,
    pub remote_id: RemoteId,
    pub slug: Option<String>,
    pub remote: Remote,
}

#[derive(Debug, Encode, Decode)]
#[musli(crate = musli_core)]
pub struct SetPersonRemoteEnabledRequest {
    pub id: PersonId,
    pub remote_id: RemoteId,
    pub enabled: bool,
}

#[derive(Debug, Encode, Decode)]
#[musli(crate = musli_core)]
pub struct ReorderPersonRemotesRequest {
    pub id: PersonId,
    /// Remote ids in the desired priority order (first = highest priority).
    pub remote_ids: Vec<RemoteId>,
}

#[derive(Debug, Encode, Decode)]
#[musli(crate = musli_core)]
pub struct SetPersonRemoteSyncKindsRequest {
    pub id: PersonId,
    pub remote_id: RemoteId,
    /// `None` clears the override so the remote inherits the global default.
    pub sync_kinds: Option<SyncKindSet>,
}

#[derive(Debug, Encode, Decode)]
#[musli(crate = musli_core)]
pub struct PurgePersonRemoteCacheRequest {
    pub id: PersonId,
    pub remote_id: RemoteId,
}

#[derive(Debug, Encode, Decode)]
#[musli(crate = musli_core)]
pub struct DeletePersonRequest {
    pub id: PersonId,
}

#[derive(Debug, Encode, Decode)]
#[musli(crate = musli_core)]
pub struct GetSeasonImagesRequest {
    pub season_id: SeasonId,
}

#[derive(Debug, Encode, Decode)]
#[musli(crate = musli_core)]
pub struct GetSeasonImagesResponse {
    pub images: Vec<MediaImage>,
}

#[derive(Debug, Encode, Decode)]
#[musli(crate = musli_core)]
pub struct TrackShowRequest {
    pub slug: Option<String>,
    pub remote: Remote,
}

#[derive(Debug, Encode, Decode)]
#[musli(crate = musli_core)]
pub struct UntrackShowRequest {
    pub id: ShowId,
    pub tracked: bool,
}

#[derive(Debug, Encode, Decode)]
#[musli(crate = musli_core)]
pub struct RemoveShowRequest {
    pub id: ShowId,
}

#[derive(Debug, Encode, Decode)]
#[musli(crate = musli_core)]
pub struct ListEpisodesRequest {
    pub show_id: ShowId,
    pub season: SeasonNumber,
}

#[derive(Debug, Encode, Decode)]
#[musli(crate = musli_core)]
pub struct ListEpisodesResponse {
    pub episodes: Vec<Episode>,
    pub watched: Vec<WatchedEpisode>,
}

#[derive(Debug, Encode, Decode)]
#[musli(crate = musli_core)]
pub struct GetEpisodeReleasesRequest {
    pub episode_id: EpisodeId,
}

#[derive(Debug, Encode, Decode)]
#[musli(crate = musli_core)]
pub struct GetEpisodeReleasesResponse {
    pub releases: Vec<ReleaseRow>,
    /// The owning show, whose air-date filters apply to this episode and which the
    /// modal mutates to change the per-series override.
    pub show_id: ShowId,
    /// The show's air-date override, or `None` when the global default is in use.
    pub filters: Option<FilterRules>,
}

#[derive(Debug, Encode, Decode)]
#[musli(crate = musli_core)]
pub struct GetEpisodeCacheRequest {
    pub episode_id: EpisodeId,
}

#[derive(Debug, Encode, Decode)]
#[musli(crate = musli_core)]
pub struct GetEpisodeCacheResponse {
    pub entries: Vec<EpisodeCacheEntry>,
}

/// One source's conditional-request state for an episode, as stored in the
/// `episode_cache` table. Unlike shows and movies, an episode is not addressed by a
/// remote id, so the entry is keyed by source alone.
#[derive(Debug, Encode, Decode)]
#[musli(crate = musli_core)]
pub struct EpisodeCacheEntry {
    pub source: RemoteSource,
    pub cache: RemoteCache,
}

#[derive(Debug, Encode, Decode)]
#[musli(crate = musli_core)]
pub struct PurgeEpisodeCacheRequest {
    pub episode_id: EpisodeId,
    pub source: RemoteSource,
}

#[derive(Debug, Encode, Decode)]
#[musli(crate = musli_core)]
pub struct GetMovieReleasesRequest {
    pub movie_id: MovieId,
}

#[derive(Debug, Encode, Decode)]
#[musli(crate = musli_core)]
pub struct GetMovieReleasesResponse {
    pub releases: Vec<ReleaseRow>,
    /// The movie's release override, or `None` when the global default is in use.
    pub filters: Option<FilterRules>,
}

/// A single release as shown in the release/air-date modal, with its grouping
/// `label` and `considered` flag already resolved server-side (movies via
/// [`FilterRules::release_accepted`], episodes via
/// [`FilterRules::air_date_considered`]). `label` is the
/// release type's name for movies and the network (or `"Unknown"`) for episodes.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
#[musli(crate = musli_core)]
pub struct ReleaseRow {
    pub label: String,
    pub source: RemoteSource,
    pub country: Country,
    pub timestamp: Timestamp,
    pub considered: bool,
}

#[derive(Debug, Encode, Decode)]
#[musli(crate = musli_core)]
pub struct FindEpisodeByTimestampRequest {
    pub show_id: ShowId,
    pub timestamp: Timestamp,
}

#[derive(Debug, Encode, Decode)]
#[musli(crate = musli_core)]
pub struct EpisodeMatch {
    pub season: SeasonNumber,
    pub episode: u32,
}

#[derive(Debug, Encode, Decode)]
#[musli(crate = musli_core)]
pub struct FindEpisodeByTimestampResponse {
    pub matched: Option<EpisodeMatch>,
}

#[derive(Debug, Encode, Decode)]
#[musli(crate = musli_core)]
pub struct GetMovieRequest {
    pub id: MovieId,
}

#[derive(Debug, Encode, Decode)]
#[musli(crate = musli_core)]
pub struct TrackMovieRequest {
    pub slug: Option<String>,
    pub remote: Remote,
}

#[derive(Debug, Encode, Decode)]
#[musli(crate = musli_core)]
pub struct UntrackMovieRequest {
    pub id: MovieId,
    pub tracked: bool,
}

#[derive(Debug, Encode, Decode)]
#[musli(crate = musli_core)]
pub struct RemoveMovieRequest {
    pub id: MovieId,
}

#[derive(Debug, Clone, Copy, Encode, Decode)]
#[musli(crate = musli_core)]
pub enum MarkTime {
    Now,
    WhenAired,
    /// An explicit instant chosen by the user.
    At(Timestamp),
}

#[derive(Debug, Encode, Decode)]
#[musli(crate = musli_core)]
pub struct MarkWatchedRequest {
    pub kind: WatchedKind,
    pub mark_time: MarkTime,
}

#[derive(Debug, Encode, Decode)]
#[musli(crate = musli_core)]
pub struct MarkWatchedResponse {
    pub watched: Watched,
    /// What was pending before the watch moved it along, for undoing it.
    pub pending_before: PendingBefore,
}

/// The pending entry a watch replaced; see [`UndoWatchedRequest`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Encode, Decode)]
#[musli(crate = musli_core)]
pub enum PendingBefore {
    None,
    Episode {
        episode: EpisodeId,
        timestamp: Timestamp,
    },
    Movie {
        timestamp: Timestamp,
    },
}

/// Remove a watch that was just marked and put back what was pending before.
#[derive(Debug, Encode, Decode)]
#[musli(crate = musli_core)]
pub struct UndoWatchedRequest {
    pub id: WatchedId,
    pub kind: WatchedKind,
    pub pending_before: PendingBefore,
}

#[derive(Debug, Encode, Decode)]
#[musli(crate = musli_core)]
pub struct MarkWatchedRemainingRequest {
    pub show_id: ShowId,
    pub season: SeasonNumber,
    pub mark_time: MarkTime,
}

#[derive(Debug, Encode, Decode)]
#[musli(crate = musli_core)]
pub struct RemoveWatchedRequest {
    pub id: WatchedId,
    pub kind: WatchedKind,
}

#[derive(Debug, Encode, Decode)]
#[musli(crate = musli_core)]
pub struct ListEpisodesWatchedRequest {
    pub show_id: ShowId,
}

#[derive(Debug, Encode, Decode)]
#[musli(crate = musli_core)]
pub struct ListEpisodesWatchedResponse {
    pub watched: Vec<WatchedEpisode>,
}

#[derive(Debug, Encode, Decode)]
#[musli(crate = musli_core)]
pub struct ListWatchedRequest {
    pub kind: WatchedKind,
}

#[derive(Debug, Encode, Decode)]
#[musli(crate = musli_core)]
pub struct ListWatchedResponse {
    pub watched: Vec<Watched>,
}

#[derive(Debug, Clone, Encode, Decode)]
#[musli(crate = musli_core)]
pub struct OrphanedWatched {
    pub id: WatchedId,
    pub timestamp: Timestamp,
    pub show_id: ShowId,
    pub season: SeasonNumber,
    pub episode: u32,
}

impl OrphanedWatched {
    #[inline]
    pub fn code(&self) -> Code {
        Code {
            season: self.season,
            episode: self.episode,
        }
    }
}

#[derive(Debug, Encode, Decode)]
#[musli(crate = musli_core)]
pub struct MoveWatchedEpisodeRequest {
    pub id: WatchedId,
    pub show_id: ShowId,
    pub season: SeasonNumber,
    pub episode: u32,
}

#[derive(Debug, Encode, Decode)]
#[musli(crate = musli_core)]
pub struct ListOrphanedWatchedRequest {
    pub show_id: ShowId,
}

#[derive(Debug, Encode, Decode)]
#[musli(crate = musli_core)]
pub struct ListOrphanedWatchedResponse {
    pub watched: Vec<OrphanedWatched>,
}

#[derive(Debug, Encode, Decode)]
#[musli(crate = musli_core)]
pub struct ListPendingRequest;

#[derive(Debug, Encode, Decode)]
#[musli(crate = musli_core)]
pub struct ListPendingResponse {
    pub pending: Vec<Pending>,
}

#[derive(Debug, Encode, Decode)]
#[musli(crate = musli_core)]
pub struct ListScheduleRequest<'a> {
    pub tz: Option<&'a str>,
    /// Start of the window relative to the server's "today", in days. Negative
    /// values reach into past weeks.
    pub start_offset_days: i32,
    pub days: u32,
}

#[derive(Debug, Encode, Decode)]
#[musli(crate = musli_core)]
pub struct ListScheduleResponse {
    pub days: Vec<ScheduledDay>,
}

#[derive(Debug, Encode, Decode)]
#[musli(crate = musli_core)]
pub struct ListWatchNextRequest;

#[derive(Debug, Encode, Decode)]
#[musli(crate = musli_core)]
pub struct ListWatchNextResponse {
    pub pending: Vec<Pending>,
}

#[derive(Debug, Encode, Decode)]
#[musli(crate = musli_core)]
pub struct SearchRequest {
    pub query: String,
    pub page: usize,
    /// Whether to search remote shows. Both default on so search spans series
    /// and movies at once.
    pub shows: bool,
    /// Whether to search remote movies.
    pub movies: bool,
}

/// A single search hit, either a show or a movie. The backend interleaves the
/// two kinds into one ordered list so results are mixed rather than grouped.
#[derive(Debug, Clone, Encode, Decode)]
#[musli(crate = musli_core)]
pub enum SearchResult {
    Show(SearchShow),
    Movie(SearchMovie),
}

#[derive(Debug, Encode, Decode)]
#[musli(crate = musli_core)]
pub struct SearchResponse {
    /// Shows and movies interleaved, preserving each source's own order.
    pub results: Vec<SearchResult>,
    /// Total number of results across the queried sources.
    pub total: usize,
}

#[derive(Debug, Encode, Decode)]
#[musli(crate = musli_core)]
pub struct SyncShowRequest {
    pub id: ShowId,
}

#[derive(Debug, Encode, Decode)]
#[musli(crate = musli_core)]
pub struct SyncMovieRequest {
    pub id: MovieId,
}

#[derive(Debug, Encode, Decode)]
#[musli(crate = musli_core)]
pub struct SyncPersonRequest {
    pub id: PersonId,
}

#[derive(Debug, Encode, Decode)]
#[musli(crate = musli_core)]
pub struct SyncEpisodeRequest {
    pub show_id: ShowId,
    pub episode_id: EpisodeId,
}

#[derive(Debug, Encode, Decode)]
#[musli(crate = musli_core)]
pub struct SetShowRemoteEnabledRequest {
    pub id: ShowId,
    pub remote_id: RemoteId,
    pub enabled: bool,
}

#[derive(Debug, Encode, Decode)]
#[musli(crate = musli_core)]
pub struct ReorderShowRemotesRequest {
    pub id: ShowId,
    /// Remote ids in the desired priority order (first = highest priority).
    pub remote_ids: Vec<RemoteId>,
}

#[derive(Debug, Encode, Decode)]
#[musli(crate = musli_core)]
pub struct SetMovieRemoteEnabledRequest {
    pub id: MovieId,
    pub remote_id: RemoteId,
    pub enabled: bool,
}

#[derive(Debug, Encode, Decode)]
#[musli(crate = musli_core)]
pub struct ReorderMovieRemotesRequest {
    pub id: MovieId,
    /// Remote ids in the desired priority order (first = highest priority).
    pub remote_ids: Vec<RemoteId>,
}

#[derive(Debug, Encode, Decode)]
#[musli(crate = musli_core)]
pub struct SetShowRemoteSyncKindsRequest {
    pub id: ShowId,
    pub remote_id: RemoteId,
    /// `None` clears the override so the remote inherits the global default.
    pub sync_kinds: Option<SyncKindSet>,
}

#[derive(Debug, Encode, Decode)]
#[musli(crate = musli_core)]
pub struct SetMovieRemoteSyncKindsRequest {
    pub id: MovieId,
    pub remote_id: RemoteId,
    /// `None` clears the override so the remote inherits the global default.
    pub sync_kinds: Option<SyncKindSet>,
}

#[derive(Debug, Encode, Decode)]
#[musli(crate = musli_core)]
pub struct SetShowLanguageRequest {
    pub id: ShowId,
    pub language: Locale,
}

#[derive(Default, Debug, Clone, Copy, PartialEq, Encode, Decode)]
#[musli(crate = musli_core)]
pub enum IncludeSpecials {
    #[default]
    Default,
    Include,
    Skip,
}

impl IncludeSpecials {
    #[inline]
    pub fn cycle(self) -> Self {
        match self {
            IncludeSpecials::Default => IncludeSpecials::Include,
            IncludeSpecials::Include => IncludeSpecials::Skip,
            IncludeSpecials::Skip => IncludeSpecials::Default,
        }
    }

    /// Resolve this to a concrete boolean value.
    #[inline]
    pub fn unwrap_or(self, default: bool) -> bool {
        match self {
            IncludeSpecials::Default => default,
            IncludeSpecials::Include => true,
            IncludeSpecials::Skip => false,
        }
    }

    /// Return a human-readable label for this option, suitable for use in a UI.
    #[inline]
    pub fn as_label(&self) -> &'static str {
        match self {
            IncludeSpecials::Default => "Use global default",
            IncludeSpecials::Include => "Include specials",
            IncludeSpecials::Skip => "Skip specials",
        }
    }
}

impl fmt::Display for IncludeSpecials {
    #[inline]
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            IncludeSpecials::Default => write!(f, "default"),
            IncludeSpecials::Include => write!(f, "include"),
            IncludeSpecials::Skip => write!(f, "skip"),
        }
    }
}

#[cfg(feature = "sqll")]
impl ::sqll::FromColumn<'_> for IncludeSpecials {
    type Type = ::sqll::ty::Nullable<::sqll::ty::Integer>;

    fn from_column(
        stmt: &::sqll::Statement,
        index: ::sqll::ty::Nullable<::sqll::ty::Integer>,
    ) -> ::sqll::Result<Self> {
        match Option::<bool>::from_column(stmt, index)? {
            Some(true) => Ok(IncludeSpecials::Include),
            Some(false) => Ok(IncludeSpecials::Skip),
            None => Ok(IncludeSpecials::Default),
        }
    }
}

#[cfg(feature = "sqll")]
impl ::sqll::BindValue for IncludeSpecials {
    fn bind_value(&self, stmt: &mut ::sqll::Statement, index: ::sqll::Index) -> ::sqll::Result<()> {
        let value = match self {
            IncludeSpecials::Default => None,
            IncludeSpecials::Include => Some(true),
            IncludeSpecials::Skip => Some(false),
        };

        value.bind_value(stmt, index)
    }
}

#[derive(Debug, Encode, Decode)]
#[musli(crate = musli_core)]
pub struct SetShowIncludeSpecialsRequest {
    pub id: ShowId,
    pub include_specials: IncludeSpecials,
}

#[derive(Debug, Encode, Decode)]
#[musli(crate = musli_core)]
pub struct SetShowAutoSyncRequest {
    pub id: ShowId,
    pub auto_sync: bool,
}

#[derive(Debug, Encode, Decode)]
#[musli(crate = musli_core)]
pub struct SetMovieAutoSyncRequest {
    pub id: MovieId,
    pub auto_sync: bool,
}

#[derive(Debug, Encode, Decode)]
#[musli(crate = musli_core)]
pub struct SetShowAirDateFiltersRequest {
    pub id: ShowId,
    pub air_date_filters: Option<FilterRules>,
}

#[derive(Debug, Encode, Decode)]
#[musli(crate = musli_core)]
pub struct SetMovieLanguageRequest {
    pub id: MovieId,
    pub language: Locale,
}

#[derive(Debug, Encode, Decode)]
#[musli(crate = musli_core)]
pub struct SetMovieReleaseFiltersRequest {
    pub id: MovieId,
    pub release_filters: Option<FilterRules>,
}

#[derive(Debug, Encode, Decode)]
#[musli(crate = musli_core)]
pub struct AddShowRemoteRequest {
    pub id: ShowId,
    pub slug: Option<String>,
    pub remote: Remote,
}

#[derive(Debug, Encode, Decode)]
#[musli(crate = musli_core)]
pub struct RemoveShowRemoteRequest {
    pub id: ShowId,
    pub remote_id: RemoteId,
}

#[derive(Debug, Encode, Decode)]
#[musli(crate = musli_core)]
pub struct AddMovieRemoteRequest {
    pub id: MovieId,
    pub slug: Option<String>,
    pub remote: Remote,
}

#[derive(Debug, Encode, Decode)]
#[musli(crate = musli_core)]
pub struct RemoveMovieRemoteRequest {
    pub id: MovieId,
    pub remote_id: RemoteId,
}

#[derive(Debug, Encode, Decode)]
#[musli(crate = musli_core)]
pub struct PurgeShowRemoteCacheRequest {
    pub id: ShowId,
    pub remote_id: RemoteId,
}

#[derive(Debug, Encode, Decode)]
#[musli(crate = musli_core)]
pub struct PurgeMovieRemoteCacheRequest {
    pub id: MovieId,
    pub remote_id: RemoteId,
}

#[derive(Debug, Encode, Decode)]
#[musli(crate = musli_core)]
pub struct UpdateShowRemoteRequest {
    pub id: ShowId,
    pub remote_id: RemoteId,
    pub slug: Option<String>,
    pub remote: Remote,
}

#[derive(Debug, Encode, Decode)]
#[musli(crate = musli_core)]
pub struct UpdateMovieRemoteRequest {
    pub id: MovieId,
    pub remote_id: RemoteId,
    pub slug: Option<String>,
    pub remote: Remote,
}

#[derive(Debug, Encode, Decode)]
#[musli(crate = musli_core)]
pub struct SyncAllRequest;

#[derive(Debug, Encode, Decode)]
#[musli(crate = musli_core)]
pub struct ListTasksRequest;

#[derive(Debug, Encode, Decode)]
#[musli(crate = musli_core)]
pub struct RemoveTaskRequest {
    pub id: TaskId,
}

#[derive(Debug, Encode, Decode)]
#[musli(crate = musli_core)]
pub struct BumpTaskRequest {
    pub id: TaskId,
}

#[derive(Debug, Encode, Decode)]
#[musli(crate = musli_core)]
pub struct ListTasksResponse {
    pub pending: Vec<Task>,
    pub running: Vec<Task>,
    pub completed: Vec<CompletedTask>,
}

#[derive(Debug, Encode, Decode)]
#[musli(crate = musli_core)]
pub struct GetConfigRequest;

#[derive(Debug, Encode, Decode)]
#[musli(crate = musli_core)]
pub struct GetConfigResponse {
    pub config: Config,
}

#[derive(Debug, Encode, Decode)]
#[musli(crate = musli_core)]
pub struct GetTopLanguagesRequest;

#[derive(Debug, Encode, Decode)]
#[musli(crate = musli_core)]
pub struct GetTopLanguagesResponse {
    /// Most-used custom locales, ordered most-used first.
    pub top_languages: Vec<Locale>,
}

#[derive(Debug, Encode, Decode)]
#[musli(crate = musli_core)]
pub struct SetConfigRequest {
    pub config: Config,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Encode, Decode)]
#[musli(crate = musli_core)]
pub enum PendingKind {
    Episode { show: ShowId, episode: EpisodeId },
    Movie { movie: MovieId },
}

impl PendingKind {
    #[inline]
    pub fn title(&self) -> &'static str {
        match self {
            PendingKind::Episode { .. } => "episode",
            PendingKind::Movie { .. } => "movie",
        }
    }
}

#[derive(Debug, Encode, Decode)]
#[musli(crate = musli_core)]
pub struct AddPendingRequest {
    pub kind: PendingKind,
    /// When the pending slot should be dated: `Now`, or when the episode aired
    /// / movie was released (`WhenAired`).
    pub mark_time: MarkTime,
}

#[derive(Debug, Encode, Decode)]
#[musli(crate = musli_core)]
pub struct AddPendingResponse {
    /// The rebuilt pending entry, or `None` if the media is no longer tracked.
    pub pending: Option<Pending>,
}

#[derive(Debug, Encode, Decode)]
#[musli(crate = musli_core)]
pub struct RemovePendingRequest {
    pub kind: PendingKind,
}

#[derive(Debug, Encode, Decode)]
#[musli(crate = musli_core)]
pub struct SkipEpisodeRequest {
    pub show: ShowId,
    pub episode: EpisodeId,
}

#[derive(Debug, Encode, Decode)]
#[musli(crate = musli_core)]
pub struct SelectImageRequest {
    pub id: ImageId,
}

#[derive(Debug, Encode, Decode)]
#[musli(crate = musli_core)]
pub struct ClearSelectedImageRequest {
    pub owner: ImageOwner,
    pub kind: ImageKind,
}

/// Select the best stored graphic (highest-priority remote's top-scored image)
/// for a single kind when `kind` is `Some`, or for every kind when `None`. This
/// marks the selection as user-chosen and operates only on stored data.
#[derive(Debug, Encode, Decode)]
#[musli(crate = musli_core)]
pub struct PickBestImagesRequest {
    pub owner: ImageOwner,
    pub kind: Option<ImageKind>,
}

/// Clear the user override for a kind, re-applying the configured-order default
/// and handing the kind back to automatic sync management.
#[derive(Debug, Encode, Decode)]
#[musli(crate = musli_core)]
pub struct ResetImageSelectionRequest {
    pub owner: ImageOwner,
    pub kind: ImageKind,
}

#[derive(Debug, Clone, Encode, Decode)]
#[musli(crate = musli_core)]
pub struct AppEvent {
    pub channel: ChannelId,
    pub kind: AppEventKind,
}

#[derive(Debug, Clone, Encode, Decode)]
#[musli(crate = musli_core)]
pub enum AppEventKind {
    ShowCreated {
        show: Show,
    },
    ShowChanged {
        show: Show,
    },
    ShowDeleted {
        show_id: ShowId,
    },
    SeasonsChanged {
        show_id: ShowId,
        seasons: Vec<Season>,
    },
    EpisodeChanged {
        episode: Episode,
    },
    EpisodesChanged {
        show_id: ShowId,
        season: SeasonNumber,
    },
    MovieCreated {
        movie: Movie,
    },
    MovieChanged {
        movie: Movie,
    },
    MovieDeleted {
        movie_id: MovieId,
    },
    /// The translated strings under a show or movie were rewritten (during sync),
    /// so an open translations view for that entity can refresh itself.
    TranslationsChanged {
        target: TranslationTarget,
    },
    /// The cast & crew credits under a show or movie were rewritten (during
    /// sync), so an open credits view for that entity can refresh itself.
    CreditsChanged {
        target: TranslationTarget,
    },
    /// A person's own data (name, biography, images) was re-synced, so any
    /// credit view showing that person can refresh.
    PersonChanged {
        person_id: PersonId,
    },
    WatchedChanged {
        event: WatchedEvent,
    },
    PendingChanged,
    /// A single pending entry was added or re-dated. Carries the rebuilt entry so
    /// listeners can update just that row instead of reloading the whole list.
    PendingEntryChanged {
        pending: Pending,
    },
    ConfigChanged {
        config: Config,
    },
    TopLanguagesChanged {
        top_languages: Vec<Locale>,
    },
    TaskAdded {
        task: Task,
    },
    TaskStarted {
        task: Task,
    },
    TaskCompleted {
        task: CompletedTask,
    },
    TaskBumped {
        task: Task,
    },
    TaskRemoved {
        task_id: TaskId,
    },
}

api::define! {
    pub type ListMedia;
    impl Endpoint for ListMedia {
        impl Request for ListMediaRequest;
        type Response<'de> = ListMediaResponse;
    }

    pub type GetShow;
    impl Endpoint for GetShow {
        impl Request for GetShowRequest;
        type Response<'de> = Option<Show>;
    }

    pub type GetTranslations;
    impl Endpoint for GetTranslations {
        impl Request for GetTranslationsRequest;
        type Response<'de> = GetTranslationsResponse;
    }

    pub type ListSeasons;
    impl Endpoint for ListSeasons {
        impl Request for ListSeasonsRequest;
        type Response<'de> = ListSeasonsResponse;
    }

    pub type ListCredits;
    impl Endpoint for ListCredits {
        impl Request for ListCreditsRequest;
        type Response<'de> = ListCreditsResponse;
    }

    pub type ListPersons;
    impl Endpoint for ListPersons {
        impl Request for ListPersonsRequest;
        type Response<'de> = ListPersonsResponse;
    }

    pub type GetPerson;
    impl Endpoint for GetPerson {
        impl Request for GetPersonRequest;
        type Response<'de> = Option<Person>;
    }

    pub type ListPersonCredits;
    impl Endpoint for ListPersonCredits {
        impl Request for ListPersonCreditsRequest;
        type Response<'de> = ListPersonCreditsResponse;
    }

    pub type AddPersonRemote;
    impl Endpoint for AddPersonRemote {
        impl Request for AddPersonRemoteRequest;
        type Response<'de> = Empty;
    }

    pub type RemovePersonRemote;
    impl Endpoint for RemovePersonRemote {
        impl Request for RemovePersonRemoteRequest;
        type Response<'de> = Empty;
    }

    pub type UpdatePersonRemote;
    impl Endpoint for UpdatePersonRemote {
        impl Request for UpdatePersonRemoteRequest;
        type Response<'de> = Empty;
    }

    pub type SetPersonRemoteEnabled;
    impl Endpoint for SetPersonRemoteEnabled {
        impl Request for SetPersonRemoteEnabledRequest;
        type Response<'de> = Empty;
    }

    pub type ReorderPersonRemotes;
    impl Endpoint for ReorderPersonRemotes {
        impl Request for ReorderPersonRemotesRequest;
        type Response<'de> = Empty;
    }

    pub type SetPersonRemoteSyncKinds;
    impl Endpoint for SetPersonRemoteSyncKinds {
        impl Request for SetPersonRemoteSyncKindsRequest;
        type Response<'de> = Empty;
    }

    pub type PurgePersonRemoteCache;
    impl Endpoint for PurgePersonRemoteCache {
        impl Request for PurgePersonRemoteCacheRequest;
        type Response<'de> = Empty;
    }

    pub type DeletePerson;
    impl Endpoint for DeletePerson {
        impl Request for DeletePersonRequest;
        type Response<'de> = Empty;
    }

    pub type GetSeasonImages;
    impl Endpoint for GetSeasonImages {
        impl Request for GetSeasonImagesRequest;
        type Response<'de> = GetSeasonImagesResponse;
    }

    pub type TrackShow;
    impl Endpoint for TrackShow {
        impl Request for TrackShowRequest;
        type Response<'de> = Show;
    }

    pub type UntrackShow;
    impl Endpoint for UntrackShow {
        impl Request for UntrackShowRequest;
        type Response<'de> = Empty;
    }

    pub type RemoveShow;
    impl Endpoint for RemoveShow {
        impl Request for RemoveShowRequest;
        type Response<'de> = Empty;
    }

    pub type ListEpisodes;
    impl Endpoint for ListEpisodes {
        impl Request for ListEpisodesRequest;
        type Response<'de> = ListEpisodesResponse;
    }

    pub type FindEpisodeByTimestamp;
    impl Endpoint for FindEpisodeByTimestamp {
        impl Request for FindEpisodeByTimestampRequest;
        type Response<'de> = FindEpisodeByTimestampResponse;
    }

    pub type GetEpisodeReleases;
    impl Endpoint for GetEpisodeReleases {
        impl Request for GetEpisodeReleasesRequest;
        type Response<'de> = GetEpisodeReleasesResponse;
    }

    pub type GetEpisodeCache;
    impl Endpoint for GetEpisodeCache {
        impl Request for GetEpisodeCacheRequest;
        type Response<'de> = GetEpisodeCacheResponse;
    }

    pub type PurgeEpisodeCache;
    impl Endpoint for PurgeEpisodeCache {
        impl Request for PurgeEpisodeCacheRequest;
        type Response<'de> = Empty;
    }

    pub type GetMovieReleases;
    impl Endpoint for GetMovieReleases {
        impl Request for GetMovieReleasesRequest;
        type Response<'de> = GetMovieReleasesResponse;
    }

    pub type GetMovie;
    impl Endpoint for GetMovie {
        impl Request for GetMovieRequest;
        type Response<'de> = Option<Movie>;
    }

    pub type TrackMovie;
    impl Endpoint for TrackMovie {
        impl Request for TrackMovieRequest;
        type Response<'de> = Movie;
    }

    pub type UntrackMovie;
    impl Endpoint for UntrackMovie {
        impl Request for UntrackMovieRequest;
        type Response<'de> = Empty;
    }

    pub type RemoveMovie;
    impl Endpoint for RemoveMovie {
        impl Request for RemoveMovieRequest;
        type Response<'de> = Empty;
    }

    pub type MarkWatched;
    impl Endpoint for MarkWatched {
        impl Request for MarkWatchedRequest;
        type Response<'de> = MarkWatchedResponse;
    }

    pub type MarkWatchedRemaining;
    impl Endpoint for MarkWatchedRemaining {
        impl Request for MarkWatchedRemainingRequest;
        type Response<'de> = Empty;
    }

    pub type RemoveWatched;
    impl Endpoint for RemoveWatched {
        impl Request for RemoveWatchedRequest;
        type Response<'de> = Empty;
    }

    pub type UndoWatched;
    impl Endpoint for UndoWatched {
        impl Request for UndoWatchedRequest;
        type Response<'de> = Empty;
    }

    pub type ListEpisodesWatched;
    impl Endpoint for ListEpisodesWatched {
        impl Request for ListEpisodesWatchedRequest;
        type Response<'de> = ListEpisodesWatchedResponse;
    }

    pub type ListWatched;
    impl Endpoint for ListWatched {
        impl Request for ListWatchedRequest;
        type Response<'de> = ListWatchedResponse;
    }

    pub type MoveWatchedEpisode;
    impl Endpoint for MoveWatchedEpisode {
        impl Request for MoveWatchedEpisodeRequest;
        type Response<'de> = Empty;
    }

    pub type ListOrphanedWatched;
    impl Endpoint for ListOrphanedWatched {
        impl Request for ListOrphanedWatchedRequest;
        type Response<'de> = ListOrphanedWatchedResponse;
    }

    pub type ListPending;
    impl Endpoint for ListPending {
        impl Request for ListPendingRequest;
        type Response<'de> = ListPendingResponse;
    }

    pub type ListSchedule;
    impl Endpoint for ListSchedule {
        impl Request for ListScheduleRequest<'_>;
        type Response<'de> = ListScheduleResponse;
    }

    pub type ListWatchNext;
    impl Endpoint for ListWatchNext {
        impl Request for ListWatchNextRequest;
        type Response<'de> = ListWatchNextResponse;
    }

    pub type Search;
    impl Endpoint for Search {
        impl Request for SearchRequest;
        type Response<'de> = SearchResponse;
    }

    pub type SyncShow;
    impl Endpoint for SyncShow {
        impl Request for SyncShowRequest;
        type Response<'de> = Empty;
    }

    pub type SyncMovie;
    impl Endpoint for SyncMovie {
        impl Request for SyncMovieRequest;
        type Response<'de> = Empty;
    }

    pub type SyncEpisode;
    impl Endpoint for SyncEpisode {
        impl Request for SyncEpisodeRequest;
        type Response<'de> = Empty;
    }

    pub type SyncPerson;
    impl Endpoint for SyncPerson {
        impl Request for SyncPersonRequest;
        type Response<'de> = Empty;
    }

    pub type SetShowRemoteEnabled;
    impl Endpoint for SetShowRemoteEnabled {
        impl Request for SetShowRemoteEnabledRequest;
        type Response<'de> = Empty;
    }

    pub type ReorderShowRemotes;
    impl Endpoint for ReorderShowRemotes {
        impl Request for ReorderShowRemotesRequest;
        type Response<'de> = Empty;
    }

    pub type SetMovieRemoteEnabled;
    impl Endpoint for SetMovieRemoteEnabled {
        impl Request for SetMovieRemoteEnabledRequest;
        type Response<'de> = Empty;
    }

    pub type ReorderMovieRemotes;
    impl Endpoint for ReorderMovieRemotes {
        impl Request for ReorderMovieRemotesRequest;
        type Response<'de> = Empty;
    }

    pub type SetShowRemoteSyncKinds;
    impl Endpoint for SetShowRemoteSyncKinds {
        impl Request for SetShowRemoteSyncKindsRequest;
        type Response<'de> = Empty;
    }

    pub type SetMovieRemoteSyncKinds;
    impl Endpoint for SetMovieRemoteSyncKinds {
        impl Request for SetMovieRemoteSyncKindsRequest;
        type Response<'de> = Empty;
    }

    pub type SetShowLanguage;
    impl Endpoint for SetShowLanguage {
        impl Request for SetShowLanguageRequest;
        type Response<'de> = Empty;
    }

    pub type SetShowIncludeSpecials;
    impl Endpoint for SetShowIncludeSpecials {
        impl Request for SetShowIncludeSpecialsRequest;
        type Response<'de> = Empty;
    }

    pub type SetShowAutoSync;
    impl Endpoint for SetShowAutoSync {
        impl Request for SetShowAutoSyncRequest;
        type Response<'de> = Empty;
    }

    pub type SetMovieAutoSync;
    impl Endpoint for SetMovieAutoSync {
        impl Request for SetMovieAutoSyncRequest;
        type Response<'de> = Empty;
    }

    pub type SetShowAirDateFilters;
    impl Endpoint for SetShowAirDateFilters {
        impl Request for SetShowAirDateFiltersRequest;
        type Response<'de> = Empty;
    }

    pub type SetMovieLanguage;
    impl Endpoint for SetMovieLanguage {
        impl Request for SetMovieLanguageRequest;
        type Response<'de> = Empty;
    }

    pub type SetMovieReleaseFilters;
    impl Endpoint for SetMovieReleaseFilters {
        impl Request for SetMovieReleaseFiltersRequest;
        type Response<'de> = Empty;
    }

    pub type AddShowRemote;
    impl Endpoint for AddShowRemote {
        impl Request for AddShowRemoteRequest;
        type Response<'de> = Empty;
    }

    pub type RemoveShowRemote;
    impl Endpoint for RemoveShowRemote {
        impl Request for RemoveShowRemoteRequest;
        type Response<'de> = Empty;
    }

    pub type AddMovieRemote;
    impl Endpoint for AddMovieRemote {
        impl Request for AddMovieRemoteRequest;
        type Response<'de> = Empty;
    }

    pub type RemoveMovieRemote;
    impl Endpoint for RemoveMovieRemote {
        impl Request for RemoveMovieRemoteRequest;
        type Response<'de> = Empty;
    }

    pub type PurgeShowRemoteCache;
    impl Endpoint for PurgeShowRemoteCache {
        impl Request for PurgeShowRemoteCacheRequest;
        type Response<'de> = Empty;
    }

    pub type PurgeMovieRemoteCache;
    impl Endpoint for PurgeMovieRemoteCache {
        impl Request for PurgeMovieRemoteCacheRequest;
        type Response<'de> = Empty;
    }

    pub type UpdateShowRemote;
    impl Endpoint for UpdateShowRemote {
        impl Request for UpdateShowRemoteRequest;
        type Response<'de> = Empty;
    }

    pub type UpdateMovieRemote;
    impl Endpoint for UpdateMovieRemote {
        impl Request for UpdateMovieRemoteRequest;
        type Response<'de> = Empty;
    }

    pub type SyncAll;
    impl Endpoint for SyncAll {
        impl Request for SyncAllRequest;
        type Response<'de> = Empty;
    }

    pub type ListTasks;
    impl Endpoint for ListTasks {
        impl Request for ListTasksRequest;
        type Response<'de> = ListTasksResponse;
    }

    pub type RemoveTask;
    impl Endpoint for RemoveTask {
        impl Request for RemoveTaskRequest;
        type Response<'de> = Empty;
    }

    pub type BumpTask;
    impl Endpoint for BumpTask {
        impl Request for BumpTaskRequest;
        type Response<'de> = Empty;
    }

    pub type GetConfig;
    impl Endpoint for GetConfig {
        impl Request for GetConfigRequest;
        type Response<'de> = GetConfigResponse;
    }

    pub type GetTopLanguages;
    impl Endpoint for GetTopLanguages {
        impl Request for GetTopLanguagesRequest;
        type Response<'de> = GetTopLanguagesResponse;
    }

    pub type SetConfig;
    impl Endpoint for SetConfig {
        impl Request for SetConfigRequest;
        type Response<'de> = Empty;
    }

    pub type AddPending;
    impl Endpoint for AddPending {
        impl Request for AddPendingRequest;
        type Response<'de> = AddPendingResponse;
    }

    pub type RemovePending;
    impl Endpoint for RemovePending {
        impl Request for RemovePendingRequest;
        type Response<'de> = Empty;
    }

    pub type SkipEpisode;
    impl Endpoint for SkipEpisode {
        impl Request for SkipEpisodeRequest;
        type Response<'de> = Empty;
    }

    pub type SelectImage;
    impl Endpoint for SelectImage {
        impl Request for SelectImageRequest;
        type Response<'de> = Empty;
    }

    pub type ClearSelectedImage;
    impl Endpoint for ClearSelectedImage {
        impl Request for ClearSelectedImageRequest;
        type Response<'de> = Empty;
    }

    pub type PickBestImages;
    impl Endpoint for PickBestImages {
        impl Request for PickBestImagesRequest;
        type Response<'de> = Empty;
    }

    pub type ResetImageSelection;
    impl Endpoint for ResetImageSelection {
        impl Request for ResetImageSelectionRequest;
        type Response<'de> = Empty;
    }

    pub type AppBroadcast;
    impl Broadcast for AppBroadcast {
        impl Event for AppEvent;
    }
}
