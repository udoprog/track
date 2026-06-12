use core::fmt;
use core::num::NonZero;
use core::str::FromStr;

use jiff::Timestamp as JiffTimestamp;
use jiff::civil::Date as JiffDate;
use jiff::tz::TimeZone as JiffTimeZone;
use musli_core::{Context, Decode, Encode};
use musli_web::api::{self, ChannelId};

macro_rules! define_id {
    ($name:ident) => {
        #[derive(
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

            #[cfg(feature = "rand")]
            #[inline]
            pub fn random() -> Self {
                Self(rand::random())
            }
        }

        impl ::core::fmt::Display for $name {
            fn fmt(&self, f: &mut ::core::fmt::Formatter<'_>) -> ::core::fmt::Result {
                let bytes = self.0.to_be_bytes();

                let d = ::base64::display::Base64Display::new(
                    &bytes,
                    &::base64::engine::general_purpose::URL_SAFE_NO_PAD,
                );

                d.fmt(f)
            }
        }

        impl ::core::fmt::Debug for $name {
            #[inline]
            fn fmt(&self, f: &mut ::core::fmt::Formatter<'_>) -> ::core::fmt::Result {
                ::core::fmt::Display::fmt(self, f)
            }
        }

        impl ::core::str::FromStr for $name {
            type Err = ::base64::DecodeSliceError;

            fn from_str(s: &str) -> Result<Self, Self::Err> {
                use ::base64::Engine as _;
                let mut bytes = [0u8; 8];

                ::base64::engine::general_purpose::URL_SAFE_NO_PAD
                    .decode_slice(s.as_bytes(), &mut bytes)?;

                Ok($name(u64::from_be_bytes(bytes)))
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
define_id!(MovieReleaseId);
define_id!(WatchedId);
define_id!(TaskId);
define_id!(ImageId);
define_id!(PendingId);

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TimeZone(JiffTimeZone);

impl TimeZone {
    /// The UTC TimeZone.
    pub const UTC: Self = Self(JiffTimeZone::UTC);

    #[inline]
    pub fn from_jiff(tz: JiffTimeZone) -> Self {
        Self(tz)
    }

    #[inline]
    pub fn into_jiff(self) -> JiffTimeZone {
        self.0
    }

    #[inline]
    pub fn get(s: &str) -> Option<Self> {
        Some(Self(JiffTimeZone::get(s).ok()?))
    }

    #[inline]
    pub fn iana_name(&self) -> Option<String> {
        self.0.iana_name().map(|s| s.to_owned())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct Timestamp(JiffTimestamp);

impl Timestamp {
    #[inline]
    pub fn now() -> Self {
        Self(JiffTimestamp::now())
    }

    #[inline]
    pub fn inner(self) -> JiffTimestamp {
        self.0
    }

    #[inline]
    pub fn from_jiff(ts: JiffTimestamp) -> Self {
        Self(ts)
    }

    /// Format this timestamp in the given timezone as `"YYYY-MM-DD HH:MM TZ"`.
    /// The timezone suffix is the IANA abbreviation (e.g. `CEST`, `EST`) when
    /// available, or the numeric offset (e.g. `+05:30`) for fixed-offset zones.
    #[inline]
    pub fn display(&self, tz: TimeZone) -> String {
        self.0
            .to_zoned(tz.0)
            .strftime("%Y-%m-%d %H:%M %Z")
            .to_string()
    }

    #[inline]
    pub fn date(&self, tz: TimeZone) -> Date {
        Date(self.0.to_zoned(tz.0).date())
    }
}

impl FromStr for Timestamp {
    type Err = jiff::Error;

    #[inline]
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(Self(s.parse::<JiffTimestamp>()?))
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

/// Day of the week, Monday-anchored.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[repr(u32)]
pub enum Weekday {
    Monday,
    Tuesday,
    Wednesday,
    Thursday,
    Friday,
    Saturday,
    Sunday,
}

impl Weekday {
    pub const ALL: [Weekday; 7] = [
        Weekday::Monday,
        Weekday::Tuesday,
        Weekday::Wednesday,
        Weekday::Thursday,
        Weekday::Friday,
        Weekday::Saturday,
        Weekday::Sunday,
    ];

    /// Days since Monday (0 = Monday … 6 = Sunday).
    pub fn from_monday(self) -> u32 {
        self as u32
    }

    pub fn short_name(self) -> &'static str {
        match self {
            Weekday::Monday => "Mon",
            Weekday::Tuesday => "Tue",
            Weekday::Wednesday => "Wed",
            Weekday::Thursday => "Thu",
            Weekday::Friday => "Fri",
            Weekday::Saturday => "Sat",
            Weekday::Sunday => "Sun",
        }
    }
}

#[derive(Debug)]
enum InnerDateError {
    ToUtc(jiff::Error),
}

pub struct DateError {
    inner: InnerDateError,
}

impl From<InnerDateError> for DateError {
    #[inline]
    fn from(inner: InnerDateError) -> Self {
        Self { inner }
    }
}

impl core::error::Error for DateError {
    #[inline]
    fn source(&self) -> Option<&(dyn core::error::Error + 'static)> {
        match self.inner {
            InnerDateError::ToUtc(ref e) => Some(e),
        }
    }
}

impl fmt::Display for DateError {
    #[inline]
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.inner {
            InnerDateError::ToUtc(..) => write!(f, "date to utc error"),
        }
    }
}

impl fmt::Debug for DateError {
    #[inline]
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.inner.fmt(f)
    }
}

/// Calendar date stored as TEXT "YYYY-MM-DD".
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Date(JiffDate);

impl Date {
    pub fn new(year: i16, month: i8, day: i8) -> Option<Self> {
        Some(Self(JiffDate::new(year, month, day).ok()?))
    }

    pub fn to_timestamp_at_midnight_utc(self) -> Result<Timestamp, DateError> {
        self.to_timestamp_at_midnight_zoned(TimeZone::UTC)
    }

    pub fn to_timestamp_at_midnight_zoned(self, tz: TimeZone) -> Result<Timestamp, DateError> {
        let zoned = self
            .0
            .at(0, 0, 0, 0)
            .to_zoned(tz.into_jiff())
            .map_err(InnerDateError::ToUtc)?;

        Ok(Timestamp(zoned.timestamp()))
    }

    pub fn today() -> Self {
        Self(jiff::Zoned::now().date())
    }

    pub fn inner(self) -> JiffDate {
        self.0
    }

    pub fn year(self) -> i16 {
        self.0.year()
    }

    pub fn month(self) -> u8 {
        self.0.month() as u8
    }

    pub fn day(self) -> u8 {
        self.0.day() as u8
    }

    pub fn weekday(self) -> Weekday {
        match self.0.weekday().to_monday_zero_offset() {
            0 => Weekday::Monday,
            1 => Weekday::Tuesday,
            2 => Weekday::Wednesday,
            3 => Weekday::Thursday,
            4 => Weekday::Friday,
            5 => Weekday::Saturday,
            _ => Weekday::Sunday,
        }
    }

    pub fn month_name(self) -> &'static str {
        match self.0.month() {
            1 => "January",
            2 => "February",
            3 => "March",
            4 => "April",
            5 => "May",
            6 => "June",
            7 => "July",
            8 => "August",
            9 => "September",
            10 => "October",
            11 => "November",
            _ => "December",
        }
    }

    pub fn checked_sub_days(self, days: u32) -> Option<Self> {
        let days = i32::try_from(days).ok()?.checked_neg()?;
        Some(Self(self.0.checked_add(jiff::Span::new().days(days)).ok()?))
    }

    pub fn checked_add_days(self, days: u32) -> Option<Self> {
        let days = i32::try_from(days).ok()?;
        Some(Self(self.0.checked_add(jiff::Span::new().days(days)).ok()?))
    }
}

impl FromStr for Date {
    type Err = jiff::Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        s.parse::<JiffDate>().map(Date)
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
        decoder.decode_unsized(|s: &str| s.parse::<JiffDate>().map(Date).map_err(cx.map()))
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
        s.parse::<JiffDate>()
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

        JiffDate::new(year, month, day)
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

/// The source of a remote identifier.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Encode, Decode)]
#[musli(crate = musli_core)]
pub enum RemoteSource {
    Tvdb,
    Tmdb,
    Imdb,
    Unknown,
}

impl PartialEq<SyncSource> for RemoteSource {
    #[inline]
    fn eq(&self, other: &SyncSource) -> bool {
        matches!(
            (self, other),
            (RemoteSource::Tvdb, SyncSource::Tvdb) | (RemoteSource::Tmdb, SyncSource::Tmdb)
        )
    }
}

impl RemoteSource {
    pub fn is_unknown(&self) -> bool {
        matches!(self, Self::Unknown)
    }

    pub fn as_str(&self) -> &str {
        match self {
            Self::Tvdb => "tvdb",
            Self::Tmdb => "tmdb",
            Self::Imdb => "imdb",
            Self::Unknown => "unknown",
        }
    }

    pub fn from_raw(s: &str) -> Self {
        match s {
            "tvdb" => Self::Tvdb,
            "tmdb" => Self::Tmdb,
            "imdb" => Self::Imdb,
            _ => Self::Unknown,
        }
    }

    pub fn into_sync_source(self) -> SyncSource {
        match self {
            Self::Tvdb => SyncSource::Tvdb,
            Self::Tmdb => SyncSource::Tmdb,
            _ => SyncSource::Unknown,
        }
    }
}

impl fmt::Display for RemoteSource {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// The value part of a remote identifier — either an integer or a string.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
#[musli(crate = musli_core)]
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

/// Remote identifier: "tvdb:123", "tmdb:456", "imdb:tt0001234".
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode, serde::Serialize, serde::Deserialize)]
#[musli(crate = musli_core)]
#[serde(from = "String", into = "String")]
pub struct RemoteId {
    source: RemoteSource,
    value: RemoteValue,
}

impl RemoteId {
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

    pub fn from_raw(s: &str) -> Self {
        match s.split_once(':') {
            Some((src, val)) => Self {
                source: RemoteSource::from_raw(src),
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

    pub fn series_url(&self) -> Option<String> {
        match &self.source {
            RemoteSource::Tvdb => Some(format!("https://thetvdb.com/search?query={}", self.value)),
            RemoteSource::Tmdb => Some(format!("https://www.themoviedb.org/tv/{}", self.value)),
            RemoteSource::Imdb => Some(format!("https://www.imdb.com/title/{}/", self.value)),
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

impl fmt::Display for RemoteId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}:{}", self.source, self.value)
    }
}

impl From<String> for RemoteId {
    fn from(s: String) -> Self {
        Self::from_raw(&s)
    }
}

impl From<RemoteId> for String {
    fn from(r: RemoteId) -> String {
        r.to_string()
    }
}

#[cfg(feature = "sqll")]
impl ::sqll::FromColumn<'_> for RemoteId {
    type Type = ::sqll::ty::Text;

    #[inline]
    fn from_column(stmt: &::sqll::Statement, index: ::sqll::ty::Text) -> ::sqll::Result<Self> {
        let s = String::from_column(stmt, index)?;
        Ok(RemoteId::from_raw(&s))
    }
}

#[cfg(feature = "sqll")]
impl ::sqll::BindValue for RemoteId {
    #[inline]
    fn bind_value(&self, stmt: &mut ::sqll::Statement, index: ::sqll::Index) -> ::sqll::Result<()> {
        let s = self.to_string();
        s.as_str().bind_value(stmt, index)
    }
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
    Default,
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
    #[default]
    Specials,
    Number(NonZero<u32>),
}

impl SeasonNumber {
    #[inline]
    pub fn from_ordinal(n: u32) -> Self {
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
    Debug, Clone, Copy, PartialEq, Eq, Encode, Decode, serde::Serialize, serde::Deserialize,
)]
#[musli(crate = musli_core)]
#[serde(rename_all = "lowercase")]
pub enum SyncSource {
    Tvdb,
    Tmdb,
    Unknown,
}

impl SyncSource {
    /// Test if the sync source is unknown.
    pub fn is_unknown(&self) -> bool {
        matches!(*self, Self::Unknown)
    }

    pub fn parse(s: &str) -> Self {
        match s {
            "tvdb" => Self::Tvdb,
            "tmdb" => Self::Tmdb,
            _ => Self::Unknown,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Tvdb => "tvdb",
            Self::Tmdb => "tmdb",
            Self::Unknown => "unknown",
        }
    }

    pub fn from_remote_source(remote_source: &RemoteSource) -> Self {
        match remote_source {
            RemoteSource::Tvdb => Self::Tvdb,
            RemoteSource::Tmdb => Self::Tmdb,
            _ => Self::Unknown,
        }
    }

    pub fn into_remote_source(self) -> RemoteSource {
        match self {
            Self::Tvdb => RemoteSource::Tvdb,
            Self::Tmdb => RemoteSource::Tmdb,
            Self::Unknown => RemoteSource::Unknown,
        }
    }
}

impl PartialEq<RemoteSource> for SyncSource {
    #[inline]
    fn eq(&self, other: &RemoteSource) -> bool {
        matches!(
            (self, other),
            (SyncSource::Tvdb, RemoteSource::Tvdb) | (SyncSource::Tmdb, RemoteSource::Tmdb)
        )
    }
}

impl fmt::Display for SyncSource {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

#[cfg(feature = "sqll")]
impl ::sqll::FromColumn<'_> for SyncSource {
    type Type = ::sqll::ty::Integer;

    fn from_column(stmt: &::sqll::Statement, index: ::sqll::ty::Integer) -> ::sqll::Result<Self> {
        let s = u32::from_column(stmt, index)?;

        match s {
            1 => Ok(SyncSource::Tvdb),
            2 => Ok(SyncSource::Tmdb),
            _ => Ok(SyncSource::Unknown),
        }
    }
}

#[cfg(feature = "sqll")]
impl ::sqll::BindValue for SyncSource {
    fn bind_value(&self, stmt: &mut ::sqll::Statement, index: ::sqll::Index) -> ::sqll::Result<()> {
        let n: u32 = match self {
            SyncSource::Unknown => 0,
            SyncSource::Tvdb => 1,
            SyncSource::Tmdb => 2,
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

// ── Core data types ──────────────────────────────────────────────────────────

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
    pub country: String,
    pub release_type: ReleaseType,
    pub timestamp: Timestamp,
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
pub struct Series {
    pub id: SeriesId,
    pub title: Option<String>,
    pub first_air_date: Option<Timestamp>,
    pub overview: Option<String>,
    pub tracked: bool,
    pub sync_source: Option<SyncSource>,
    pub remotes: Vec<RemoteId>,
    pub images: Vec<MediaImage>,
    pub poster: Option<Image>,
    pub banner: Option<Image>,
    pub backdrop: Option<Image>,
    pub last_synced_at: Option<Timestamp>,
    pub language: Option<String>,
}

impl Series {
    pub fn remote_by_source(&self, source: SyncSource) -> Option<&RemoteId> {
        self.remotes.iter().find(|r| *r.source() == source)
    }

    pub fn effective_sync_source(&self) -> Option<SyncSource> {
        if let Some(source) = self.sync_source
            && self.remote_by_source(source).is_some()
        {
            return Some(source);
        }

        if self.remote_by_source(SyncSource::Tmdb).is_some() {
            return Some(SyncSource::Tmdb);
        }

        if self.remote_by_source(SyncSource::Tvdb).is_some() {
            return Some(SyncSource::Tvdb);
        }

        None
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
    pub series_id: SeriesId,
    pub season: SeasonNumber,
    pub air_date: Option<Timestamp>,
    pub name: Option<String>,
    pub overview: Option<String>,
    pub watched_count: u32,
    pub total_count: u32,
}

#[derive(Debug, Clone, Encode, Decode)]
#[musli(crate = musli_core)]
pub struct Episode {
    pub id: EpisodeId,
    pub series_id: SeriesId,
    pub season: SeasonNumber,
    pub episode: u32,
    pub absolute_number: Option<u32>,
    pub name: Option<String>,
    pub overview: Option<String>,
    pub aired: Option<Timestamp>,
    pub remote_id: Option<RemoteId>,
    pub pending: bool,
    pub screenshot: Option<Image>,
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
    pub title: Option<String>,
    pub release_date: Option<Timestamp>,
    pub overview: Option<String>,
    pub remotes: Vec<RemoteId>,
    pub sync_source: Option<SyncSource>,
    pub tracked: bool,
    pub pending: bool,
    pub images: Vec<MediaImage>,
    pub poster: Option<Image>,
    pub banner: Option<Image>,
    pub backdrop: Option<Image>,
    pub last_synced_at: Option<Timestamp>,
    pub releases: Vec<MovieRelease>,
    pub language: Option<String>,
}

impl Movie {
    pub fn remote_by_source(&self, source: SyncSource) -> Option<&RemoteId> {
        self.remotes.iter().find(|r| *r.source() == source)
    }

    pub fn effective_sync_source(&self) -> Option<SyncSource> {
        if let Some(source) = self.sync_source
            && self.remote_by_source(source).is_some()
        {
            return Some(source);
        }

        if self.remote_by_source(SyncSource::Tmdb).is_some() {
            return Some(SyncSource::Tmdb);
        }

        if self.remote_by_source(SyncSource::Tvdb).is_some() {
            return Some(SyncSource::Tvdb);
        }

        None
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
    Series(SeriesId),
    Movie(MovieId),
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

impl WatchedKind {
    pub fn into_event(self) -> WatchedEvent {
        match self {
            WatchedKind::Episode { series, episode } => WatchedEvent::Episode { series, episode },
            WatchedKind::Movie { movie } => WatchedEvent::Movie { movie },
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Encode, Decode)]
#[musli(crate = musli_core)]
pub enum WatchedEvent {
    Episode {
        series: SeriesId,
        episode: EpisodeId,
    },
    RemainingSeason {
        series: SeriesId,
        season: SeasonNumber,
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
pub enum PendingInfo {
    Episode {
        series: Option<String>,
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
    pub poster: Option<Image>,
    pub banner: Option<Image>,
}

/// Implemented by types that carry both a civil air date and an optional
/// precise timestamp. `display_at` picks the most precise value available
/// and formats it in the given time zone.
pub trait HasAired {
    fn aired(&self) -> Option<Timestamp>;

    fn display_at(&self, tz: TimeZone) -> Option<String> {
        let ts = self.aired()?;
        Some(ts.display(tz))
    }
}

impl HasAired for Episode {
    fn aired(&self) -> Option<Timestamp> {
        self.aired
    }
}

impl HasAired for Pending {
    fn aired(&self) -> Option<Timestamp> {
        self.aired
    }
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
    pub tvdb_api_key: String,
    pub tvdb_pin: Option<String>,
    pub tmdb_api_key: String,
    pub schedule_duration_days: u32,
    pub dashboard_page: u32,
    pub auto_sync_enabled: bool,
    pub auto_sync_interval_hours: u32,
    pub timezone: String,
    pub language: Option<String>,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            theme: ThemeType::Dark,
            tvdb_api_key: String::new(),
            tvdb_pin: None,
            tmdb_api_key: String::new(),
            schedule_duration_days: 7,
            dashboard_page: 5,
            auto_sync_enabled: false,
            auto_sync_interval_hours: 24,
            timezone: String::new(),
            language: None,
        }
    }
}

#[derive(Debug, Encode, Decode)]
#[musli(crate = musli_core)]
pub struct Empty;

// ── Search types ─────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Encode, Decode)]
#[musli(crate = musli_core)]
pub enum SearchKind {
    #[default]
    Series,
    Movies,
}

#[derive(Debug, Clone, Encode, Decode)]
#[musli(crate = musli_core)]
pub struct SearchSeries {
    pub remote_id: RemoteId,
    pub title: Option<String>,
    pub poster: Option<Image>,
    pub banner: Option<Image>,
    pub backdrop: Option<Image>,
    pub overview: Option<String>,
    pub first_air_date: Option<Date>,
    pub already_tracked: Option<SeriesId>,
}

#[derive(Debug, Clone, Encode, Decode)]
#[musli(crate = musli_core)]
pub struct SearchMovie {
    pub remote_id: RemoteId,
    pub title: Option<String>,
    pub poster: Option<Image>,
    pub banner: Option<Image>,
    pub backdrop: Option<Image>,
    pub overview: Option<String>,
    pub release_date: Option<Date>,
    pub already_tracked: Option<MovieId>,
}

// ── Task queue ────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Encode, Decode)]
#[musli(crate = musli_core)]
pub enum TaskKind {
    SyncSeries {
        series_id: SeriesId,
        title: Option<String>,
    },
    SyncMovie {
        movie_id: MovieId,
        title: Option<String>,
    },
}

impl TaskKind {
    #[inline]
    pub fn title(&self) -> Option<&str> {
        match self {
            TaskKind::SyncSeries { title, .. } | TaskKind::SyncMovie { title, .. } => {
                title.as_deref()
            }
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
}

#[derive(Debug, Clone, Encode, Decode)]
#[musli(crate = musli_core)]
pub struct CompletedTask {
    pub id: TaskId,
    pub kind: TaskKind,
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
    pub tracked: bool,
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
    pub watched: Vec<WatchedEpisode>,
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
    pub series_id: SeriesId,
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
    pub series_id: SeriesId,
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
    pub series_id: SeriesId,
    pub season: SeasonNumber,
    pub episode: u32,
}

#[derive(Debug, Encode, Decode)]
#[musli(crate = musli_core)]
pub struct MoveWatchedEpisodeRequest {
    pub id: WatchedId,
    pub series_id: SeriesId,
    pub season: SeasonNumber,
    pub episode: u32,
}

#[derive(Debug, Encode, Decode)]
#[musli(crate = musli_core)]
pub struct ListOrphanedWatchedRequest {
    pub series_id: SeriesId,
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
pub struct ListScheduleRequest {
    pub tz: Option<String>,
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
    pub page: usize,
}

#[derive(Debug, Encode, Decode)]
#[musli(crate = musli_core)]
pub struct SearchResponse {
    pub series: Vec<SearchSeries>,
    pub movies: Vec<SearchMovie>,
    /// Total number of results across the queried sources.
    pub total: usize,
}

#[derive(Debug, Encode, Decode)]
#[musli(crate = musli_core)]
pub struct SyncSeriesRequest {
    pub id: SeriesId,
}

#[derive(Debug, Encode, Decode)]
#[musli(crate = musli_core)]
pub struct SyncMovieRequest {
    pub id: MovieId,
}

#[derive(Debug, Encode, Decode)]
#[musli(crate = musli_core)]
pub struct SetSeriesSyncSourceRequest {
    pub id: SeriesId,
    pub source: SyncSource,
}

#[derive(Debug, Encode, Decode)]
#[musli(crate = musli_core)]
pub struct SetMovieSyncSourceRequest {
    pub id: MovieId,
    pub source: SyncSource,
}

#[derive(Debug, Encode, Decode)]
#[musli(crate = musli_core)]
pub struct SetSeriesLanguageRequest {
    pub id: SeriesId,
    pub language: Option<String>,
}

#[derive(Debug, Encode, Decode)]
#[musli(crate = musli_core)]
pub struct SetMovieLanguageRequest {
    pub id: MovieId,
    pub language: Option<String>,
}

#[derive(Debug, Encode, Decode)]
#[musli(crate = musli_core)]
pub struct AddSeriesRemoteRequest {
    pub id: SeriesId,
    pub remote_id: RemoteId,
}

#[derive(Debug, Encode, Decode)]
#[musli(crate = musli_core)]
pub struct RemoveSeriesRemoteRequest {
    pub id: SeriesId,
    pub remote_id: RemoteId,
}

#[derive(Debug, Encode, Decode)]
#[musli(crate = musli_core)]
pub struct AddMovieRemoteRequest {
    pub id: MovieId,
    pub remote_id: RemoteId,
}

#[derive(Debug, Encode, Decode)]
#[musli(crate = musli_core)]
pub struct RemoveMovieRemoteRequest {
    pub id: MovieId,
    pub remote_id: RemoteId,
}

#[derive(Debug, Encode, Decode)]
#[musli(crate = musli_core)]
pub struct UpdateSeriesRemoteRequest {
    pub id: SeriesId,
    pub old: RemoteId,
    pub new: RemoteId,
}

#[derive(Debug, Encode, Decode)]
#[musli(crate = musli_core)]
pub struct UpdateMovieRemoteRequest {
    pub id: MovieId,
    pub old: RemoteId,
    pub new: RemoteId,
}

#[derive(Debug, Encode, Decode)]
#[musli(crate = musli_core)]
pub struct SyncAllRequest;

#[derive(Debug, Encode, Decode)]
#[musli(crate = musli_core)]
pub struct ListTasksRequest;

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
pub struct SetConfigRequest {
    pub config: Config,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Encode, Decode)]
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

#[derive(Debug, Encode, Decode)]
#[musli(crate = musli_core)]
pub struct AddPendingRequest {
    pub kind: PendingKind,
}

#[derive(Debug, Encode, Decode)]
#[musli(crate = musli_core)]
pub struct RemovePendingRequest {
    pub kind: PendingKind,
}

#[derive(Debug, Encode, Decode)]
#[musli(crate = musli_core)]
pub struct SkipEpisodeRequest {
    pub series: SeriesId,
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
        event: WatchedEvent,
    },
    PendingChanged,
    ConfigChanged {
        config: Config,
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

    pub type SyncMovie;
    impl Endpoint for SyncMovie {
        impl Request for SyncMovieRequest;
        type Response<'de> = Empty;
    }

    pub type SetSeriesSyncSource;
    impl Endpoint for SetSeriesSyncSource {
        impl Request for SetSeriesSyncSourceRequest;
        type Response<'de> = Empty;
    }

    pub type SetMovieSyncSource;
    impl Endpoint for SetMovieSyncSource {
        impl Request for SetMovieSyncSourceRequest;
        type Response<'de> = Empty;
    }

    pub type SetSeriesLanguage;
    impl Endpoint for SetSeriesLanguage {
        impl Request for SetSeriesLanguageRequest;
        type Response<'de> = Empty;
    }

    pub type SetMovieLanguage;
    impl Endpoint for SetMovieLanguage {
        impl Request for SetMovieLanguageRequest;
        type Response<'de> = Empty;
    }

    pub type AddSeriesRemote;
    impl Endpoint for AddSeriesRemote {
        impl Request for AddSeriesRemoteRequest;
        type Response<'de> = Empty;
    }

    pub type RemoveSeriesRemote;
    impl Endpoint for RemoveSeriesRemote {
        impl Request for RemoveSeriesRemoteRequest;
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

    pub type UpdateSeriesRemote;
    impl Endpoint for UpdateSeriesRemote {
        impl Request for UpdateSeriesRemoteRequest;
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

    pub type AddPending;
    impl Endpoint for AddPending {
        impl Request for AddPendingRequest;
        type Response<'de> = Empty;
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
