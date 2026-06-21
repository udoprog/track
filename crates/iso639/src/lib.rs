use std::collections::{HashMap, HashSet};
use std::sync::LazyLock;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Scope {
    Individual,
    Macrolanguage,
    Special,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LanguageType {
    Living,
    Extinct,
    Historical,
    Constructed,
    Special,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Entry {
    pub id: &'static str,
    pub part2b: Option<&'static str>,
    pub part2t: Option<&'static str>,
    pub part1: Option<&'static str>,
    pub scope: Scope,
    pub language_type: LanguageType,
    pub ref_name: &'static str,
    pub comment: Option<&'static str>,
}

#[allow(dead_code)]
mod generated {
    use super::{Entry, LanguageType, Scope};
    include!(concat!(env!("OUT_DIR"), "/generated.rs"));
}

#[allow(dead_code)]
mod generated_to_3166_1 {
    include!(concat!(env!("OUT_DIR"), "/generated_to_3166_1.rs"));
}

#[allow(dead_code)]
mod generated_countries {
    include!(concat!(env!("OUT_DIR"), "/generated_countries.rs"));
}

pub use generated::ENTRIES;

static BY_PART1: LazyLock<HashMap<&'static str, usize>> = LazyLock::new(|| {
    generated::ENTRIES
        .iter()
        .enumerate()
        .flat_map(|(index, entry)| Some((entry.part1?, index)))
        .collect::<HashMap<_, _>>()
});

static BY_ID: LazyLock<HashMap<&'static str, usize>> = LazyLock::new(|| {
    generated::ENTRIES
        .iter()
        .enumerate()
        .map(|(index, entry)| (entry.id, index))
        .collect::<HashMap<_, _>>()
});

static LANGUAGE_PART1_TO_COUNTRY: LazyLock<HashMap<&'static str, &'static str>> =
    LazyLock::new(|| {
        generated_to_3166_1::TO_3166_1
            .iter()
            .copied()
            .collect::<HashMap<_, _>>()
    });

static COUNTRIES: LazyLock<HashSet<&'static str>> = LazyLock::new(|| {
    generated_countries::COUNTRIES
        .iter()
        .copied()
        .collect::<HashSet<_>>()
});

pub fn by_part1(part1: &str) -> Option<&'static Entry> {
    BY_PART1.get(part1).map(|&i| &ENTRIES[i])
}

/// Look up an entry by its 3-letter ISO 639-3 code (`Entry::id`).
pub fn by_id(id: &str) -> Option<&'static Entry> {
    BY_ID.get(id).map(|&i| &ENTRIES[i])
}

pub fn iter() -> impl Iterator<Item = &'static Entry> {
    ENTRIES.iter()
}

/// Return the ISO 3166-1 alpha-2 country code corresponding to the given ISO
/// 639-1 language code, if any.
pub fn country_by_part1(part1: &str) -> Option<&'static str> {
    LANGUAGE_PART1_TO_COUNTRY.get(part1).copied()
}

pub fn is_id_country(code: &str) -> bool {
    let Some(entry) = by_id(code) else {
        return false;
    };

    let Some(part1) = entry.part1 else {
        return false;
    };

    COUNTRIES.contains(part1)
}

#[cfg(test)]
mod tests {
    #[test]
    fn test_languages() {
        for entry in super::iter() {
            let Some(part1) = entry.part1 else {
                continue;
            };

            assert!(
                super::country_by_part1(part1).is_some(),
                "language with ISO-639-1 code {part1:?} is missing from to-3166-1 mapping"
            );
        }
    }
}
