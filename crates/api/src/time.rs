use core::fmt;
use core::str::FromStr;
use std::time::Duration;

use jiff::Timestamp as JiffTimestamp;
use jiff::civil::Date as JiffDate;
use jiff::tz::TimeZone as JiffTimeZone;
use musli_core::{Allocator, Context as _, Decode, Decoder, Encode, Encoder};

#[cfg(feature = "yew")]
use implicit_clone::unsync::IString;
#[cfg(feature = "yew")]
use yew::AttrValue;
#[cfg(feature = "yew")]
use yew::html::IntoPropValue;
#[cfg(feature = "yew")]
use yew::virtual_dom::VNode;

#[derive(Debug, PartialEq, Eq, Clone)]
pub struct TimeInfo {
    tz: TimeZone,
    now: Timestamp,
}

impl TimeInfo {
    #[inline]
    pub fn new(tz: TimeZone, now: Timestamp) -> Self {
        Self { tz, now }
    }

    #[inline]
    pub fn tz(&self) -> &TimeZone {
        &self.tz
    }

    #[inline]
    pub fn now(&self) -> Timestamp {
        self.now
    }

    #[inline]
    pub fn date(&self) -> Date {
        self.now.date(self.clone())
    }

    #[inline]
    pub fn hour_minute(&self) -> (u8, u8) {
        self.now.hour_minute(self.clone())
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TimeZone(JiffTimeZone);

impl TimeZone {
    /// The UTC TimeZone.
    pub const UTC: Self = Self(JiffTimeZone::UTC);

    /// The system's local TimeZone.
    #[inline]
    pub fn system() -> Self {
        Self(JiffTimeZone::system())
    }

    /// Get a TimeZone by IANA name (e.g. "America/New_York"). Returns `None` if
    /// the name is invalid or not supported by the current platform.
    #[inline]
    pub fn get(s: &str) -> Option<Self> {
        Some(Self(JiffTimeZone::get(s).ok()?))
    }

    #[inline]
    pub fn iana_name(&self) -> Option<&str> {
        self.0.iana_name()
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
    pub fn from_now(duration: Duration) -> Self {
        let ms = JiffTimestamp::now().as_millisecond() + duration.as_millis() as i64;
        Self(JiffTimestamp::from_millisecond(ms).unwrap_or_else(|_| JiffTimestamp::now()))
    }

    /// The duration from `earlier` until this timestamp, or `None` if this
    /// timestamp is not after `earlier`.
    #[inline]
    pub fn checked_duration_since(self, earlier: Timestamp) -> Option<Duration> {
        let ms = self.0.as_millisecond() - earlier.0.as_millisecond();
        u64::try_from(ms).ok().map(Duration::from_millis)
    }

    /// The absolute duration between this timestamp and `earlier`, regardless
    /// of which is earlier.
    #[inline]
    pub fn absolute_duration_since(self, earlier: Timestamp) -> Duration {
        let ms = self.0.as_millisecond() - earlier.0.as_millisecond();
        Duration::from_millis(ms.unsigned_abs())
    }

    /// This timestamp shifted forward by `duration`, saturating at the
    /// representable range.
    #[inline]
    pub fn saturating_add(self, duration: crate::Duration) -> Timestamp {
        let ms = self.0.as_millisecond().saturating_add(duration.millis());

        Self(JiffTimestamp::from_millisecond(ms).unwrap_or(if ms < 0 {
            JiffTimestamp::MIN
        } else {
            JiffTimestamp::MAX
        }))
    }

    #[inline]
    pub fn from_jiff(ts: JiffTimestamp) -> Self {
        Self(ts)
    }

    /// Format this timestamp in the given timezone as `"YYYY-MM-DD HH:MM TZ"`.
    /// The timezone suffix is the IANA abbreviation (e.g. `CEST`, `EST`) when
    /// available, or the numeric offset (e.g. `+05:30`) for fixed-offset zones.
    #[inline]
    pub fn date_and_time(&self, time: TimeInfo) -> String {
        self.0
            .to_zoned(time.tz.0)
            .strftime("%Y-%m-%d %H:%M %Z")
            .to_string()
    }

    #[inline]
    pub fn human_date(&self, time: TimeInfo) -> HumanDate {
        let today = time.now().date(time.clone());
        let date = self.date(time.clone());

        let kind = 'kind: {
            if date == today {
                break 'kind HumanDateKind::Special(Special::Today);
            }

            if date == today.checked_sub_days(1).unwrap_or(date) {
                break 'kind HumanDateKind::Special(Special::Yesterday);
            }

            if date == today.checked_add_days(1).unwrap_or(date) {
                break 'kind HumanDateKind::Special(Special::Tomorrow);
            }

            HumanDateKind::Date(date)
        };

        HumanDate {
            kind,
            lower: false,
            same_year: date.year() == today.year(),
        }
    }

    #[inline]
    pub fn human_date_time(&self, time: TimeInfo) -> HumanDateTime {
        let past = *self < time.now();
        let date = self.human_date(time.clone());
        let time_of_day = self.time_of_day(time);
        HumanDateTime {
            past,
            date,
            time_of_day,
        }
    }

    #[inline]
    pub fn date(&self, time: TimeInfo) -> Date {
        Date(self.0.to_zoned(time.tz.0).date())
    }

    /// Format just the local time of day (`"HH:MM"`, 24-hour) in the given timezone.
    #[inline]
    pub fn time_of_day(&self, time: TimeInfo) -> TimeOfDay {
        TimeOfDay {
            zoned: self.0.to_zoned(time.tz.0),
        }
    }

    /// The local `(hour, minute)` of this timestamp in the given timezone.
    #[inline]
    pub fn hour_minute(&self, time: TimeInfo) -> (u8, u8) {
        let zoned = self.0.to_zoned(time.tz.0);
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
    #[inline]
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}

impl<M> Encode<M> for Timestamp {
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

/// Serialized as UTC milliseconds, matching how a `Timestamp` binds to a SQL column
/// (and how the backup format writes one), so a value round-trips through JSON and
/// through storage identically.
impl serde::Serialize for Timestamp {
    #[inline]
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.serialize_i64(self.0.as_millisecond())
    }
}

impl<'de> serde::Deserialize<'de> for Timestamp {
    #[inline]
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let ms = i64::deserialize(deserializer)?;

        JiffTimestamp::from_millisecond(ms)
            .map(Timestamp)
            .map_err(serde::de::Error::custom)
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

/// A human-friendly date relative to "today" in a given timezone, used for display.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct HumanDate {
    kind: HumanDateKind,
    lower: bool,
    same_year: bool,
}

impl HumanDate {
    #[inline]
    pub fn lower(self) -> Self {
        Self {
            kind: self.kind,
            lower: true,
            same_year: self.same_year,
        }
    }

    #[cfg(feature = "yew")]
    pub fn view(&self) -> VNode {
        match self.kind {
            HumanDateKind::Special(special) if self.lower => {
                yew::html!(<date class="special">{special.lower()}</date>)
            }
            HumanDateKind::Special(special) => {
                yew::html!(<date class="special">{special.upper()}</date>)
            }
            HumanDateKind::Date(date) if self.same_year => {
                yew::html!(<date>{format_date(date.0).to_string()}</date>)
            }
            HumanDateKind::Date(date) => {
                yew::html!(<date>{format_date_year(date.0).to_string()}</date>)
            }
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Special {
    Today,
    Yesterday,
    Tomorrow,
}

impl Special {
    fn lower(&self) -> &'static str {
        match self {
            Special::Today => "today",
            Special::Yesterday => "yesterday",
            Special::Tomorrow => "tomorrow",
        }
    }

    fn upper(&self) -> &'static str {
        match self {
            Special::Today => "Today",
            Special::Yesterday => "Yesterday",
            Special::Tomorrow => "Tomorrow",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum HumanDateKind {
    Special(Special),
    Date(Date),
}

fn nth(day: i8) -> impl fmt::Display {
    let suffix = match day {
        11..=13 => "th",
        _ => match day % 10 {
            1 => "st",
            2 => "nd",
            3 => "rd",
            _ => "th",
        },
    };

    fmt::from_fn(move |f| write!(f, "{day}{suffix}"))
}

fn month(month: i8) -> &'static str {
    match month {
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

fn format_date(date: jiff::civil::Date) -> impl fmt::Display {
    fmt::from_fn(move |f| write!(f, "{} of {}", nth(date.day()), month(date.month())))
}

fn format_date_year(date: jiff::civil::Date) -> impl fmt::Display {
    fmt::from_fn(move |f| {
        write!(
            f,
            "{} of {}, {}",
            nth(date.day()),
            month(date.month()),
            date.year()
        )
    })
}

impl fmt::Display for HumanDate {
    #[inline]
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.kind {
            HumanDateKind::Special(special) if self.lower => special.lower().fmt(f),
            HumanDateKind::Special(special) => special.upper().fmt(f),
            HumanDateKind::Date(date) if self.same_year => format_date(date.0).fmt(f),
            HumanDateKind::Date(date) => format_date_year(date.0).fmt(f),
        }
    }
}

#[cfg(feature = "yew")]
impl IntoPropValue<VNode> for HumanDate {
    #[inline]
    fn into_prop_value(self) -> VNode {
        self.view()
    }
}

/// A human-friendly date and time relative to "today" in a given timezone, used for display.
pub struct HumanDateTime {
    date: HumanDate,
    time_of_day: TimeOfDay,
    past: bool,
}

impl HumanDateTime {
    #[inline]
    pub fn lower(self) -> Self {
        Self {
            date: self.date.lower(),
            time_of_day: self.time_of_day,
            past: self.past,
        }
    }

    #[inline]
    pub fn is_past(&self) -> bool {
        self.past
    }

    #[inline]
    #[cfg(feature = "yew")]
    pub fn view(&self) -> VNode {
        yew::html! {
            <datetime>
                {self.date.view()}
                <span>{"at"}</span>
                {self.time_of_day.view()}
            </datetime>
        }
    }
}

impl fmt::Display for HumanDateTime {
    #[inline]
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} at {}", self.date, self.time_of_day)
    }
}

#[cfg(feature = "yew")]
impl IntoPropValue<VNode> for HumanDateTime {
    #[inline]
    fn into_prop_value(self) -> VNode {
        self.view()
    }
}

#[cfg(feature = "yew")]
impl IntoPropValue<Option<IString>> for HumanDateTime {
    #[inline]
    fn into_prop_value(self) -> Option<IString> {
        Some(self.to_string().into())
    }
}

#[cfg(feature = "yew")]
impl From<HumanDateTime> for AttrValue {
    #[inline]
    fn from(hdt: HumanDateTime) -> Self {
        hdt.to_string().into()
    }
}

/// A local time of day in a given timezone, used for display.
#[derive(Clone)]
pub struct TimeOfDay {
    zoned: jiff::Zoned,
}

impl TimeOfDay {
    #[inline]
    #[cfg(feature = "yew")]
    pub fn view(&self) -> VNode {
        yew::html!(<time>{self.to_string()}</time>)
    }
}

impl fmt::Display for TimeOfDay {
    #[inline]
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.zoned.strftime("%H:%M").fmt(f)
    }
}

#[cfg(feature = "yew")]
impl IntoPropValue<VNode> for TimeOfDay {
    #[inline]
    fn into_prop_value(self) -> VNode {
        self.view()
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
            .to_zoned(tz.0)
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
