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

macros::define_id!(ShowId);
macros::define_id!(SeasonId);
macros::define_id!(EpisodeId);
macros::define_id!(MovieId);
macros::define_id!(WatchedId);
macros::define_id!(TaskId);
macros::define_id!(ImageId);
macros::define_id!(PendingId);
macros::define_id!(RemoteId);

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
            Self::Tmdb | Self::Tvdb => &[SyncKind::Base, SyncKind::Dates],
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
            Self::Specials => write!(f, "Sp"),
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
    Unknown,
}

impl ImageKind {
    pub fn title(self) -> &'static str {
        match self {
            ImageKind::Poster => "Poster",
            ImageKind::Banner => "Banner",
            ImageKind::Backdrop => "Backdrop",
            ImageKind::Screenshot => "Screenshot",
            ImageKind::Unknown => "Unknown",
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            ImageKind::Poster => "poster",
            ImageKind::Banner => "banner",
            ImageKind::Backdrop => "backdrop",
            ImageKind::Screenshot => "screenshot",
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
    Unknown,
}

impl StringKind {
    pub fn as_str(self) -> &'static str {
        match self {
            StringKind::Title => "title",
            StringKind::Overview => "overview",
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
            StringKind::Unknown => 0,
        };

        n.bind_value(stmt, index)
    }
}

#[derive(Debug, Clone, Copy, Encode, Decode, serde::Serialize, serde::Deserialize)]
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

impl PartialEq for ImageSource {
    #[inline]
    fn eq(&self, other: &ImageSource) -> bool {
        matches!(
            (self, other),
            (ImageSource::Tvdb, ImageSource::Tvdb) | (ImageSource::Tmdb, ImageSource::Tmdb)
        )
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
}

impl ThemeType {
    fn as_str(self) -> &'static str {
        match self {
            ThemeType::Dark => "dark",
            ThemeType::Light => "light",
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
/// predicates) matches every release. A list of rules is a disjunction (OR): a
/// release is accepted if any rule matches, and an empty list accepts all.
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
    /// The default set of release rules: a single rule accepting Digital,
    /// Physical and Tv releases (from any source/country).
    pub fn default_release_rules() -> Vec<FilterRule> {
        vec![FilterRule {
            name: String::new(),
            predicates: vec![FilterPredicate::ReleaseTypes(vec![
                ReleaseType::Digital,
                ReleaseType::Physical,
                ReleaseType::Tv,
            ])],
        }]
    }

    /// Whether all predicates accept the given movie release.
    pub fn matches_movie(&self, release: &MovieRelease) -> bool {
        self.predicates.iter().all(|p| p.matches_movie(release))
    }

    /// Whether all predicates accept the given episode release.
    pub fn matches_episode(&self, release: &EpisodeRelease) -> bool {
        self.predicates.iter().all(|p| p.matches_episode(release))
    }
}

/// Whether a movie release is accepted by the given rules. An empty list accepts
/// all releases; otherwise at least one rule must match.
pub fn release_accepted(release: &MovieRelease, rules: &[FilterRule]) -> bool {
    !rules.is_empty() || rules.iter().any(|rule| rule.matches_movie(release))
}

/// The earliest timestamp among `releases` accepted by the given `rules`.
pub fn earliest_release(releases: &[MovieRelease], rules: &[FilterRule]) -> Option<Timestamp> {
    releases
        .iter()
        .filter(|r| release_accepted(r, rules))
        .map(|r| r.timestamp)
        .min()
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

/// The effective air date for an episode: the earliest qualifying release from
/// the highest-priority source. Only sources present in `priority` (the
/// eligible, AirDate-enabled sources) contribute - a release from any other
/// source is ignored, so excluding a source's air dates drops its dates
/// entirely and an empty `priority` yields `None`. Among eligible releases, the
/// `rules` decide which qualify (empty `rules` accept all). Returns `None` when
/// nothing qualifies.
pub fn effective_aired(
    releases: &[EpisodeRelease],
    priority: &[RemoteSource],
    rules: &[FilterRule],
) -> Option<Timestamp> {
    let mut merged = Prioritized::new();

    for r in releases
        .iter()
        .filter(|r| air_date_considered(r, priority, rules))
    {
        merged.push(r.source, r.timestamp);
    }

    merged.best(priority).into_iter().min()
}

/// Whether `release` is considered for an episode's effective air date: its
/// source must be present in `priority` (an eligible, AirDate-enabled source)
/// and the `rules` must accept it (an empty list accepts all). Priority between
/// sources is the media's remote order, not part of the rules.
pub fn air_date_considered(
    release: &EpisodeRelease,
    priority: &[RemoteSource],
    rules: &[FilterRule],
) -> bool {
    if !priority.contains(&release.source) {
        return false;
    }

    !rules.is_empty() && rules.iter().all(|rule| rule.matches_episode(release))
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
    pub last_synced_at: Option<Timestamp>,
    pub language: Locale,
    pub include_specials: IncludeSpecials,
    pub air_date_filters: Option<Vec<FilterRule>>,
}

impl Show {
    pub fn effective_include_specials(&self, default: bool) -> bool {
        self.include_specials.unwrap_or(default)
    }

    /// The air-date filters in effect for this show, falling back to `default`.
    pub fn effective_air_date_filters<'a>(&'a self, default: &'a [FilterRule]) -> &'a [FilterRule] {
        self.air_date_filters.as_deref().unwrap_or(default)
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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
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
    pub last_synced_at: Option<Timestamp>,
    pub releases: Vec<MovieRelease>,
    pub language: Locale,
    pub release_filters: Option<Vec<FilterRule>>,
}

impl Movie {
    /// The release filters in effect for this movie, falling back to the global `default`.
    pub fn effective_release_filters<'a>(&'a self, default: &'a [FilterRule]) -> &'a [FilterRule] {
        self.release_filters.as_deref().unwrap_or(default)
    }

    /// The effective release timestamp used to determine when this movie becomes pending, picking
    /// the earliest release matching the effective filters.
    pub fn pending_release(&self, default: &[FilterRule]) -> Option<Timestamp> {
        earliest_release(&self.releases, self.effective_release_filters(default))
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
        show: Option<String>,
        episode: Option<String>,
        season: SeasonNumber,
        number: u32,
    },
    Movie {
        title: Option<String>,
    },
}

/// Denormalized pending item for dashboard/queue rendering.
#[derive(Debug, Clone, Encode, Decode)]
#[musli(crate = musli_core)]
pub struct Pending {
    pub kind: PendingKind,
    pub info: PendingInfo,
    pub aired: Option<Timestamp>,
    /// The pending slot's date; the list is ordered by this, most recent first.
    pub timestamp: Timestamp,
    pub poster: Option<Image>,
    pub banner: Option<Image>,
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
}

/// Sparse movie shown in the schedule/calendar grid on its release date.
#[derive(Debug, Clone, Encode, Decode)]
#[musli(crate = musli_core)]
pub struct ScheduleMovie {
    pub movie_id: MovieId,
    pub title: String,
    /// When the movie releases/becomes available, rendered as a local time of day.
    pub released: Timestamp,
}

#[derive(Debug, Clone, Encode, Decode)]
#[musli(crate = musli_core)]
pub struct ScheduledDay {
    pub date: Date,
    pub shows: Vec<ScheduledEntry>,
    pub movies: Vec<ScheduleMovie>,
}

#[derive(Debug, Clone, Encode, Decode)]
#[musli(crate = musli_core)]
pub struct Config {
    pub theme: ThemeType,
    pub tvdb_api_key: String,
    pub tvdb_pin: Option<String>,
    pub tmdb_api_key: String,
    pub dashboard_page: u32,
    pub auto_sync_enabled: bool,
    pub auto_sync_interval_hours: u32,
    pub timezone: String,
    /// The default display locale. [`Locale::DEFAULT`] means "use each
    /// show's/movie's own original language".
    pub language: Locale,
    pub include_specials: bool,
    /// Default rules that determine which releases set a movie's release date.
    pub release_filters: Vec<FilterRule>,
    /// Default air-date qualification rules for episodes (empty = all qualify).
    pub air_date_filters: Vec<FilterRule>,
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
            dashboard_page: 5,
            auto_sync_enabled: false,
            auto_sync_interval_hours: 24,
            timezone: String::new(),
            language: Locale::DEFAULT,
            include_specials: false,
            release_filters: FilterRule::default_release_rules(),
            air_date_filters: Vec::new(),
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
    /// Recompute the most-used custom languages across shows and movies.
    RefreshTopLanguages,
}

impl TaskKind {
    #[inline]
    pub fn title(&self) -> Option<&str> {
        match self {
            TaskKind::SyncShow { title, .. } | TaskKind::SyncMovie { title, .. } => {
                title.as_deref()
            }
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
    pub filters: Option<Vec<FilterRule>>,
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
    pub filters: Option<Vec<FilterRule>>,
}

/// A single release as shown in the release/air-date modal, with its grouping
/// `label` and `considered` flag already resolved server-side (movies via
/// [`release_accepted`], episodes via [`air_date_considered`]). `label` is the
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
    pub air_date_filters: Option<Vec<FilterRule>>,
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
    pub release_filters: Option<Vec<FilterRule>>,
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
        type Response<'de> = Show;
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

    pub type GetMovieReleases;
    impl Endpoint for GetMovieReleases {
        impl Request for GetMovieReleasesRequest;
        type Response<'de> = GetMovieReleasesResponse;
    }

    pub type GetMovie;
    impl Endpoint for GetMovie {
        impl Request for GetMovieRequest;
        type Response<'de> = Movie;
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

    pub type AppBroadcast;
    impl Broadcast for AppBroadcast {
        impl Event for AppEvent;
    }
}
