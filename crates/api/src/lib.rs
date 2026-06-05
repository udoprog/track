use core::fmt;

use jiff::Timestamp as JiffTimestamp;
use jiff::civil::Date as CivilDate;
use musli_core::{Context, Decode, Encode};
use musli_web::api::{self, ChannelId};

macro_rules! define_id {
    ($name:ident) => {
        #[derive(
            Debug,
            Copy,
            Clone,
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
        #[serde(transparent)]
        #[musli(crate = musli_core, transparent)]
        pub struct $name(u64);

        impl $name {
            #[inline]
            pub const fn new(id: u64) -> Self {
                Self(id)
            }

            #[inline]
            pub fn get(self) -> u64 {
                self.0
            }
        }

        impl fmt::Display for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                write!(f, "{:x}", self.0)
            }
        }

        #[cfg(feature = "sqll")]
        impl ::sqll::FromColumn<'_> for $name {
            type Type = ::sqll::ty::Integer;

            #[inline]
            fn from_column(
                stmt: &::sqll::Statement,
                index: ::sqll::ty::Integer,
            ) -> ::sqll::Result<Self> {
                let value = i64::from_column(stmt, index)?;
                Ok($name(value.cast_unsigned()))
            }
        }

        #[cfg(feature = "sqll")]
        impl ::sqll::BindValue for $name {
            #[inline]
            fn bind_value(
                &self,
                stmt: &mut ::sqll::Statement,
                index: ::sqll::Index,
            ) -> ::sqll::Result<()> {
                self.0.cast_signed().bind_value(stmt, index)
            }
        }
    };
}

define_id!(SeriesId);
define_id!(SeasonId);
define_id!(EpisodeId);
define_id!(MovieId);
define_id!(WatchedId);

/// RFC 3339 UTC-normalised timestamp stored as TEXT.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct Timestamp(JiffTimestamp);

impl Timestamp {
    pub fn now() -> Self {
        Self(JiffTimestamp::now())
    }

    pub fn inner(self) -> JiffTimestamp {
        self.0
    }
}

impl std::str::FromStr for Timestamp {
    type Err = jiff::Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        s.parse::<JiffTimestamp>().map(Timestamp)
    }
}

impl fmt::Display for Timestamp {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        // jiff::Timestamp displays as RFC 3339 UTC, e.g. "2024-01-15T10:30:00Z"
        self.0.fmt(f)
    }
}

impl<M> musli_core::Encode<M> for Timestamp {
    type Encode = Self;

    fn encode<E>(&self, encoder: E) -> Result<(), E::Error>
    where
        E: musli_core::Encoder<Mode = M>,
    {
        encoder.collect_string(self)
    }

    fn as_encode(&self) -> &Self::Encode {
        self
    }
}

impl<'de, M, A> musli_core::Decode<'de, M, A> for Timestamp
where
    A: musli_core::Allocator,
{
    fn decode<D>(decoder: D) -> Result<Self, D::Error>
    where
        D: musli_core::Decoder<'de, Mode = M, Allocator = A>,
    {
        let cx = decoder.cx();
        decoder
            .decode_unsized(|s: &str| s.parse::<JiffTimestamp>().map(Timestamp).map_err(cx.map()))
    }
}

#[cfg(feature = "sqll")]
impl ::sqll::FromColumn<'_> for Timestamp {
    type Type = ::sqll::ty::Integer;

    #[inline]
    fn from_column(stmt: &::sqll::Statement, index: ::sqll::ty::Integer) -> ::sqll::Result<Self> {
        let ms = i64::from_column(stmt, index)?;
        JiffTimestamp::from_millisecond(ms)
            .map(Timestamp)
            .map_err(|e| ::sqll::Error::custom(format!("invalid timestamp ms {ms}: {e}")))
    }
}

#[cfg(feature = "sqll")]
impl ::sqll::BindValue for Timestamp {
    #[inline]
    fn bind_value(&self, stmt: &mut ::sqll::Statement, index: ::sqll::Index) -> ::sqll::Result<()> {
        self.0.as_millisecond().bind_value(stmt, index)
    }
}

/// Calendar date stored as TEXT "YYYY-MM-DD".
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct Date(CivilDate);

impl Date {
    pub fn today() -> Self {
        Self(jiff::Zoned::now().date())
    }

    pub fn inner(self) -> CivilDate {
        self.0
    }

    pub fn year(self) -> i16 {
        self.0.year()
    }

    pub fn checked_add_days(self, days: i32) -> Self {
        Self(
            self.0
                .checked_add(jiff::Span::new().days(days))
                .unwrap_or(self.0),
        )
    }
}

impl std::str::FromStr for Date {
    type Err = jiff::Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        s.parse::<CivilDate>().map(Date)
    }
}

impl fmt::Display for Date {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        // jiff::civil::Date displays as ISO 8601, e.g. "2024-01-15"
        self.0.fmt(f)
    }
}

impl<M> musli_core::Encode<M> for Date {
    type Encode = Self;

    fn encode<E>(&self, encoder: E) -> Result<(), E::Error>
    where
        E: musli_core::Encoder<Mode = M>,
    {
        encoder.collect_string(self)
    }

    fn as_encode(&self) -> &Self::Encode {
        self
    }
}

impl<'de, M, A> musli_core::Decode<'de, M, A> for Date
where
    A: musli_core::Allocator,
{
    fn decode<D>(decoder: D) -> Result<Self, D::Error>
    where
        D: musli_core::Decoder<'de, Mode = M, Allocator = A>,
    {
        let cx = decoder.cx();
        decoder.decode_unsized(|s: &str| s.parse::<CivilDate>().map(Date).map_err(cx.map()))
    }
}

impl serde::Serialize for Date {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.collect_str(self)
    }
}

impl<'de> serde::Deserialize<'de> for Date {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let s = String::deserialize(deserializer)?;
        s.parse::<CivilDate>()
            .map(Date)
            .map_err(serde::de::Error::custom)
    }
}

#[cfg(feature = "sqll")]
impl ::sqll::FromColumn<'_> for Date {
    type Type = ::sqll::ty::Integer;

    #[inline]
    fn from_column(stmt: &::sqll::Statement, index: ::sqll::ty::Integer) -> ::sqll::Result<Self> {
        let n = i64::from_column(stmt, index)?;
        let year = (n / 10000) as i16;
        let month = ((n / 100) % 100) as i8;
        let day = (n % 100) as i8;

        CivilDate::new(year, month, day)
            .map(Date)
            .map_err(|e| ::sqll::Error::custom(format!("invalid date integer {n}: {e}")))
    }
}

#[cfg(feature = "sqll")]
impl ::sqll::BindValue for Date {
    #[inline]
    fn bind_value(&self, stmt: &mut ::sqll::Statement, index: ::sqll::Index) -> ::sqll::Result<()> {
        let d = self.0;
        let n = d.year() as i64 * 10000 + d.month() as i64 * 100 + d.day() as i64;
        n.bind_value(stmt, index)
    }
}

/// Remote identifier: "tvdb:123", "tmdb:456", "imdb:tt0001234".
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode, serde::Serialize, serde::Deserialize)]
#[musli(crate = musli_core, transparent)]
#[serde(transparent)]
pub struct RemoteId(String);

impl RemoteId {
    pub fn tvdb(id: u32) -> Self {
        Self(format!("tvdb:{id}"))
    }

    pub fn tmdb(id: u32) -> Self {
        Self(format!("tmdb:{id}"))
    }

    pub fn imdb(s: &str) -> Self {
        Self(format!("imdb:{s}"))
    }

    pub fn from_raw(s: impl Into<String>) -> Self {
        Self(s.into())
    }

    pub fn source(&self) -> &str {
        self.0.split_once(':').map(|(s, _)| s).unwrap_or("")
    }

    pub fn value(&self) -> &str {
        self.0.split_once(':').map(|(_, v)| v).unwrap_or(&self.0)
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for RemoteId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

#[cfg(feature = "sqll")]
impl ::sqll::FromColumn<'_> for RemoteId {
    type Type = ::sqll::ty::Text;

    #[inline]
    fn from_column(stmt: &::sqll::Statement, index: ::sqll::ty::Text) -> ::sqll::Result<Self> {
        Ok(RemoteId(String::from_column(stmt, index)?))
    }
}

#[cfg(feature = "sqll")]
impl ::sqll::BindValue for RemoteId {
    #[inline]
    fn bind_value(&self, stmt: &mut ::sqll::Statement, index: ::sqll::Index) -> ::sqll::Result<()> {
        self.0.as_str().bind_value(stmt, index)
    }
}

/// Image reference: "tvdb:/banners/abc.jpg", "tmdb:/xy.jpg".
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode, serde::Serialize, serde::Deserialize)]
#[musli(crate = musli_core, transparent)]
#[serde(transparent)]
pub struct Image(String);

impl Image {
    pub fn tvdb(path: &str) -> Self {
        let path = path.trim_start_matches('/');
        Self(format!("tvdb:{path}"))
    }

    pub fn tmdb(path: &str) -> Self {
        let path = path.trim_start_matches('/');
        Self(format!("tmdb:{path}"))
    }

    pub fn from_raw(s: impl Into<String>) -> Self {
        Self(s.into())
    }

    pub fn source(&self) -> &str {
        self.0.split_once(':').map(|(s, _)| s).unwrap_or("")
    }

    pub fn path(&self) -> &str {
        self.0.split_once(':').map(|(_, p)| p).unwrap_or("")
    }

    pub fn proxy_url(&self) -> String {
        format!("/api/image/{}/{}", self.source(), self.path())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for Image {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

#[cfg(feature = "sqll")]
impl ::sqll::FromColumn<'_> for Image {
    type Type = ::sqll::ty::Text;

    #[inline]
    fn from_column(stmt: &::sqll::Statement, index: ::sqll::ty::Text) -> ::sqll::Result<Self> {
        Ok(Image(String::from_column(stmt, index)?))
    }
}

#[cfg(feature = "sqll")]
impl ::sqll::BindValue for Image {
    #[inline]
    fn bind_value(&self, stmt: &mut ::sqll::Statement, index: ::sqll::Index) -> ::sqll::Result<()> {
        self.0.as_str().bind_value(stmt, index)
    }
}

/// Season number: Specials (stored as 0) or a regular numbered season.
#[derive(
    Debug,
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
    Number(u32),
}

impl SeasonNumber {
    pub fn to_i64(self) -> i64 {
        match self {
            SeasonNumber::Specials => 0,
            SeasonNumber::Number(n) => n as i64,
        }
    }

    pub fn from_i64(n: i64) -> Self {
        if n == 0 {
            SeasonNumber::Specials
        } else {
            SeasonNumber::Number(n as u32)
        }
    }

    pub fn is_special(&self) -> bool {
        matches!(self, SeasonNumber::Specials)
    }

    pub fn short(&self) -> String {
        match self {
            SeasonNumber::Specials => "S".to_string(),
            SeasonNumber::Number(n) => n.to_string(),
        }
    }
}

impl Default for SeasonNumber {
    fn default() -> Self {
        SeasonNumber::Specials
    }
}

impl fmt::Display for SeasonNumber {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            SeasonNumber::Specials => write!(f, "Specials"),
            SeasonNumber::Number(n) => write!(f, "Season {n}"),
        }
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

// ── Core data types ──────────────────────────────────────────────────────────

#[derive(Debug, Clone, Encode, Decode)]
#[musli(crate = musli_core)]
pub struct Series {
    pub id: SeriesId,
    pub title: String,
    pub first_air_date: Option<Date>,
    pub overview: String,
    pub poster: Option<Image>,
    pub banner: Option<Image>,
    pub fanart: Option<Image>,
    pub tracked: bool,
    pub remote_id: Option<RemoteId>,
}

#[derive(Debug, Clone, Encode, Decode)]
#[musli(crate = musli_core)]
pub struct Season {
    pub id: SeasonId,
    pub series_id: SeriesId,
    pub number: SeasonNumber,
    pub air_date: Option<Date>,
    pub name: Option<String>,
    pub overview: String,
    pub poster: Option<Image>,
}

#[derive(Debug, Clone, Encode, Decode)]
#[musli(crate = musli_core)]
pub struct Episode {
    pub id: EpisodeId,
    pub series_id: SeriesId,
    pub season: SeasonNumber,
    pub number: u32,
    pub absolute_number: Option<u32>,
    pub name: Option<String>,
    pub overview: String,
    pub aired: Option<Date>,
    pub filename: Option<Image>,
    pub remote_id: Option<RemoteId>,
    pub watched: bool,
    pub watched_count: u32,
}

#[derive(Debug, Clone, Encode, Decode)]
#[musli(crate = musli_core)]
pub struct Movie {
    pub id: MovieId,
    pub title: String,
    pub release_date: Option<Date>,
    pub overview: String,
    pub poster: Option<Image>,
    pub banner: Option<Image>,
    pub fanart: Option<Image>,
    pub remote_id: Option<RemoteId>,
    pub watched: bool,
    pub watched_count: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Encode, Decode)]
#[musli(crate = musli_core)]
pub enum WatchedKind {
    Episode {
        series: SeriesId,
        episode: EpisodeId,
    },
    Movie {
        movie: MovieId,
    },
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
pub enum PendingKind {
    Episode {
        series: SeriesId,
        episode: EpisodeId,
    },
    Movie {
        movie: MovieId,
    },
}

/// Denormalized pending item for dashboard/queue rendering.
#[derive(Debug, Clone, Encode, Decode)]
#[musli(crate = musli_core)]
pub struct Pending {
    pub kind: PendingKind,
    pub aired: Option<Date>,
    pub series_title: Option<String>,
    pub label: String,
    pub poster: Option<Image>,
}

#[derive(Debug, Clone, Encode, Decode)]
#[musli(crate = musli_core)]
pub struct ScheduledEntry {
    pub series_id: SeriesId,
    pub series_title: String,
    pub episodes: Vec<Episode>,
}

#[derive(Debug, Clone, Encode, Decode)]
#[musli(crate = musli_core)]
pub struct ScheduledDay {
    pub date: Date,
    pub entries: Vec<ScheduledEntry>,
}

#[derive(Debug, Clone, Encode, Decode)]
#[musli(crate = musli_core)]
pub struct Config {
    pub theme: ThemeType,
    pub tvdb_legacy_apikey: String,
    pub tmdb_api_key: String,
    pub schedule_duration_days: u32,
    pub dashboard_limit: u32,
    pub dashboard_page: u32,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            theme: ThemeType::Dark,
            tvdb_legacy_apikey: String::new(),
            tmdb_api_key: String::new(),
            schedule_duration_days: 7,
            dashboard_limit: 6,
            dashboard_page: 6,
        }
    }
}

#[derive(Debug, Encode, Decode)]
#[musli(crate = musli_core)]
pub struct Empty;

// ── Search types ─────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq, Encode, Decode)]
#[musli(crate = musli_core)]
pub enum SearchKind {
    Series,
    Movies,
}

#[derive(Debug, Clone, Encode, Decode)]
#[musli(crate = musli_core)]
pub struct SearchSeries {
    pub remote_id: RemoteId,
    pub title: String,
    pub poster: Option<Image>,
    pub overview: String,
    pub first_air_date: Option<Date>,
    pub already_tracked: Option<SeriesId>,
}

#[derive(Debug, Clone, Encode, Decode)]
#[musli(crate = musli_core)]
pub struct SearchMovie {
    pub remote_id: RemoteId,
    pub title: String,
    pub poster: Option<Image>,
    pub overview: String,
    pub release_date: Option<Date>,
    pub already_tracked: Option<MovieId>,
}

// ── Request / Response structs ───────────────────────────────────────────────

#[derive(Debug, Encode, Decode)]
#[musli(crate = musli_core)]
pub struct ListSeriesRequest;

#[derive(Debug, Encode, Decode)]
#[musli(crate = musli_core)]
pub struct ListSeriesResponse {
    pub series: Vec<Series>,
}

#[derive(Debug, Encode, Decode)]
#[musli(crate = musli_core)]
pub struct GetSeriesRequest {
    pub id: SeriesId,
}

#[derive(Debug, Encode, Decode)]
#[musli(crate = musli_core)]
pub struct ListSeasonsRequest {
    pub series_id: SeriesId,
}

#[derive(Debug, Encode, Decode)]
#[musli(crate = musli_core)]
pub struct ListSeasonsResponse {
    pub seasons: Vec<Season>,
}

#[derive(Debug, Encode, Decode)]
#[musli(crate = musli_core)]
pub struct TrackSeriesRequest {
    pub remote_id: RemoteId,
}

#[derive(Debug, Encode, Decode)]
#[musli(crate = musli_core)]
pub struct UntrackSeriesRequest {
    pub id: SeriesId,
}

#[derive(Debug, Encode, Decode)]
#[musli(crate = musli_core)]
pub struct RemoveSeriesRequest {
    pub id: SeriesId,
}

#[derive(Debug, Encode, Decode)]
#[musli(crate = musli_core)]
pub struct ListEpisodesRequest {
    pub series_id: SeriesId,
    pub season: SeasonNumber,
}

#[derive(Debug, Encode, Decode)]
#[musli(crate = musli_core)]
pub struct ListEpisodesResponse {
    pub episodes: Vec<Episode>,
}

#[derive(Debug, Encode, Decode)]
#[musli(crate = musli_core)]
pub struct ListMoviesRequest;

#[derive(Debug, Encode, Decode)]
#[musli(crate = musli_core)]
pub struct ListMoviesResponse {
    pub movies: Vec<Movie>,
}

#[derive(Debug, Encode, Decode)]
#[musli(crate = musli_core)]
pub struct GetMovieRequest {
    pub id: MovieId,
}

#[derive(Debug, Encode, Decode)]
#[musli(crate = musli_core)]
pub struct TrackMovieRequest {
    pub remote_id: RemoteId,
}

#[derive(Debug, Encode, Decode)]
#[musli(crate = musli_core)]
pub struct RemoveMovieRequest {
    pub id: MovieId,
}

#[derive(Debug, Encode, Decode)]
#[musli(crate = musli_core)]
pub struct MarkWatchedRequest {
    pub kind: WatchedKind,
    pub timestamp: Option<Timestamp>,
}

#[derive(Debug, Encode, Decode)]
#[musli(crate = musli_core)]
pub struct MarkWatchedResponse {
    pub watched: Watched,
}

#[derive(Debug, Encode, Decode)]
#[musli(crate = musli_core)]
pub struct RemoveWatchedRequest {
    pub id: WatchedId,
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
pub struct ListScheduleRequest {
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
    pub kind: SearchKind,
    pub query: String,
}

#[derive(Debug, Encode, Decode)]
#[musli(crate = musli_core)]
pub struct SearchResponse {
    pub series: Vec<SearchSeries>,
    pub movies: Vec<SearchMovie>,
}

#[derive(Debug, Encode, Decode)]
#[musli(crate = musli_core)]
pub struct SyncSeriesRequest {
    pub id: SeriesId,
}

#[derive(Debug, Encode, Decode)]
#[musli(crate = musli_core)]
pub struct SyncAllRequest;

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
pub struct SetConfigRequest {
    pub config: Config,
}

// ── Broadcast events ─────────────────────────────────────────────────────────

#[derive(Debug, Clone, Encode, Decode)]
#[musli(crate = musli_core)]
pub struct AppEvent {
    pub channel: ChannelId,
    pub kind: AppEventKind,
}

#[derive(Debug, Clone, Encode, Decode)]
#[musli(crate = musli_core)]
pub enum AppEventKind {
    SeriesCreated {
        series: Series,
    },
    SeriesChanged {
        series: Series,
    },
    SeriesDeleted {
        series_id: SeriesId,
    },
    SeasonsChanged {
        series_id: SeriesId,
        seasons: Vec<Season>,
    },
    EpisodeChanged {
        episode: Episode,
    },
    EpisodesChanged {
        series_id: SeriesId,
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
    WatchedChanged {
        kind: WatchedKind,
    },
    PendingChanged,
    ConfigChanged {
        config: Config,
    },
    SyncStarted {
        series_id: Option<SeriesId>,
    },
    SyncFinished {
        series_id: Option<SeriesId>,
    },
}

// ── Endpoint definitions ─────────────────────────────────────────────────────

api::define! {
    pub type ListSeries;
    impl Endpoint for ListSeries {
        impl Request for ListSeriesRequest;
        type Response<'de> = ListSeriesResponse;
    }

    pub type GetSeries;
    impl Endpoint for GetSeries {
        impl Request for GetSeriesRequest;
        type Response<'de> = Series;
    }

    pub type ListSeasons;
    impl Endpoint for ListSeasons {
        impl Request for ListSeasonsRequest;
        type Response<'de> = ListSeasonsResponse;
    }

    pub type TrackSeries;
    impl Endpoint for TrackSeries {
        impl Request for TrackSeriesRequest;
        type Response<'de> = Series;
    }

    pub type UntrackSeries;
    impl Endpoint for UntrackSeries {
        impl Request for UntrackSeriesRequest;
        type Response<'de> = Empty;
    }

    pub type RemoveSeries;
    impl Endpoint for RemoveSeries {
        impl Request for RemoveSeriesRequest;
        type Response<'de> = Empty;
    }

    pub type ListEpisodes;
    impl Endpoint for ListEpisodes {
        impl Request for ListEpisodesRequest;
        type Response<'de> = ListEpisodesResponse;
    }

    pub type ListMovies;
    impl Endpoint for ListMovies {
        impl Request for ListMoviesRequest;
        type Response<'de> = ListMoviesResponse;
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

    pub type RemoveWatched;
    impl Endpoint for RemoveWatched {
        impl Request for RemoveWatchedRequest;
        type Response<'de> = Empty;
    }

    pub type ListWatched;
    impl Endpoint for ListWatched {
        impl Request for ListWatchedRequest;
        type Response<'de> = ListWatchedResponse;
    }

    pub type ListPending;
    impl Endpoint for ListPending {
        impl Request for ListPendingRequest;
        type Response<'de> = ListPendingResponse;
    }

    pub type ListSchedule;
    impl Endpoint for ListSchedule {
        impl Request for ListScheduleRequest;
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

    pub type SyncSeries;
    impl Endpoint for SyncSeries {
        impl Request for SyncSeriesRequest;
        type Response<'de> = Empty;
    }

    pub type SyncAll;
    impl Endpoint for SyncAll {
        impl Request for SyncAllRequest;
        type Response<'de> = Empty;
    }

    pub type GetConfig;
    impl Endpoint for GetConfig {
        impl Request for GetConfigRequest;
        type Response<'de> = GetConfigResponse;
    }

    pub type SetConfig;
    impl Endpoint for SetConfig {
        impl Request for SetConfigRequest;
        type Response<'de> = Empty;
    }

    pub type AppBroadcast;
    impl Broadcast for AppBroadcast {
        impl Event for AppEvent;
    }
}
