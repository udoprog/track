use core::fmt;
use core::str::FromStr;

use jiff::Timestamp as JiffTimestamp;
use jiff::civil::Date as JiffDate;
use jiff::tz::TimeZone as JiffTimeZone;
use musli_core::{Allocator, Context as _, Decode, Decoder, Encode, Encoder};

#[cfg(feature = "yew")]
use yew::html::IntoPropValue;
#[cfg(feature = "yew")]
use yew::virtual_dom::VNode;

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

    /// The local `(hour, minute)` of this timestamp in the given timezone.
    #[inline]
    pub fn hour_minute(&self, tz: TimeZone) -> (u8, u8) {
        let zoned = self.0.to_zoned(tz.0);
        (zoned.hour() as u8, zoned.minute() as u8)
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

impl<M> Encode<M> for Timestamp {
    type Encode = Self;

    fn encode<E>(&self, encoder: E) -> Result<(), E::Error>
    where
        E: Encoder<Mode = M>,
    {
        encoder.collect_string(self)
    }

    fn as_encode(&self) -> &Self::Encode {
        self
    }
}

impl<'de, M, A> Decode<'de, M, A> for Timestamp
where
    A: Allocator,
{
    fn decode<D>(decoder: D) -> Result<Self, D::Error>
    where
        D: Decoder<'de, Mode = M, Allocator = A>,
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
        self.to_timestamp_at_zoned(0, 0, tz)
    }

    /// Compose this date with a local `hour`/`minute` in `tz` into a [`Timestamp`].
    pub fn to_timestamp_at_zoned(
        self,
        hour: u8,
        minute: u8,
        tz: TimeZone,
    ) -> Result<Timestamp, DateError> {
        let zoned = self
            .0
            .at(hour as i8, minute as i8, 0, 0)
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

    /// Move by whole months, clamping the day into the target month (e.g. Jan 31
    /// + 1 month → Feb 28). Used to navigate a calendar grid.
    pub fn checked_add_months(self, months: i32) -> Option<Self> {
        Some(Self(
            self.0.checked_add(jiff::Span::new().months(months)).ok()?,
        ))
    }

    /// The first day of this date's month.
    pub fn first_of_month(self) -> Self {
        Self(self.0.first_of_month())
    }

    /// The number of days in this date's month.
    pub fn days_in_month(self) -> u8 {
        self.0.days_in_month() as u8
    }

    /// The weekday as a Monday-zero index (Monday = 0 … Sunday = 6).
    pub fn weekday_index(self) -> u8 {
        self.0.weekday().to_monday_zero_offset() as u8
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

impl<M> Encode<M> for Date {
    type Encode = Self;

    #[inline]
    fn encode<E>(&self, encoder: E) -> Result<(), E::Error>
    where
        E: Encoder<Mode = M>,
    {
        encoder.collect_string(self)
    }

    #[inline]
    fn as_encode(&self) -> &Self::Encode {
        self
    }
}

impl<'de, M, A> Decode<'de, M, A> for Date
where
    A: Allocator,
{
    fn decode<D>(decoder: D) -> Result<Self, D::Error>
    where
        D: Decoder<'de, Mode = M, Allocator = A>,
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

#[cfg(feature = "yew")]
impl IntoPropValue<VNode> for Date {
    #[inline]
    fn into_prop_value(self) -> VNode {
        self.to_string().into()
    }
}
