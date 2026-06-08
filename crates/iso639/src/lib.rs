use std::collections::BTreeMap;
use std::sync::Arc;
use std::cell::LazyCell;

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

pub use generated::ENTRIES;

#[derive(Debug, Clone)]
pub struct Languages {
    by_part1: Arc<BTreeMap<&'static str, usize>>,
}

impl Languages {
    pub fn new() -> Self {
        let by_part1 = LazyCell::new(|| Arc::new(generated::PART1_MAP.iter().copied().collect::<BTreeMap<_, _>>()));
        Self { by_part1: (*by_part1).clone() }
    }

    pub fn get_by_part1(&self, part1: &str) -> Option<&'static Entry> {
        self.by_part1.get(part1).map(|index| &ENTRIES[*index])
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
