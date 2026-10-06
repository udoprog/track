use core::fmt;

#[cfg(feature = "yew")]
use implicit_clone::unsync::IString;
#[cfg(feature = "yew")]
use yew::html::IntoPropValue;
#[cfg(feature = "yew")]
use yew::virtual_dom::Key;

use crate::macros;

macros::define_code! {
    /// A Country identified by its 2-letter ISO 3166-1 alpha-2 code, stored as
    /// four bytes: the two ASCII letters followed by `0` padding.
    ///
    /// The all-zero value is [`Country::DEFAULT`], a reference-time sentinel
    /// meaning "use the media's own default (original) Country". It is never a
    /// real Country and must never be persisted as data in the `strings` table.
    pub struct Country;
    /// Error produced when a string cannot be parsed as a [`Country`].
    pub struct ParseCountryErr("invalid Country code");
    /// Build a country from its raw representation.
    ///
    /// If the input is not a valid ISO 3166-1 alpha-2 code, returns
    /// [`Country::DEFAULT`].
    ///
    /// ```
    /// use api::Country;
    ///
    /// assert_eq!(Country::new(b"US"), Country::US);
    /// assert_eq!(Country::new(b"us"), Country::US);
    /// assert_eq!(Country::new(b"ZZ"), Country::DEFAULT);
    /// ```
    const fn new valid by iso3166::is_valid_alpha2;
    case to_ascii_uppercase, is_ascii_uppercase;
    expecting "a 2-letter ISO 3166-1 Country code or 'default'";
    stored error "invalid stored country code";
}

impl Country {
    /// United States (`US`).
    pub const US: Country = Country::new(b"US");

    /// Great Britain (`GB`).
    pub const GB: Country = Country::new(b"GB");

    /// Japan (`JP`).
    pub const JP: Country = Country::new(b"JP");

    /// Build from a 2-letter 3166-1 alpha-2 code (case-insensitive).
    pub fn from_iso(code: &str) -> Option<Self> {
        let code = code.trim();

        if code.is_empty() || code.eq_ignore_ascii_case("default") {
            return Some(Self::DEFAULT);
        }

        let mut bytes = [0u8; 4];
        let upper = Self::normalize(&mut bytes, code)?;

        let bytes = match upper.len() {
            2 if iso3166::is_valid_alpha2(upper.as_bytes()) => {
                let &[a, b] = upper.as_bytes() else {
                    return None;
                };

                [a, b, 0, 0]
            }
            _ => return None,
        };

        Some(Self(bytes))
    }

    /// Get a reference to a static ISO 3166 country, or `None` for
    /// [`Country::DEFAULT`].
    #[inline]
    pub fn to_iso(&self) -> Option<&'static iso3166::Country> {
        if self.is_default() {
            return None;
        }

        iso3166::by_alpha2(self.as_raw_code())
    }
}

impl fmt::Debug for Country {
    #[inline]
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Country({self})")
    }
}

#[cfg(feature = "yew")]
impl IntoPropValue<Option<IString>> for Country {
    #[inline]
    fn into_prop_value(self) -> Option<IString> {
        self.to_iso().map(|c| IString::Static(c.name))
    }
}

impl PartialEq<&'static iso3166::Country> for Country {
    #[inline]
    fn eq(&self, other: &&'static iso3166::Country) -> bool {
        self.as_raw_code() == other.alpha2
    }
}

#[cfg(feature = "yew")]
impl From<Country> for Key {
    #[inline]
    fn from(country: Country) -> Self {
        country.as_str().into()
    }
}
