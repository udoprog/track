use core::fmt;
use core::num::NonZero;
use core::str::FromStr;

use jiff::Timestamp as JiffTimestamp;
use jiff::civil::Date as JiffDate;
use jiff::tz::TimeZone as JiffTimeZone;
use musli_core::{Context, Decode, Encode};
use musli_web::api::{self, ChannelId};

#[cfg(feature = "yew")]
use implicit_clone::unsync::IString;
#[cfg(feature = "yew")]
use yew::html::IntoPropValue;
#[cfg(feature = "yew")]
use yew::virtual_dom::VNode;

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

define_id!(ShowId);
define_id!(SeasonId);
define_id!(EpisodeId);
define_id!(MovieId);
define_id!(MovieReleaseId);
define_id!(WatchedId);
define_id!(TaskId);
define_id!(ImageId);
define_id!(PendingId);
define_id!(RemoteId);

/// A language identified by its 3-letter ISO 639-3 code, stored as four bytes:
/// the three ASCII letters followed by a `0` pad.
///
/// The all-zero value is [`LanguageCode::DEFAULT`], a reference-time sentinel
/// meaning "use the media's own default (original) language". It is never a real
/// language and must never be persisted as data in the `strings` table.
///
/// The in-memory representation is the code's bytes directly; the only place
/// bytes are turned into an integer is the SQLite conversion below, which pins
/// the byte order so the stored value is identical regardless of host endianness.
#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Language([u8; 4]);

/// Error produced when a string cannot be parsed as a [`LanguageCode`].
#[derive(Debug)]
pub struct ParseLanguageErr;

impl fmt::Display for ParseLanguageErr {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("invalid language code")
    }
}

impl core::error::Error for ParseLanguageErr {}

impl Language {
    /// Sentinel meaning "use the media's own default (original) language".
    pub const DEFAULT: Language = Language([0; 4]);

    /// English (`eng`).
    pub const ENG: Language = Language(*b"eng\0");

    /// Unwrap the current language or fall back to other if the current
    /// language is `DEFAULT`.
    pub fn or(self, other: Self) -> Self {
        if self.is_default() { other } else { self }
    }

    /// The ascii string corresponding to this language code.
    pub fn as_str(&self) -> &str {
        if self.is_default() {
            return "default";
        }

        self.as_raw_code()
    }

    fn as_raw_code(&self) -> &str {
        let end = self.0.iter().position(|&b| b == 0).unwrap_or(self.0.len());

        // SAFETY: The language code is valid through construction.
        unsafe { str::from_utf8_unchecked(&self.0[..end]) }
    }

    /// Build from a 2- or 3-letter ISO 639 code (case-insensitive). A 2-letter
    /// code is resolved to its 3-letter form via the `iso639` data. An empty
    /// string or `"default"` maps to [`LanguageCode::DEFAULT`]. Returns `None`
    /// for anything else.
    pub fn from_iso639(code: &str) -> Option<Self> {
        fn to_lower<'a>(buf: &'a mut [u8; 4], input: &str) -> Option<&'a str> {
            let bytes = input.as_bytes();

            if bytes.len() > buf.len() {
                return None;
            }

            for (b, o) in bytes.iter().zip(buf.iter_mut()) {
                if !b.is_ascii_alphabetic() {
                    return None;
                }

                *o = b.to_ascii_lowercase();
            }

            Some(unsafe { str::from_utf8_unchecked(&buf[..bytes.len()]) })
        }

        let code = code.trim();

        if code.is_empty() || code.eq_ignore_ascii_case("default") {
            return Some(Self::DEFAULT);
        }

        let mut bytes = [0u8; 4];
        let lower = to_lower(&mut bytes, code)?;

        let bytes = match lower.len() {
            2 => {
                let id = iso639::by_part1(lower)?.id;

                let &[a, b, c] = id.as_bytes() else {
                    return None;
                };

                [a, b, c, 0]
            }
            3 => bytes,
            _ => return None,
        };

        Some(Self(bytes))
    }

    /// Whether this is the [`LanguageCode::DEFAULT`] sentinel.
    #[inline]
    pub const fn is_default(self) -> bool {
        matches!(self.0, [0, 0, 0, 0])
    }

    /// The 3-letter ISO 639-3 code, or `None` for [`LanguageCode::DEFAULT`].
    pub fn to_iso639_3(&self) -> Option<&str> {
        if self.is_default() {
            return None;
        }

        Some(self.as_raw_code())
    }

    /// The 2-letter ISO 639-1 code, if one exists (`None` for `DEFAULT` or codes
    /// without a 2-letter form). Used for remotes that key on ISO 639-1.
    pub fn to_iso639_1(&self) -> Option<&str> {
        let id = self.to_iso639_3()?;
        iso639::by_id(&id)?.part1
    }
}

impl fmt::Display for Language {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl fmt::Debug for Language {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "LanguageCode({self})")
    }
}

impl FromStr for Language {
    type Err = ParseLanguageErr;

    #[inline]
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Self::from_iso639(s).ok_or(ParseLanguageErr)
    }
}

impl serde::Serialize for Language {
    #[inline]
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        self.as_str().serialize(serializer)
    }
}

impl<'de> serde::Deserialize<'de> for Language {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        struct Visitor;

        impl serde::de::Visitor<'_> for Visitor {
            type Value = Language;

            fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
                formatter.write_str("a 2- or 3-letter ISO 639 language code or 'default'")
            }

            fn visit_str<E>(self, v: &str) -> Result<Self::Value, E>
            where
                E: serde::de::Error,
            {
                v.parse().map_err(serde::de::Error::custom)
            }
        }

        deserializer.deserialize_str(Visitor)
    }
}

impl<M> musli_core::Encode<M> for Language {
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

impl<'de, M, A> musli_core::Decode<'de, M, A> for Language
where
    A: musli_core::Allocator,
{
    fn decode<D>(decoder: D) -> Result<Self, D::Error>
    where
        D: musli_core::Decoder<'de, Mode = M, Allocator = A>,
    {
        let cx = decoder.cx();
        decoder.decode_unsized(|s: &str| s.parse::<Language>().map_err(cx.map()))
    }
}

#[cfg(feature = "sqll")]
impl ::sqll::FromColumn<'_> for Language {
    type Type = ::sqll::ty::Integer;

    #[inline]
    fn from_column(stmt: &::sqll::Statement, index: ::sqll::ty::Integer) -> ::sqll::Result<Self> {
        let value = i64::from_column(stmt, index)?;
        Ok(Language((value as u32).to_be_bytes()))
    }
}

#[cfg(feature = "sqll")]
impl ::sqll::BindValue for Language {
    #[inline]
    fn bind_value(&self, stmt: &mut ::sqll::Statement, index: ::sqll::Index) -> ::sqll::Result<()> {
        i64::from(u32::from_be_bytes(self.0)).bind_value(stmt, index)
    }
}

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

    /// The wall-clock timestamp `duration` from now.
    #[inline]
    pub fn from_now(duration: std::time::Duration) -> Self {
        let ms = JiffTimestamp::now().as_millisecond() + duration.as_millis() as i64;
        Self(JiffTimestamp::from_millisecond(ms).unwrap_or_else(|_| JiffTimestamp::now()))
    }

    /// The duration from `earlier` until this timestamp, or `None` if this
    /// timestamp is not after `earlier`.
    #[inline]
    pub fn checked_duration_since(self, earlier: Timestamp) -> Option<std::time::Duration> {
        let ms = self.0.as_millisecond() - earlier.0.as_millisecond();
        u64::try_from(ms).ok().map(std::time::Duration::from_millis)
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

    /// Format just the local time of day (`"HH:MM"`, 24-hour) in the given timezone.
    #[inline]
    pub fn time_of_day(&self, tz: TimeZone) -> String {
        self.0.to_zoned(tz.0).strftime("%H:%M").to_string()
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

    /// Days since Monday (0 = Monday to 6 = Sunday).
    pub fn from_monday(self) -> u32 {
        self as u32
    }

    pub fn long_name(self) -> &'static str {
        match self {
            Weekday::Monday => "Monday",
            Weekday::Tuesday => "Tuesday",
            Weekday::Wednesday => "Wednesday",
            Weekday::Thursday => "Thursday",
            Weekday::Friday => "Friday",
            Weekday::Saturday => "Saturday",
            Weekday::Sunday => "Sunday",
        }
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
            Self::Tmdb | Self::Tvdb => &[SyncKind::Base, SyncKind::AirDate],
            Self::Tvmaze => &[SyncKind::AirDate],
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

/// A kind of data a [`RemoteSource`] can contribute during a layered sync.
///
/// During a sync the enabled remotes are visited in priority order and each
/// contributes the kinds it supports ([`RemoteSource::sync_kinds`]). A draft
/// tracks which kinds have already been contributed: an *exclusive* kind
/// ([`SyncKind::is_exclusive`]) is taken by the first source that provides it and
/// skipped by later layers, while a non-exclusive kind accumulates from every
/// source. Graphics always accumulate and are tracked separately.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Encode, Decode)]
#[musli(crate = musli_core)]
pub enum SyncKind {
    /// Core metadata: title, overview, seasons and episode details.
    Base,
    /// Episode air dates (recorded as `episode_releases` and merged by priority).
    AirDate,
}

impl SyncKind {
    /// All sync kinds, in a stable order.
    pub const ALL: &[Self] = &[Self::Base, Self::AirDate];

    /// Whether only the first (highest-priority) source providing this kind
    /// contributes it. Non-exclusive kinds accumulate from every source.
    pub fn is_exclusive(&self) -> bool {
        matches!(self, Self::Base)
    }

    /// Human-readable label.
    pub fn as_label(&self) -> &'static str {
        match self {
            Self::Base => "Base",
            Self::AirDate => "Air Date",
        }
    }

    /// The single bit representing this kind in a [`SyncKindSet`].
    pub fn bit(&self) -> u32 {
        match self {
            Self::Base => 1 << 0,
            Self::AirDate => 1 << 1,
        }
    }
}

/// A set of [`SyncKind`]s, stored compactly as a bitmask. Used both as a
/// source's capability ceiling ([`RemoteSource::default_sync_kinds`]) and as the
/// configured selection (global default or per-remote override).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct SyncKindSet(u32);

impl SyncKindSet {
    pub const fn empty() -> Self {
        Self(0)
    }

    pub fn from_kinds<'a>(kinds: impl IntoIterator<Item = SyncKind>) -> Self {
        let mut set = Self::empty();

        for k in kinds {
            set.0 |= k.bit();
        }

        set
    }

    /// Reconstruct from raw bits, masking off any unknown bits.
    pub fn from_bits(bits: u32) -> Self {
        let known: u32 = SyncKind::ALL.iter().map(SyncKind::bit).sum();
        Self(bits & known)
    }

    pub fn bits(&self) -> u32 {
        self.0
    }

    pub fn contains(&self, kind: SyncKind) -> bool {
        self.0 & kind.bit() != 0
    }

    pub fn insert(&mut self, kind: SyncKind) {
        self.0 |= kind.bit();
    }

    pub fn with(mut self, kind: SyncKind, on: bool) -> Self {
        if on {
            self.0 |= kind.bit();
        } else {
            self.0 &= !kind.bit();
        }

        self
    }

    pub fn is_empty(&self) -> bool {
        self.0 == 0
    }

    /// The kinds present in both sets.
    pub fn intersect(self, other: Self) -> Self {
        Self(self.0 & other.0)
    }

    /// The kinds present, in [`SyncKind::ALL`] order.
    #[inline]
    pub fn iter(&self) -> SyncKindSetIter {
        SyncKindSetIter { base: self.0 }
    }
}

impl IntoIterator for SyncKindSet {
    type Item = SyncKind;
    type IntoIter = SyncKindSetIter;

    #[inline]
    fn into_iter(self) -> Self::IntoIter {
        self.iter()
    }
}

/// An iterator over the kinds present in a [`SyncKindSet`].
pub struct SyncKindSetIter {
    base: u32,
}

impl Iterator for SyncKindSetIter {
    type Item = SyncKind;

    #[inline]
    fn next(&mut self) -> Option<Self::Item> {
        if self.base == 0 {
            return None;
        }

        let bit = self.base.trailing_zeros();
        self.base &= !(1 << bit);

        match bit {
            0 => Some(SyncKind::Base),
            1 => Some(SyncKind::AirDate),
            _ => None,
        }
    }
}

impl FromIterator<SyncKind> for SyncKindSet {
    fn from_iter<T: IntoIterator<Item = SyncKind>>(iter: T) -> Self {
        Self::from_kinds(iter.into_iter())
    }
}

impl<M> musli_core::Encode<M> for SyncKindSet {
    type Encode = Self;

    fn encode<E>(&self, encoder: E) -> Result<(), E::Error>
    where
        E: musli_core::Encoder<Mode = M>,
    {
        self.0.encode(encoder)
    }

    fn as_encode(&self) -> &Self::Encode {
        self
    }
}

impl<'de, M, A> musli_core::Decode<'de, M, A> for SyncKindSet
where
    A: musli_core::Allocator,
{
    fn decode<D>(decoder: D) -> Result<Self, D::Error>
    where
        D: musli_core::Decoder<'de, Mode = M, Allocator = A>,
    {
        Ok(Self::from_bits(u32::decode(decoder)?))
    }
}

impl serde::Serialize for SyncKindSet {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.serialize_u32(self.0)
    }
}

impl<'de> serde::Deserialize<'de> for SyncKindSet {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        Ok(Self::from_bits(u32::deserialize(deserializer)?))
    }
}

#[cfg(feature = "sqll")]
impl ::sqll::FromColumn<'_> for SyncKindSet {
    type Type = ::sqll::ty::Integer;

    fn from_column(stmt: &::sqll::Statement, index: ::sqll::ty::Integer) -> ::sqll::Result<Self> {
        Ok(Self::from_bits(u32::from_column(stmt, index)?))
    }
}

#[cfg(feature = "sqll")]
impl ::sqll::BindValue for SyncKindSet {
    fn bind_value(&self, stmt: &mut ::sqll::Statement, index: ::sqll::Index) -> ::sqll::Result<()> {
        self.0.bind_value(stmt, index)
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
        .filter(|e| e.enabled && effective_remote_sync_kinds(e, config).contains(SyncKind::AirDate))
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
    pub country: String,
    pub release_type: ReleaseType,
    pub timestamp: Timestamp,
}

/// A release type that contributes to a movie's effective release date, optionally restricted to a
/// set of countries.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode, serde::Serialize, serde::Deserialize)]
#[musli(crate = musli_core)]
pub struct ReleaseFilter {
    pub release_type: ReleaseType,
    /// Countries (ISO 3166-1 alpha-2) of interest for this release type. Empty means all countries.
    #[serde(default)]
    pub countries: Vec<String>,
}

impl ReleaseFilter {
    /// The default set of release filters: Digital, Physical and Tv across all countries.
    pub fn default_filters() -> Vec<ReleaseFilter> {
        [ReleaseType::Digital, ReleaseType::Physical, ReleaseType::Tv]
            .into_iter()
            .map(|release_type| ReleaseFilter {
                release_type,
                countries: Vec::new(),
            })
            .collect()
    }

    /// Whether the given release matches this filter.
    pub fn matches(&self, release: &MovieRelease) -> bool {
        self.release_type == release.release_type
            && (self.countries.is_empty()
                || self
                    .countries
                    .iter()
                    .any(|c| c.eq_ignore_ascii_case(&release.country)))
    }
}

/// The earliest timestamp among `releases` that matches any of the given `filters`.
pub fn earliest_release(releases: &[MovieRelease], filters: &[ReleaseFilter]) -> Option<Timestamp> {
    releases
        .iter()
        .filter(|r| filters.iter().any(|f| f.matches(r)))
        .map(|r| r.timestamp)
        .min()
}

/// Serialize release filters for storage in a text column.
pub fn encode_release_filters(filters: &[ReleaseFilter]) -> String {
    serde_json::to_string(filters).unwrap_or_else(|_| "[]".to_string())
}

/// Parse release filters previously written by [`encode_release_filters`].
pub fn decode_release_filters(s: &str) -> Option<Vec<ReleaseFilter>> {
    serde_json::from_str(s).ok()
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
    pub country: String,
    pub network: String,
    pub timestamp: Timestamp,
}

/// Restricts which of a source's air dates qualify, by country and/or network.
/// Empty `countries`/`networks` mean "any". Priority between sources comes from
/// the media's remote order, not from this filter.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode, serde::Serialize, serde::Deserialize)]
#[musli(crate = musli_core)]
pub struct AirDateFilter {
    pub source: RemoteSource,
    #[serde(default)]
    pub countries: Vec<String>,
    #[serde(default)]
    pub networks: Vec<String>,
}

impl AirDateFilter {
    /// Whether `release` is allowed by this filter (source, country and network).
    pub fn matches(&self, release: &EpisodeRelease) -> bool {
        self.source == release.source
            && (self.countries.is_empty()
                || self
                    .countries
                    .iter()
                    .any(|c| c.eq_ignore_ascii_case(&release.country)))
            && (self.networks.is_empty()
                || self
                    .networks
                    .iter()
                    .any(|n| n.eq_ignore_ascii_case(&release.network)))
    }
}

/// The effective air date for an episode: the earliest qualifying release from
/// the highest-priority source. Only sources present in `priority` (the
/// eligible, AirDate-enabled sources) contribute - a release from any other
/// source is ignored, so excluding a source's air dates drops its dates
/// entirely and an empty `priority` yields `None`. A source with no filter
/// entry qualifies fully; a source with filter entries qualifies only for
/// matching country/network. Returns `None` when nothing qualifies.
pub fn effective_aired(
    releases: &[EpisodeRelease],
    priority: &[RemoteSource],
    filters: &[AirDateFilter],
) -> Option<Timestamp> {
    let qualifies = |r: &EpisodeRelease| {
        if !priority.contains(&r.source) {
            return false;
        }

        let has_source_filter = filters.iter().any(|f| f.source == r.source);
        !has_source_filter || filters.iter().any(|f| f.matches(r))
    };

    let mut merged = Prioritized::new();

    for r in releases.iter().filter(|r| qualifies(r)) {
        merged.push(r.source, r.timestamp);
    }

    merged.best(priority).into_iter().min()
}

/// Default air-date source priority: TVmaze (exact airtimes) over TMDB over TVDB.
pub fn default_air_date_priority() -> Vec<RemoteSource> {
    vec![RemoteSource::Tvmaze, RemoteSource::Tmdb, RemoteSource::Tvdb]
}

/// Serialize air-date filters for storage in a text column.
pub fn encode_air_date_filters(filters: &[AirDateFilter]) -> String {
    serde_json::to_string(filters).unwrap_or_else(|_| "[]".to_string())
}

/// Parse air-date filters previously written by [`encode_air_date_filters`].
pub fn decode_air_date_filters(s: &str) -> Option<Vec<AirDateFilter>> {
    serde_json::from_str(s).ok()
}

/// Serialize the global per-source sync-kind defaults for storage in a text column.
pub fn encode_sync_kinds(kinds: &[SourceSyncKinds]) -> String {
    serde_json::to_string(kinds).unwrap_or_else(|_| "[]".to_string())
}

/// Parse global per-source sync-kind defaults written by [`encode_sync_kinds`].
pub fn decode_sync_kinds(s: &str) -> Option<Vec<SourceSyncKinds>> {
    serde_json::from_str(s).ok()
}

/// Serialize the languages the sync path populates, for storage in a text column.
/// Each entry is its string form (`"default"` / `"eng"`).
pub fn encode_sync_languages(languages: &[Language]) -> String {
    serde_json::to_string(languages).unwrap_or_else(|_| "[]".to_string())
}

/// Parse sync languages written by [`encode_sync_languages`].
pub fn decode_sync_languages(s: &str) -> Option<Vec<Language>> {
    serde_json::from_str(s).ok()
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
    pub title: Option<String>,
    pub first_air_date: Option<Timestamp>,
    pub overview: Option<String>,
    pub tracked: bool,
    pub auto_sync: bool,
    pub remotes: Vec<RemoteEntry>,
    pub images: Vec<MediaImage>,
    pub poster: Option<Image>,
    pub banner: Option<Image>,
    pub backdrop: Option<Image>,
    pub last_synced_at: Option<Timestamp>,
    pub language: Language,
    pub include_specials: Option<bool>,
    pub air_date_filters: Option<Vec<AirDateFilter>>,
}

impl Show {
    pub fn effective_include_specials(&self, default: bool) -> bool {
        self.include_specials.unwrap_or(default)
    }

    /// The air-date filters in effect for this show, falling back to `default`.
    pub fn effective_air_date_filters<'a>(
        &'a self,
        default: &'a [AirDateFilter],
    ) -> &'a [AirDateFilter] {
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
    pub name: Option<String>,
    pub overview: Option<String>,
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
    pub name: Option<String>,
    pub overview: Option<String>,
    pub aired: Option<Timestamp>,
    pub remote_id: Option<Remote>,
    pub pending: bool,
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
    pub title: Option<String>,
    pub release_date: Option<Timestamp>,
    pub overview: Option<String>,
    pub remotes: Vec<RemoteEntry>,
    pub tracked: bool,
    /// Whether the background loop automatically refreshes this movie.
    pub auto_sync: bool,
    pub pending: bool,
    pub images: Vec<MediaImage>,
    pub poster: Option<Image>,
    pub banner: Option<Image>,
    pub backdrop: Option<Image>,
    pub last_synced_at: Option<Timestamp>,
    pub releases: Vec<MovieRelease>,
    pub language: Language,
    pub release_filters: Option<Vec<ReleaseFilter>>,
}

impl Movie {
    /// The release filters in effect for this movie, falling back to the global `default`.
    pub fn effective_release_filters<'a>(
        &'a self,
        default: &'a [ReleaseFilter],
    ) -> &'a [ReleaseFilter] {
        self.release_filters.as_deref().unwrap_or(default)
    }

    /// The effective release timestamp used to determine when this movie becomes pending, picking
    /// the earliest release matching the effective filters.
    pub fn pending_release(&self, default: &[ReleaseFilter]) -> Option<Timestamp> {
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
    /// The default display language. [`LanguageCode::DEFAULT`] means "use each
    /// show's/movie's own original language".
    pub language: Language,
    pub include_specials: bool,
    /// Default release types/countries that determine a movie's release date.
    pub release_filters: Vec<ReleaseFilter>,
    /// Default air-date qualification filters for episodes (empty = all qualify).
    pub air_date_filters: Vec<AirDateFilter>,
    /// Global per-source selection of which kinds each source contributes during
    /// sync. A source absent here uses its full capability. Per-remote overrides
    /// take precedence. See [`Config::sync_kinds_for`].
    pub sync_kinds: Vec<SourceSyncKinds>,
    /// Which languages the sync path populates translations for.
    /// [`LanguageCode::DEFAULT`] stands for each media's own original language.
    pub sync_languages: Vec<Language>,
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
            language: Language::DEFAULT,
            include_specials: false,
            release_filters: ReleaseFilter::default_filters(),
            air_date_filters: Vec::new(),
            sync_kinds: Vec::new(),
            sync_languages: vec![Language::DEFAULT, Language::ENG],
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
    pub title: Option<String>,
    /// Release date (movie) or first-air date (show).
    pub date: Option<Timestamp>,
    pub overview: Option<String>,
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
    pub language: Language,
}

#[derive(Debug, Encode, Decode)]
#[musli(crate = musli_core)]
pub struct SetShowIncludeSpecialsRequest {
    pub id: ShowId,
    pub include_specials: Option<bool>,
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
    pub air_date_filters: Option<Vec<AirDateFilter>>,
}

#[derive(Debug, Encode, Decode)]
#[musli(crate = musli_core)]
pub struct SetMovieLanguageRequest {
    pub id: MovieId,
    pub language: Language,
}

#[derive(Debug, Encode, Decode)]
#[musli(crate = musli_core)]
pub struct SetMovieReleaseFiltersRequest {
    pub id: MovieId,
    pub release_filters: Option<Vec<ReleaseFilter>>,
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
    /// Most-used custom language codes (ISO 639-1), ordered most-used first.
    pub top_languages: Vec<Language>,
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
    WatchedChanged {
        event: WatchedEvent,
    },
    PendingChanged,
    ConfigChanged {
        config: Config,
    },
    TopLanguagesChanged {
        top_languages: Vec<Language>,
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

#[cfg(test)]
mod tests {
    use super::*;

    fn rel(source: RemoteSource, country: &str, network: &str, ts: i64) -> EpisodeRelease {
        EpisodeRelease {
            source,
            country: country.to_owned(),
            network: network.to_owned(),
            timestamp: Timestamp::from_jiff(jiff::Timestamp::from_second(ts).unwrap()),
        }
    }

    fn entry(source: RemoteSource, sync_kinds: Option<SyncKindSet>) -> RemoteEntry {
        RemoteEntry {
            id: RemoteId::new(1),
            slug: None,
            remote: Remote::new(source, RemoteValue::Int(1)),
            enabled: true,
            priority: 0,
            sync_kinds,
        }
    }

    #[test]
    fn language_code_round_trip() {
        // Default sentinel.
        assert!(Language::DEFAULT.is_default());
        assert_eq!(Language::DEFAULT.to_string(), "default");
        assert_eq!(Language::DEFAULT.to_iso639_3(), None);
        assert_eq!(Language::from_iso639("default"), Some(Language::DEFAULT));
        assert_eq!(Language::from_iso639(""), Some(Language::DEFAULT));

        // 3-letter packs to its own bytes.
        let eng = Language::from_iso639("eng").unwrap();
        assert_eq!(eng, Language::ENG);
        assert_eq!(eng.to_string(), "eng");
        assert_eq!(eng.to_iso639_3().as_deref(), Some("eng"));
        assert_eq!(eng.to_iso639_1().as_deref(), Some("en"));

        // 2-letter resolves to 3-letter.
        assert_eq!(Language::from_iso639("en"), Some(Language::ENG));
        assert_eq!(Language::from_iso639("SV"), Language::from_iso639("swe"));

        // Case-insensitive and Display/FromStr round-trip.
        let swe = Language::from_iso639("Swe").unwrap();
        assert_eq!(swe.to_string().parse::<Language>().unwrap(), swe);

        // serde round-trips through the string form.
        let json = serde_json::to_string(&swe).unwrap();
        assert_eq!(json, "\"swe\"");
        assert_eq!(serde_json::from_str::<Language>(&json).unwrap(), swe);
        assert_eq!(
            serde_json::to_string(&Language::DEFAULT).unwrap(),
            "\"default\""
        );

        // Garbage is rejected.
        assert_eq!(Language::from_iso639("123"), None);
        assert_eq!(Language::from_iso639("toolong"), None);
    }

    #[test]
    fn sync_kind_set_bits_round_trip() {
        let set = SyncKindSet::from_kinds([SyncKind::AirDate]);
        assert!(set.contains(SyncKind::AirDate));
        assert!(!set.contains(SyncKind::Base));
        assert_eq!(SyncKindSet::from_bits(set.bits()), set);

        // Unknown bits are masked off.
        let masked = SyncKindSet::from_bits(0xFFFF_FFFF);
        assert_eq!(
            masked,
            SyncKindSet::from_kinds(SyncKind::ALL.iter().copied())
        );

        let toggled = SyncKindSet::empty()
            .with(SyncKind::Base, true)
            .with(SyncKind::AirDate, true)
            .with(SyncKind::Base, false);

        assert_eq!(toggled, SyncKindSet::from_kinds([SyncKind::AirDate]));
        assert_eq!(toggled.iter().collect::<Vec<_>>(), vec![SyncKind::AirDate]);
    }

    #[test]
    fn config_sync_kinds_for_clamps_to_capability() {
        // Absent source falls back to its full capability.
        let config = Config::default();
        assert_eq!(
            config.sync_kinds_for(RemoteSource::Tvmaze),
            SyncKindSet::from_kinds([SyncKind::AirDate])
        );

        // A configured entry granting more than the capability is clamped.
        let config = Config {
            sync_kinds: vec![SourceSyncKinds {
                source: RemoteSource::Tvmaze,
                kinds: SyncKindSet::from_kinds(SyncKind::ALL.iter().copied()),
            }],
            ..Config::default()
        };
        assert_eq!(
            config.sync_kinds_for(RemoteSource::Tvmaze),
            SyncKindSet::from_kinds([SyncKind::AirDate])
        );
    }

    #[test]
    fn effective_remote_sync_kinds_override_beats_global() {
        let config = Config {
            sync_kinds: vec![SourceSyncKinds {
                source: RemoteSource::Tmdb,
                kinds: SyncKindSet::from_kinds([SyncKind::AirDate]),
            }],
            ..Config::default()
        };

        // No override inherits the global default.
        let inherited = entry(RemoteSource::Tmdb, None);
        assert_eq!(
            effective_remote_sync_kinds(&inherited, &config),
            SyncKindSet::from_kinds([SyncKind::AirDate])
        );

        // An override wins, still clamped to capability.
        let overridden = entry(
            RemoteSource::Tmdb,
            Some(SyncKindSet::from_kinds([SyncKind::Base])),
        );
        assert_eq!(
            effective_remote_sync_kinds(&overridden, &config),
            SyncKindSet::from_kinds([SyncKind::Base])
        );
    }

    #[test]
    fn sync_kinds_capabilities() {
        use RemoteSource::*;

        // TMDB/TVDB are full base + air-date sources; TVmaze is air-dates only;
        // IMDb contributes nothing and no graphics.
        assert_eq!(Tmdb.sync_kinds(), &[SyncKind::Base, SyncKind::AirDate]);
        assert_eq!(Tvdb.sync_kinds(), &[SyncKind::Base, SyncKind::AirDate]);
        assert_eq!(Tvmaze.sync_kinds(), &[SyncKind::AirDate]);
        assert_eq!(Imdb.sync_kinds(), &[]);

        assert!(Tmdb.has_graphics());
        assert!(Tvdb.has_graphics());
        assert!(!Tvmaze.has_graphics());
        assert!(!Imdb.has_graphics());

        // Base is exclusive (first source wins); air dates accumulate.
        assert!(SyncKind::Base.is_exclusive());
        assert!(!SyncKind::AirDate.is_exclusive());
    }

    #[test]
    fn eligible_sync_kinds_unions_enabled_remotes() {
        let config = Config::default();

        // A remote restricted to AirDate plus one restricted to Base together make
        // both kinds eligible.
        let both = [
            entry(
                RemoteSource::Tmdb,
                Some(SyncKindSet::from_kinds([SyncKind::AirDate])),
            ),
            entry(
                RemoteSource::Tvdb,
                Some(SyncKindSet::from_kinds([SyncKind::Base])),
            ),
        ];
        assert_eq!(
            eligible_sync_kinds(&both, &config),
            SyncKindSet::from_kinds([SyncKind::Base, SyncKind::AirDate])
        );

        // With Base excluded from every remote, Base is no longer eligible, so its
        // derived seasons/episodes should be cleared on sync.
        let air_only = [
            entry(
                RemoteSource::Tmdb,
                Some(SyncKindSet::from_kinds([SyncKind::AirDate])),
            ),
            entry(RemoteSource::Tvmaze, None),
        ];
        let eligible = eligible_sync_kinds(&air_only, &config);
        assert!(!eligible.contains(SyncKind::Base));
        assert!(eligible.contains(SyncKind::AirDate));

        // A disabled remote contributes nothing.
        let mut disabled = entry(RemoteSource::Tmdb, None);
        disabled.enabled = false;
        assert!(eligible_sync_kinds(&[disabled], &config).is_empty());
    }

    #[test]
    fn air_date_priority_prefers_higher_ranked_source() {
        let releases = [
            rel(RemoteSource::Tmdb, "", "", 200),
            rel(RemoteSource::Tvmaze, "", "", 300),
        ];
        let priority = default_air_date_priority();

        // TVmaze outranks TMDB even though its date is later.
        let aired = effective_aired(&releases, &priority, &[]).unwrap();
        assert_eq!(aired.inner().as_second(), 300);

        // Flip the priority and TMDB wins.
        let flipped = [RemoteSource::Tmdb, RemoteSource::Tvmaze];
        let aired = effective_aired(&releases, &flipped, &[]).unwrap();
        assert_eq!(aired.inner().as_second(), 200);
    }

    #[test]
    fn air_date_filter_restricts_country() {
        let releases = [
            rel(RemoteSource::Tvmaze, "US", "", 300),
            rel(RemoteSource::Tvmaze, "GB", "", 100),
        ];
        let priority = default_air_date_priority();
        let filters = [AirDateFilter {
            source: RemoteSource::Tvmaze,
            countries: vec!["gb".to_owned()],
            networks: Vec::new(),
        }];

        // Only the GB date qualifies for TVmaze.
        let aired = effective_aired(&releases, &priority, &filters).unwrap();
        assert_eq!(aired.inner().as_second(), 100);
    }

    #[test]
    fn air_date_ignores_ineligible_source() {
        // A source absent from the priority list (e.g. its AirDate kind is
        // excluded) does not contribute, even as the only release.
        let releases = [rel(RemoteSource::Unknown, "", "", 50)];
        assert!(effective_aired(&releases, &default_air_date_priority(), &[]).is_none());
    }

    #[test]
    fn air_date_none_when_no_eligible_source() {
        // Excluding air dates from every remote leaves no eligible source, so even
        // a stored release yields no effective date.
        let releases = [rel(RemoteSource::Tvmaze, "", "", 50)];
        assert!(effective_aired(&releases, &[], &[]).is_none());
    }

    #[test]
    fn air_date_earliest_within_winning_source() {
        let releases = [
            rel(RemoteSource::Tvmaze, "US", "", 300),
            rel(RemoteSource::Tvmaze, "JP", "", 150),
            rel(RemoteSource::Tmdb, "", "", 10),
        ];
        let aired = effective_aired(&releases, &default_air_date_priority(), &[]).unwrap();
        // TVmaze wins by priority; earliest of its dates is used.
        assert_eq!(aired.inner().as_second(), 150);
    }
}
