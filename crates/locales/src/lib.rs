//! Valid language+country locale combinations.
//!
//! The only payload is the set of *valid* pairings of an ISO 639-3 language with
//! an ISO 3166-1 country (e.g. `en-US`, `pt-BR`), sourced from SimpleLocalize.
//! Richer per-language and per-country metadata lives in the `iso639` and
//! `iso3166` crates.

/// A single valid language+country combination.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Locale {
    /// ISO 639-3 id (3-letter, lowercase), matching `iso639::Language::id`.
    pub language: &'static str,
    /// ISO 3166-1 alpha-2 code (uppercase), matching `iso3166::Country::alpha2`.
    pub country: &'static str,
}

mod generated;
use generated::ENTRIES;

/// Iterate over every valid locale, ordered by language then country.
///
/// ```
/// assert!(locales::iter().any(|l| l.language == "eng" && l.country == "US"));
/// ```
pub fn iter() -> impl Iterator<Item = &'static Locale> {
    ENTRIES.iter()
}

/// Whether the given (3-letter language id, uppercase alpha-2 country) pair is a
/// valid locale.
///
/// ```
/// assert!(locales::contains("eng", "US"));
/// assert!(!locales::contains("eng", "ZZ"));
/// ```
pub fn contains(language: &str, country: &str) -> bool {
    generated::BY_KEY
        .get(&key(language, country))
        .and_then(|&i| ENTRIES.get(i))
        .is_some()
}

/// Build the `"<language>-<country>"` lookup key used by [`generated::BY_KEY`].
fn key(language: &str, country: &str) -> String {
    let mut s = String::with_capacity(language.len() + 1 + country.len());
    s.push_str(language);
    s.push('-');
    s.push_str(country);
    s
}
