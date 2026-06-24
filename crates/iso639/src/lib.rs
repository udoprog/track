//! ISO 639 languages list.

/// The scope of an ISO 639 [`Language`] entry.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Scope {
    /// An individual language.
    Individual,
    /// A macrolanguage that subsumes several closely related individual
    /// languages.
    Macrolanguage,
    /// A special code (e.g. for undetermined or multiple languages).
    Special,
}

/// The type of an ISO 639 [`Language`] entry.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum Type {
    /// A language still in everyday use by a living community.
    Living,
    /// A language with no remaining living speakers.
    Extinct,
    /// A language that is no longer in common use but is distinct from its
    /// modern descendants.
    Historical,
    /// An artificially created language (e.g. Esperanto).
    Constructed,
    /// A special-purpose code rather than a natural language.
    Special,
}

/// A single ISO 639 language entry.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Language {
    /// The 3-letter ISO 639-3 code, the primary identifier for this language.
    pub id: &'static str,
    /// The 3-letter ISO 639-2/B (bibliographic) code, when one exists.
    pub part2b: Option<&'static str>,
    /// The 3-letter ISO 639-2/T (terminological) code, when one exists.
    pub part2t: Option<&'static str>,
    /// The 2-letter ISO 639-1 code, when one exists.
    pub part1: Option<&'static str>,
    /// ISO 3166-1 flag code to render for this language, when a flag asset is
    /// available.
    pub flag: Option<&'static str>,
    /// The scope of this entry.
    pub scope: Scope,
    /// The type of this entry.
    pub ty: Type,
    /// The English reference name of the language.
    pub name: &'static str,
    /// A free-form comment about the entry, when present.
    pub comment: Option<&'static str>,
}

#[allow(clippy::match_like_matches_macro)]
mod generated;
use generated::ENTRIES;

/// Test if the given byte slice is a valid ISO 639-3 code (3 ASCII letters).
pub const fn is_valid_id(id: &[u8]) -> bool {
    generated::is_valid_id(id)
}

/// Look up a [`Language`] by its 2-letter ISO 639-1 code [`Language::part1`].
///
/// ```
/// let english = iso639::by_part1("en").unwrap();
/// assert_eq!(english.id, "eng");
/// assert_eq!(english.name, "English");
///
/// assert!(iso639::by_part1("zz").is_none());
/// ```
pub fn by_part1(part1: &str) -> Option<&'static Language> {
    generated::BY_PART1.get(part1).and_then(|&i| ENTRIES.get(i))
}

/// Look up a [`Language`] by its 3-letter ISO 639-3 code [`Language::id`].
///
/// ```
/// let english = iso639::by_id("eng").unwrap();
/// assert_eq!(english.part1, Some("en"));
/// assert_eq!(english.name, "English");
///
/// assert!(iso639::by_id("zzz").is_none());
/// ```
pub fn by_id(id: &str) -> Option<&'static Language> {
    generated::BY_ID.get(id).and_then(|&i| ENTRIES.get(i))
}

/// Iterate over every known [`Language`].
///
/// ```
/// assert!(iso639::iter().any(|lang| lang.id == "eng"));
/// ```
pub fn iter() -> impl Iterator<Item = &'static Language> {
    ENTRIES.iter()
}

#[cfg(test)]
mod tests {
    #[test]
    fn test_languages() {
        for entry in super::iter() {
            if entry.part1.is_none() {
                continue;
            }

            assert!(
                entry.flag.is_some(),
                "language {:?} with ISO-639-1 code {:?} is missing from the to-3166-1 mapping",
                entry.id,
                entry.part1
            );
        }
    }
}
