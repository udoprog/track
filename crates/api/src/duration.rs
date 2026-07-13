use core::fmt;
use core::num::ParseIntError;
use core::str::FromStr;

use musli_core::{Decode, Encode};

/// A duration with millisecond precision.
///
/// The [`Display`][fmt::Display] and [`FromStr`] implementations use the raw
/// millisecond count so the value round-trips exactly through storage. Use
/// [`Duration::human`] for a human-readable rendering.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Default, Encode, Decode)]
#[musli(crate = musli_core, transparent)]
pub struct Duration(i64);

impl Duration {
    /// The zero duration.
    pub const ZERO: Self = Self(0);

    #[inline]
    pub const fn from_millis(millis: i64) -> Self {
        Self(millis)
    }

    #[inline]
    pub const fn from_hours(hours: i64) -> Self {
        Self(hours * DurationUnit::Hour.millis())
    }

    #[inline]
    pub const fn millis(self) -> i64 {
        self.0
    }

    /// Split this duration into an amount and the largest unit it is at least
    /// one of, such as `(1.5, Hour)` for 90 minutes. Durations smaller than a
    /// second are expressed in milliseconds.
    pub fn split(self) -> (f64, DurationUnit) {
        for unit in DurationUnit::ALL {
            if self.0.unsigned_abs() >= unit.millis().unsigned_abs() {
                return (self.0 as f64 / unit.millis() as f64, unit);
            }
        }

        (self.0 as f64, DurationUnit::Millisecond)
    }

    /// A human-readable rendering of this duration, such as `24 hours`.
    #[inline]
    pub fn human(self) -> HumanDuration {
        HumanDuration(self)
    }
}

impl fmt::Display for Duration {
    #[inline]
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}

impl FromStr for Duration {
    type Err = ParseIntError;

    #[inline]
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(Self(s.parse::<i64>()?))
    }
}

/// The unit a [`Duration`] is expressed in when displayed or edited, largest
/// first.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DurationUnit {
    Week,
    Day,
    Hour,
    Minute,
    Second,
    Millisecond,
}

impl DurationUnit {
    /// All units, largest first.
    pub const ALL: [DurationUnit; 6] = [
        DurationUnit::Week,
        DurationUnit::Day,
        DurationUnit::Hour,
        DurationUnit::Minute,
        DurationUnit::Second,
        DurationUnit::Millisecond,
    ];

    /// The number of milliseconds in one of this unit.
    #[inline]
    pub const fn millis(self) -> i64 {
        match self {
            DurationUnit::Week => 604_800_000,
            DurationUnit::Day => 86_400_000,
            DurationUnit::Hour => 3_600_000,
            DurationUnit::Minute => 60_000,
            DurationUnit::Second => 1_000,
            DurationUnit::Millisecond => 1,
        }
    }

    /// The stable identifier for this unit.
    #[inline]
    pub const fn as_str(self) -> &'static str {
        match self {
            DurationUnit::Week => "week",
            DurationUnit::Day => "day",
            DurationUnit::Hour => "hour",
            DurationUnit::Minute => "minute",
            DurationUnit::Second => "second",
            DurationUnit::Millisecond => "millisecond",
        }
    }

    /// The singular name of this unit.
    #[inline]
    pub const fn singular(self) -> &'static str {
        self.as_str()
    }

    /// The plural name of this unit.
    #[inline]
    pub const fn plural(self) -> &'static str {
        match self {
            DurationUnit::Week => "weeks",
            DurationUnit::Day => "days",
            DurationUnit::Hour => "hours",
            DurationUnit::Minute => "minutes",
            DurationUnit::Second => "seconds",
            DurationUnit::Millisecond => "milliseconds",
        }
    }

    /// The name of this unit, pluralized for `amount`.
    #[inline]
    pub fn name(self, amount: f64) -> &'static str {
        if amount == 1.0 {
            self.singular()
        } else {
            self.plural()
        }
    }
}

impl FromStr for DurationUnit {
    type Err = ParseDurationUnitErr;

    #[inline]
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        DurationUnit::ALL
            .into_iter()
            .find(|unit| unit.as_str() == s)
            .ok_or(ParseDurationUnitErr)
    }
}

/// Error raised when a [`DurationUnit`] cannot be parsed.
#[derive(Debug)]
pub struct ParseDurationUnitErr;

impl fmt::Display for ParseDurationUnitErr {
    #[inline]
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Not a duration unit")
    }
}

impl std::error::Error for ParseDurationUnitErr {}

/// A human-readable [`Duration`], such as `24 hours` or `1.5 minutes`.
#[derive(Debug, Clone, Copy)]
pub struct HumanDuration(Duration);

impl fmt::Display for HumanDuration {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let (amount, unit) = self.0.split();
        write!(f, "{} {}", format_amount(amount), unit.name(amount))
    }
}

/// Format an amount with at most three decimals and no trailing zeroes.
fn format_amount(amount: f64) -> String {
    if amount.fract() == 0.0 {
        return format!("{amount:.0}");
    }

    let formatted = format!("{amount:.3}");
    let trimmed = formatted.trim_end_matches('0').trim_end_matches('.');
    trimmed.to_owned()
}
