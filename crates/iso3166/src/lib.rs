//! ISO 3166-1 country list.
//!
//! Modeled on the `iso639` crate: the [`ENTRIES`] table is generated at build time by `build.rs`
//! from `data/all.csv` (refresh it with `cargo run -p tools --bin update -- download-countries`).
//!
//! Each [`Country`] records whether a flag asset is available (`has_flag`) and, via
//! [`Countries::flag`], the code used to render it. Flags are rendered with the existing
//! `classes!("flag", code)` CSS pattern (lowercase alpha-2 code), the same one the language picker
//! uses for its country flags.

use std::cell::LazyCell;
use std::collections::BTreeMap;
use std::sync::Arc;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Country {
    /// ISO 3166-1 alpha-2 code, lowercase (also the CSS flag code).
    pub alpha2: &'static str,
    /// English short name.
    pub name: &'static str,
    /// Whether a flag asset is available for this country.
    pub has_flag: bool,
}

#[allow(dead_code)]
mod generated_countries {
    use super::Country;
    include!(concat!(env!("OUT_DIR"), "/generated_countries.rs"));
}

pub use generated_countries::ENTRIES;

#[derive(Debug, Clone)]
pub struct Countries {
    by_alpha2: Arc<BTreeMap<&'static str, usize>>,
}

impl Countries {
    pub fn new() -> Self {
        let by_alpha2 = LazyCell::new(|| {
            Arc::new(
                ENTRIES
                    .iter()
                    .enumerate()
                    .map(|(index, entry)| (entry.alpha2, index))
                    .collect::<BTreeMap<_, _>>(),
            )
        });

        Self {
            by_alpha2: (*by_alpha2).clone(),
        }
    }

    /// Look up a country by its alpha-2 code (case-insensitive).
    pub fn get(&self, alpha2: &str) -> Option<&'static Country> {
        let alpha2 = alpha2.trim().to_lowercase();
        self.by_alpha2
            .get(alpha2.as_str())
            .map(|index| &ENTRIES[*index])
    }

    /// Iterate over all known countries, ordered by alpha-2 code.
    pub fn iter(&self) -> impl Iterator<Item = &'static Country> + '_ {
        self.by_alpha2.values().map(|index| &ENTRIES[*index])
    }

    /// The flag code to render for a country, if a flag asset is available.
    pub fn flag(&self, alpha2: &str) -> Option<&'static str> {
        let entry = self.get(alpha2)?;
        entry.has_flag.then_some(entry.alpha2)
    }
}

impl Default for Countries {
    #[inline]
    fn default() -> Self {
        Self::new()
    }
}
