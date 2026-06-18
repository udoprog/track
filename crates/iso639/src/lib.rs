use std::cell::LazyCell;
use std::collections::{BTreeMap, HashSet};
use std::sync::Arc;

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

#[derive(Debug, Clone)]
pub struct Languages {
    by_part1: Arc<BTreeMap<&'static str, usize>>,
    by_id: Arc<BTreeMap<&'static str, usize>>,
}

impl Languages {
    pub fn new() -> Self {
        let by_part1 = LazyCell::new(|| {
            Arc::new(
                generated::PART1_MAP
                    .iter()
                    .copied()
                    .collect::<BTreeMap<_, _>>(),
            )
        });
        let by_id = LazyCell::new(|| {
            Arc::new(
                ENTRIES
                    .iter()
                    .enumerate()
                    .map(|(index, entry)| (entry.id, index))
                    .collect::<BTreeMap<_, _>>(),
            )
        });
        Self {
            by_part1: (*by_part1).clone(),
            by_id: (*by_id).clone(),
        }
    }

    pub fn get_by_part1(&self, part1: &str) -> Option<&'static Entry> {
        self.by_part1.get(part1).map(|index| &ENTRIES[*index])
    }

    /// Look up an entry by its 3-letter ISO 639-3 code (`Entry::id`).
    pub fn get_by_id(&self, id: &str) -> Option<&'static Entry> {
        self.by_id.get(id).map(|index| &ENTRIES[*index])
    }

    pub fn iter(&self) -> impl Iterator<Item = (&'static str, &'static Entry)> + '_ {
        self.by_part1
            .iter()
            .map(|(part1, index)| (*part1, &ENTRIES[*index]))
    }
}

impl Default for Languages {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Debug, Clone)]
pub struct LanguageToCountry {
    by_part1: Arc<BTreeMap<&'static str, &'static str>>,
}

impl LanguageToCountry {
    pub fn new() -> Self {
        let by_part1 = LazyCell::new(|| {
            Arc::new(
                generated_to_3166_1::TO_3166_1
                    .iter()
                    .copied()
                    .collect::<BTreeMap<_, _>>(),
            )
        });

        Self {
            by_part1: (*by_part1).clone(),
        }
    }

    pub fn get_by_part1(&self, part1: &str) -> Option<&'static str> {
        self.by_part1.get(part1).copied()
    }

    pub fn iter(&self) -> impl Iterator<Item = (&'static str, &'static str)> + '_ {
        self.by_part1
            .iter()
            .map(|(part1, country)| (*part1, *country))
    }
}

impl Default for LanguageToCountry {
    #[inline]
    fn default() -> Self {
        Self::new()
    }
}

pub struct Countries {
    values: Arc<HashSet<&'static str>>,
}

impl Countries {
    pub fn new() -> Self {
        let values = LazyCell::new(|| {
            Arc::new(
                generated_countries::COUNTRIES
                    .iter()
                    .copied()
                    .collect::<HashSet<_>>(),
            )
        });

        Self {
            values: (*values).clone(),
        }
    }

    pub fn get(&self, country: &str) -> Option<String> {
        let country = country.trim().to_lowercase();

        if self.values.contains(country.as_str()) {
            Some(country)
        } else {
            None
        }
    }
}

impl Default for Countries {
    #[inline]
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::{LanguageToCountry, Languages};

    #[test]
    fn test_languages() {
        let languages = Languages::new();
        let language_to_country = LanguageToCountry::new();

        for (_, entry) in languages.iter() {
            let Some(part1) = entry.part1 else {
                continue;
            };

            assert!(
                language_to_country.get_by_part1(part1).is_some(),
                "language with ISO-639-1 code {part1:?} is missing from to-3166-1 mapping"
            );
        }
    }
}
