use std::collections::{BTreeMap, HashMap};

use musli_core::{Decode, Encode};

use crate::XemSystemEpisodes;

/// XEM's numbering systems a range can target, as XEM names them, with their
/// labels.
pub const XEM_SYSTEMS: &[(&str, &str)] = &[
    ("tvdb", "TheTVDB"),
    ("scene", "Scene"),
    ("anidb", "AniDB"),
    ("trakt", "Trakt"),
    ("rage", "TVRage"),
];

/// The label of an XEM system, or the name itself when it isn't known.
pub fn xem_system_label(system: &str) -> &str {
    XEM_SYSTEMS
        .iter()
        .find(|(name, _)| *name == system)
        .map_or(system, |(_, label)| label)
}

/// A show's manual link from its own episode codes to XEM numberings. A show
/// without one (automatic) is assumed to number its episodes like TheTVDB.
#[derive(
    Debug, Clone, Default, PartialEq, Eq, Encode, Decode, serde::Serialize, serde::Deserialize,
)]
#[musli(crate = musli_core)]
pub struct Numbering {
    pub ranges: Vec<NumberingRange>,
}

/// Episodes `first..=last` of `season` map one to one onto `system`'s
/// `target_season`, starting at `target_first`.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode, serde::Serialize, serde::Deserialize)]
#[musli(crate = musli_core)]
pub struct NumberingRange {
    pub season: u32,
    pub first: u32,
    pub last: u32,
    pub system: String,
    pub target_season: u32,
    pub target_first: u32,
}

impl NumberingRange {
    /// The target episode `last` maps onto.
    pub fn target_last(&self) -> u32 {
        self.target_first + self.last.saturating_sub(self.first)
    }

    fn contains(&self, season: u32, episode: u32) -> bool {
        self.season == season && (self.first..=self.last).contains(&episode)
    }
}

/// Where a show's episode is in another numbering.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NumberingTarget {
    pub system: String,
    pub season: u32,
    pub episode: u32,
}

/// A problem with the range at `index`.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
#[musli(crate = musli_core)]
pub struct RangeError {
    pub index: u32,
    pub message: String,
}

impl Numbering {
    /// The target of `season`/`episode`, or `None` when no range covers it.
    pub fn target(&self, season: u32, episode: u32) -> Option<NumberingTarget> {
        let r = self.ranges.iter().find(|r| r.contains(season, episode))?;

        Some(NumberingTarget {
            system: r.system.clone(),
            season: r.target_season,
            episode: r.target_first + (episode - r.first),
        })
    }

    /// Every problem with the ranges, by range; empty when they can be saved.
    pub fn validate(&self) -> Vec<RangeError> {
        let mut errors = Vec::new();

        for (index, r) in self.ranges.iter().enumerate() {
            let mut error = |message: String| {
                errors.push(RangeError {
                    index: index as u32,
                    message,
                });
            };

            if !XEM_SYSTEMS.iter().any(|(name, _)| *name == r.system) {
                error(format!("\"{}\" is not a known numbering.", r.system));
            }

            if r.first == 0 || r.last == 0 || r.target_first == 0 {
                error("Episode numbers start at 1.".to_owned());
                continue;
            }

            if r.first > r.last {
                error(format!(
                    "The range ends at E{} before it starts at E{}.",
                    r.last, r.first
                ));
                continue;
            }

            let earlier = &self.ranges[..index];

            if let Some(o) = earlier
                .iter()
                .find(|o| o.season == r.season && o.first <= r.last && r.first <= o.last)
            {
                error(format!(
                    "E{} is already in the range S{} E{}–E{}. TMDB ranges must not overlap.",
                    r.first.max(o.first),
                    o.season,
                    o.first,
                    o.last
                ));
            }

            if let Some(o) = earlier.iter().find(|o| {
                o.system == r.system
                    && o.target_season == r.target_season
                    && o.first <= o.last
                    && o.target_first <= r.target_last()
                    && r.target_first <= o.target_last()
            }) {
                error(format!(
                    "{} S{} E{} is already the target of S{} E{}–E{}. Target ranges must not overlap.",
                    xem_system_label(&r.system),
                    r.target_season,
                    r.target_first.max(o.target_first),
                    o.season,
                    o.first,
                    o.last
                ));
            }
        }

        errors
    }
}

/// Ranges pairing the show's regular `episodes` with a system's regular
/// `targets` one to one in episode order, starting a range wherever either
/// side breaks a season or skips a number. Both are `(season, episode)` codes;
/// specials (season 0) are left out.
pub fn suggest_numbering(
    episodes: &[(u32, u32)],
    system: &str,
    targets: &[(u32, u32)],
) -> Numbering {
    let mut episodes = regular(episodes);
    let mut targets = regular(targets);
    episodes.sort();
    targets.sort();

    let mut ranges = Vec::<NumberingRange>::new();

    for (&(season, episode), &(target_season, target)) in episodes.iter().zip(&targets) {
        if let Some(r) = ranges.last_mut()
            && r.season == season
            && r.last + 1 == episode
            && r.target_season == target_season
            && r.target_last() + 1 == target
        {
            r.last = episode;
            continue;
        }

        ranges.push(NumberingRange {
            season,
            first: episode,
            last: episode,
            system: system.to_owned(),
            target_season,
            target_first: target,
        });
    }

    Numbering { ranges }
}

/// XEM's episodes of every system, linked through the map entries they share.
#[derive(Debug, Default, Clone, PartialEq)]
pub struct XemLinks {
    systems: Vec<String>,
    entries: HashMap<(usize, u32, u32), u32>,
    codes: HashMap<(u32, usize), (u32, u32)>,
}

impl XemLinks {
    pub fn new(systems: &[XemSystemEpisodes]) -> Self {
        let mut out = Self::default();

        for (index, s) in systems.iter().enumerate() {
            out.systems.push(s.system.clone());

            for (&(season, episode), &entry) in s.episodes.iter().zip(&s.entries) {
                out.entries.insert((index, season, episode), entry);
                out.codes.entry((entry, index)).or_insert((season, episode));
            }
        }

        out
    }

    fn index(&self, system: &str) -> Option<usize> {
        self.systems.iter().position(|s| s == system)
    }

    /// The map entry `system` numbers `season`/`episode` in.
    pub fn entry(&self, system: &str, season: u32, episode: u32) -> Option<u32> {
        let index = self.index(system)?;
        self.entries.get(&(index, season, episode)).copied()
    }

    /// The code of `system`'s episode `season`/`episode` in the `to` system.
    pub fn translate(
        &self,
        system: &str,
        season: u32,
        episode: u32,
        to: &str,
    ) -> Option<(u32, u32)> {
        if system == to {
            return Some((season, episode));
        }

        let entry = self.entry(system, season, episode)?;
        self.codes.get(&(entry, self.index(to)?)).copied()
    }

    /// The map entries the show's regular `episodes` reach when suggested
    /// from `system`'s episode order, as [`suggest_numbering`] pairs them.
    fn suggested_entries(&self, episodes: &[(u32, u32)], system: &XemSystemEpisodes) -> Vec<u32> {
        let mut targets = system
            .episodes
            .iter()
            .zip(&system.entries)
            .filter(|((s, _), _)| *s > 0)
            .collect::<Vec<_>>();
        targets.sort();

        targets
            .into_iter()
            .take(regular(episodes).len())
            .map(|(_, &entry)| entry)
            .collect()
    }

    /// The systems a suggestion could pair the show's `episodes` with,
    /// grouped by the numbering they would give: systems in one group reach
    /// the same map entries in the same order. Groups and their systems keep
    /// the order of `systems`.
    pub fn suggestion_bases(
        &self,
        episodes: &[(u32, u32)],
        systems: &[XemSystemEpisodes],
    ) -> Vec<Vec<String>> {
        let mut groups = Vec::<(Vec<u32>, Vec<String>)>::new();

        for system in systems {
            if regular(&system.episodes).is_empty() {
                continue;
            }

            let entries = self.suggested_entries(episodes, system);

            match groups.iter_mut().find(|(e, _)| *e == entries) {
                Some((_, names)) => names.push(system.system.clone()),
                None => groups.push((entries, vec![system.system.clone()])),
            }
        }

        groups.into_iter().map(|(_, names)| names).collect()
    }
}

/// A regular season whose episode count differs between the show and
/// TheTVDB's numbering in XEM.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SeasonMismatch {
    pub season: u32,
    pub episodes: u32,
    pub tvdb: u32,
}

/// The show's regular seasons whose episode count differs from TheTVDB's in
/// XEM, where automatic numbering goes wrong. Empty when XEM has no TheTVDB
/// numbering at all.
pub fn numbering_mismatches(episodes: &[(u32, u32)], tvdb: &[(u32, u32)]) -> Vec<SeasonMismatch> {
    let tvdb = counts(&regular(tvdb));

    if tvdb.is_empty() {
        return Vec::new();
    }

    counts(&regular(episodes))
        .into_iter()
        .filter_map(|(season, n)| {
            let t = tvdb.get(&season).copied().unwrap_or(0);
            (t != n).then_some(SeasonMismatch {
                season,
                episodes: n,
                tvdb: t,
            })
        })
        .collect()
}

fn regular(codes: &[(u32, u32)]) -> Vec<(u32, u32)> {
    codes.iter().copied().filter(|&(s, _)| s > 0).collect()
}

fn counts(codes: &[(u32, u32)]) -> BTreeMap<u32, u32> {
    let mut out = BTreeMap::new();

    for &(season, _) in codes {
        *out.entry(season).or_default() += 1;
    }

    out
}

/// Whether an XEM system's numbering is shown on episodes, in the order of
/// the "Other numberings" setting.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode, serde::Serialize, serde::Deserialize)]
#[musli(crate = musli_core)]
pub struct NumberingPref {
    pub system: String,
    pub shown: bool,
}

/// The systems shown by default; the rest of [`XEM_SYSTEMS`] start hidden.
const SHOWN_NUMBERINGS: &[&str] = &["tvdb", "scene", "anidb"];

/// The default "Other numberings" setting.
pub fn default_numberings() -> Vec<NumberingPref> {
    numbering_order(&[])
}

/// `prefs` with unknown systems left out and every known system missing from
/// it appended with its default, so each system has one position.
pub fn numbering_order(prefs: &[NumberingPref]) -> Vec<NumberingPref> {
    let mut out = Vec::<NumberingPref>::new();

    for p in prefs {
        if XEM_SYSTEMS.iter().any(|(name, _)| *name == p.system)
            && !out.iter().any(|o| o.system == p.system)
        {
            out.push(p.clone());
        }
    }

    for (name, _) in XEM_SYSTEMS {
        if !out.iter().any(|o| o.system == *name) {
            out.push(NumberingPref {
                system: (*name).to_owned(),
                shown: SHOWN_NUMBERINGS.contains(name),
            });
        }
    }

    out
}

/// An episode's code in another numbering. `last` is the second episode of a
/// double episode, which XEM gives as a second address in the same season.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
#[musli(crate = musli_core)]
pub struct AltNumbering {
    pub system: String,
    pub season: u32,
    pub episode: u32,
    pub last: Option<u32>,
    pub absolute: Option<u32>,
}

impl AltNumbering {
    /// The code as shown, such as `S02E01` or `S01E03+04`.
    pub fn code(&self) -> String {
        let mut code = format!("S{:02}E{:02}", self.season, self.episode);

        if let Some(last) = self.last {
            code.push_str(&format!("+{last:02}"));
        }

        code
    }
}

/// The season of another numbering an episode is linked to.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
#[musli(crate = musli_core)]
pub struct LinkedSeason {
    pub system: String,
    pub season: u32,
}

/// An alternative name from XEM, with XEM's language code (`us`, `jp`, ...)
/// when it has one.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
#[musli(crate = musli_core)]
pub struct AltName {
    pub name: String,
    pub language: Option<String>,
}

/// The XEM names of a season of another numbering that a season covers.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
#[musli(crate = musli_core)]
pub struct SeasonAltNames {
    pub target: LinkedSeason,
    pub names: Vec<AltName>,
}
