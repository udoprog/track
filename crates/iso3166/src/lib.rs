//! ISO 3166-1 country list.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Country {
    /// ISO 3166-1 alpha-2 code, uppercase (also the CSS flag class name).
    pub alpha2: &'static str,
    /// English short name.
    pub name: &'static str,
    /// Whether a flag asset is available for this country.
    pub has_flag: bool,
}

mod generated;
use generated::ENTRIES;

/// Whether a 2-letter code is a valid ISO 3166-1 alpha-2 code.
pub const fn is_valid_alpha2(alpha2: &[u8]) -> bool {
    generated::is_valid_alpha2(alpha2)
}

/// Look up a country by its uppercase alpha-2 code.
///
/// The lookup is case-sensitive; pass an uppercase code such as `"US"`.
///
/// ```
/// let us = iso3166::by_alpha2("US").unwrap();
/// assert_eq!(us.name, "United States of America");
///
/// assert!(iso3166::by_alpha2("us").is_none());
/// assert!(iso3166::by_alpha2("ZZ").is_none());
/// ```
pub fn by_alpha2(alpha2: &str) -> Option<&'static Country> {
    generated::BY_ALPHA2
        .get(alpha2)
        .and_then(|&i| ENTRIES.get(i))
}

/// Iterate over all known countries, ordered by alpha-2 code.
///
/// ```
/// assert!(iso3166::iter().any(|c| c.alpha2 == "US"));
/// assert!(iso3166::iter().is_sorted_by_key(|c| c.alpha2));
/// ```
pub fn iter() -> impl Iterator<Item = &'static Country> {
    ENTRIES.iter()
}

/// The flag class name to render for a country, if a flag asset is available.
///
/// ```
/// assert_eq!(iso3166::flag_by_alpha2("US"), Some("US"));
/// assert!(iso3166::flag_by_alpha2("zz").is_none());
/// ```
pub fn flag_by_alpha2(alpha2: &str) -> Option<&'static str> {
    let entry = by_alpha2(alpha2)?;
    entry.has_flag.then_some(entry.alpha2)
}
