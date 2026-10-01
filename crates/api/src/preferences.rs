//! User preferences and their stored form: one row per non-default value, keyed
//! by a [`PreferenceKey`] and holding the value as JSON text.

use core::fmt;
use core::str::FromStr;

use musli_core::{Decode, Encode};

use crate::{Duration, IncludeSpecials, Locale, ThemeType};

/// What a preference row belongs to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PreferenceScope {
    /// A user's own preferences (`user_config`).
    User,
    /// A user's preferences for one show (`user_show_config`).
    Show,
    /// A user's preferences for one movie (`user_movie_config`).
    Movie,
}

macro_rules! keys {
    ($($variant:ident = $text:literal),* $(,)?) => {
        /// The key of a stored preference.
        #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
        pub enum PreferenceKey {
            $($variant),*
        }

        impl PreferenceKey {
            /// Every key, in declaration order.
            pub const ALL: &'static [PreferenceKey] = &[$(PreferenceKey::$variant),*];

            /// The stored text of this key.
            pub const fn as_str(self) -> &'static str {
                match self {
                    $(PreferenceKey::$variant => $text),*
                }
            }
        }

        impl FromStr for PreferenceKey {
            type Err = PreferenceError;

            fn from_str(s: &str) -> Result<Self, Self::Err> {
                match s {
                    $($text => Ok(PreferenceKey::$variant),)*
                    _ => Err(PreferenceError::UnknownKey),
                }
            }
        }
    };
}

keys! {
    Theme = "theme",
    DashboardPage = "dashboard-page",
    DashboardLookahead = "dashboard-lookahead",
    ScheduleWeeks = "schedule-weeks",
    ScheduleRangeDays = "schedule-range-days",
    Timezone = "timezone",
    Language = "language",
    IncludeSpecials = "include-specials",
}

impl PreferenceKey {
    /// Whether this key may be stored in `scope`.
    pub fn allowed_in(self, scope: PreferenceScope) -> bool {
        match self {
            PreferenceKey::Language => true,
            PreferenceKey::IncludeSpecials => scope != PreferenceScope::Movie,
            _ => scope == PreferenceScope::User,
        }
    }
}

impl fmt::Display for PreferenceKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

#[cfg(feature = "sqll")]
impl ::sqll::BindValue for PreferenceKey {
    fn bind_value(&self, stmt: &mut ::sqll::Statement, index: ::sqll::Index) -> ::sqll::Result<()> {
        self.as_str().bind_value(stmt, index)
    }
}

/// Why a stored preference could not be read.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PreferenceError {
    UnknownKey,
    NotAllowed,
    InvalidValue,
}

impl fmt::Display for PreferenceError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            PreferenceError::UnknownKey => f.write_str("unknown preference key"),
            PreferenceError::NotAllowed => f.write_str("preference not allowed here"),
            PreferenceError::InvalidValue => f.write_str("unreadable preference value"),
        }
    }
}

impl std::error::Error for PreferenceError {}

/// A value stored as the JSON text of a preference row.
pub trait PreferenceValue: Sized {
    fn to_json(&self) -> String;

    fn from_json(json: &str) -> Option<Self>;
}

macro_rules! serde_value {
    ($($ty:ty),*) => {
        $(
            impl PreferenceValue for $ty {
                fn to_json(&self) -> String {
                    serde_json::to_string(self).expect("preference values serialize")
                }

                fn from_json(json: &str) -> Option<Self> {
                    serde_json::from_str(json).ok()
                }
            }
        )*
    };
}

serde_value!(bool, u32, String, Locale, ThemeType);

/// Milliseconds, as a JSON number.
impl PreferenceValue for Duration {
    fn to_json(&self) -> String {
        self.millis().to_string()
    }

    fn from_json(json: &str) -> Option<Self> {
        serde_json::from_str::<i64>(json)
            .ok()
            .map(Duration::from_millis)
    }
}

/// `true` or `false`; [`IncludeSpecials::Default`] is the missing row.
impl PreferenceValue for IncludeSpecials {
    fn to_json(&self) -> String {
        match self {
            IncludeSpecials::Default => "null".to_owned(),
            IncludeSpecials::Include => "true".to_owned(),
            IncludeSpecials::Skip => "false".to_owned(),
        }
    }

    fn from_json(json: &str) -> Option<Self> {
        match serde_json::from_str::<Option<bool>>(json).ok()? {
            Some(true) => Some(IncludeSpecials::Include),
            Some(false) => Some(IncludeSpecials::Skip),
            None => Some(IncludeSpecials::Default),
        }
    }
}

/// A user's own preferences.
#[derive(Debug, Clone, PartialEq, Encode, Decode)]
#[musli(crate = musli_core)]
pub struct Preferences {
    pub theme: ThemeType,
    pub dashboard_page: u32,
    /// How far into the future pending items surface on the dashboard. A
    /// pending item is shown once its timestamp falls within this window.
    pub dashboard_lookahead: Duration,
    /// Number of weeks shown in the dashboard schedule (always at least 1).
    pub schedule_weeks: u32,
    /// Number of days shown in the dashboard's upcoming-days strip (always at
    /// least 1).
    pub schedule_range_days: u32,
    pub timezone: String,
    /// The display locale. [`Locale::DEFAULT`] means "use each show's/movie's
    /// own original language".
    pub language: Locale,
    pub include_specials: bool,
}

impl Default for Preferences {
    fn default() -> Self {
        Self {
            theme: ThemeType::Dark,
            dashboard_page: 5,
            dashboard_lookahead: Duration::from_hours(24),
            schedule_weeks: 4,
            schedule_range_days: 3,
            timezone: String::new(),
            language: Locale::DEFAULT,
            include_specials: false,
        }
    }
}

impl Preferences {
    /// The values that differ from the default, as stored rows.
    pub fn encode(&self) -> Vec<(PreferenceKey, String)> {
        let default = Self::default();
        let mut out = Vec::new();

        macro_rules! field {
            ($key:ident, $field:ident) => {
                if self.$field != default.$field {
                    out.push((PreferenceKey::$key, self.$field.to_json()));
                }
            };
        }

        field!(Theme, theme);
        field!(DashboardPage, dashboard_page);
        field!(DashboardLookahead, dashboard_lookahead);
        field!(ScheduleWeeks, schedule_weeks);
        field!(ScheduleRangeDays, schedule_range_days);
        field!(Timezone, timezone);
        field!(Language, language);
        field!(IncludeSpecials, include_specials);
        out
    }

    /// Apply one stored row.
    pub fn decode(&mut self, key: &str, json: &str) -> Result<(), PreferenceError> {
        fn value<T: PreferenceValue>(json: &str) -> Result<T, PreferenceError> {
            T::from_json(json).ok_or(PreferenceError::InvalidValue)
        }

        match key.parse()? {
            PreferenceKey::Theme => self.theme = value(json)?,
            PreferenceKey::DashboardPage => self.dashboard_page = value(json)?,
            PreferenceKey::DashboardLookahead => self.dashboard_lookahead = value(json)?,
            PreferenceKey::ScheduleWeeks => self.schedule_weeks = value(json)?,
            PreferenceKey::ScheduleRangeDays => self.schedule_range_days = value(json)?,
            PreferenceKey::Timezone => self.timezone = value(json)?,
            PreferenceKey::Language => self.language = value(json)?,
            PreferenceKey::IncludeSpecials => self.include_specials = value(json)?,
        }

        Ok(())
    }
}
