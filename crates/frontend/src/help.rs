//! The help sections shipped with the frontend, read from `help/NN-*.md`.
//!
//! A section is a header of `key: value` lines (`id`, `title`), a blank line,
//! and a markdown body; see [`crate::ui::markdown`] for what the body may use.

use std::sync::OnceLock;

/// Every section's source, in the order they are listed. A unit test checks
/// that each `help/*.md` file is named here.
const SOURCES: &[(&str, &str)] = &[
    ("10-finding.md", include_str!("../help/10-finding.md")),
    ("20-watching.md", include_str!("../help/20-watching.md")),
    ("30-remotes.md", include_str!("../help/30-remotes.md")),
    ("40-settings.md", include_str!("../help/40-settings.md")),
];

/// The section about finding and adding shows and movies.
pub(crate) const FINDING: &str = "finding";
/// The section about marking things watched.
pub(crate) const WATCHING: &str = "watching";
/// The section about remotes and syncing.
pub(crate) const REMOTES: &str = "remotes";

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Section {
    pub(crate) id: String,
    pub(crate) title: String,
    pub(crate) body: String,
    /// The lowercased prose a search matches against: the title and every run
    /// of text the body draws, without its markup.
    text: String,
}

impl Section {
    fn parse(source: &str) -> Option<Self> {
        let (header, body) = source.split_once("\n\n")?;

        let mut id = None;
        let mut title = None;

        for line in header.lines() {
            let (key, value) = line.split_once(':')?;

            match key.trim() {
                "id" => id = Some(value.trim()),
                "title" => title = Some(value.trim()),
                _ => {}
            }
        }

        let title = title?.to_owned();
        let mut text = title.to_lowercase();

        for run in crate::ui::markdown::runs(body) {
            text.push('\n');
            text.push_str(&run.to_lowercase());
        }

        Some(Self {
            id: id?.to_owned(),
            title,
            body: body.to_owned(),
            text,
        })
    }

    /// Whether the section mentions every word of `query`, ignoring case.
    pub(crate) fn matches(&self, query: &str) -> bool {
        query
            .split_whitespace()
            .all(|word| self.text.contains(&word.to_lowercase()))
    }
}

/// Every section, parsed once.
pub(crate) fn sections() -> &'static [Section] {
    static SECTIONS: OnceLock<Vec<Section>> = OnceLock::new();

    SECTIONS.get_or_init(|| {
        SOURCES
            .iter()
            .filter_map(|(_, source)| Section::parse(source))
            .collect()
    })
}

/// The section called `id`.
pub(crate) fn section(id: &str) -> Option<&'static Section> {
    sections().iter().find(|section| section.id == id)
}

/// The sections matching `query`, all of them when it is blank.
pub(crate) fn search(query: &str) -> Vec<&'static Section> {
    sections()
        .iter()
        .filter(|section| section.matches(query))
        .collect()
}

#[cfg(test)]
mod tests {
    use pulldown_cmark::{CodeBlockKind, Event, Tag, TagEnd};

    use super::{FINDING, REMOTES, SOURCES, WATCHING, search, section, sections};

    #[test]
    fn every_file_is_listed_and_parses() {
        let dir = concat!(env!("CARGO_MANIFEST_DIR"), "/help");

        let mut files = std::fs::read_dir(dir)
            .unwrap()
            .map(|entry| entry.unwrap().file_name().into_string().unwrap())
            .filter(|name| name.ends_with(".md"))
            .collect::<Vec<_>>();

        files.sort();

        let listed = SOURCES.iter().map(|(name, _)| *name).collect::<Vec<_>>();
        assert_eq!(files, listed, "help/*.md and SOURCES disagree");
        assert_eq!(sections().len(), SOURCES.len(), "a section did not parse");
    }

    #[test]
    fn every_id_is_unique_and_the_named_ones_exist() {
        let mut ids = sections().iter().map(|s| s.id.as_str()).collect::<Vec<_>>();
        ids.sort();
        ids.dedup();
        assert_eq!(ids.len(), sections().len(), "two sections share an id");

        for id in [FINDING, WATCHING, REMOTES, "settings"] {
            assert!(section(id).is_some(), "no section {id}");
        }
    }

    /// Every link points at a section, every icon is vendored and every demo
    /// is one the renderer draws.
    #[test]
    fn every_reference_resolves() {
        let icons = include_str!("../style/_icons_generated.scss");

        for section in sections() {
            let mut demo = None;

            for event in crate::ui::markdown::parser(&section.body) {
                match event {
                    Event::Start(Tag::Link { dest_url, .. }) => {
                        assert!(
                            super::section(&dest_url).is_some(),
                            "{} links to unknown section {dest_url}",
                            section.id
                        );
                    }
                    Event::Start(Tag::Image { dest_url, .. }) => {
                        let name = dest_url
                            .strip_prefix("icon:")
                            .or_else(|| dest_url.strip_prefix("button:"))
                            .unwrap_or_else(|| panic!("{}: bad image {dest_url}", section.id));

                        assert!(
                            icons.contains(&format!("'{name}'")),
                            "{} uses unknown icon {name}",
                            section.id
                        );
                    }
                    Event::Start(Tag::CodeBlock(CodeBlockKind::Fenced(lang)))
                        if lang.as_ref() == "track" =>
                    {
                        demo = Some(String::new());
                    }
                    Event::Text(text) => {
                        if let Some(demo) = &mut demo {
                            demo.push_str(&text);
                        }
                    }
                    Event::End(TagEnd::CodeBlock) => {
                        if let Some(name) = demo.take() {
                            assert!(
                                crate::ui::markdown::demo(name.trim()).is_some(),
                                "{} uses unknown demo {name}",
                                section.id
                            );
                        }
                    }
                    _ => {}
                }
            }
        }
    }

    #[test]
    fn search_filters_by_every_word_in_the_prose() {
        assert_eq!(search("").len(), sections().len());
        assert_eq!(search("   ").len(), sections().len());
        assert!(search("zzznotawordzzz").is_empty());

        let ids = |query| {
            search(query)
                .iter()
                .map(|s| s.id.as_str())
                .collect::<Vec<_>>()
        };
        assert_eq!(ids("tvmaze"), ids("TVmaze"), "search ignores case");
        assert!(ids("tvmaze").contains(&REMOTES));
        assert!(ids("theme").contains(&"settings"));

        let theme = ids("theme");
        assert!(
            ids("theme tvmaze").iter().all(|id| theme.contains(id)),
            "another word only narrows the result"
        );
    }

    #[test]
    fn a_header_is_needed() {
        assert!(super::Section::parse("id: a\ntitle: A\n\nbody").is_some());
        assert!(super::Section::parse("id: a\n\nbody").is_none());
        assert!(super::Section::parse("title: A\n\nbody").is_none());
        assert!(super::Section::parse("no header").is_none());
    }
}
