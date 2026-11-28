// Renders the in-app help (`crates/frontend/help/*.md`) into the site's
// manual pages. Shared by build.rs and tests/manual.rs.
//
// A help file is a header of `key: value` lines (`id`, `title`), a blank
// line and a markdown body, parsed as `crates/frontend/src/help.rs` does.
// The body's own syntax is rendered as `crates/frontend/src/ui/markdown.rs`
// draws it in the app: `[text](section-id)` links to another page,
// `![label](icon:name)` is an icon, `![label](button:name)` a button and a
// fenced `track` block one of the app's illustrations. Raw HTML stays text.

use std::collections::{BTreeSet, HashMap};

use pulldown_cmark::{CodeBlockKind, CowStr, Event, Options, Parser, Tag, TagEnd};

/// Where the help files are, from the repository root.
pub const HELP: &str = "crates/frontend/help/";

/// Where the help directory is copied into the site (index.html's
/// `copy-dir`), so pictures beside a page load from there.
pub const IMAGES: &str = "help/";

/// The fence language of an illustration.
const DEMO_LANG: &str = "track";

pub struct Section {
    pub id: String,
    pub title: String,
    pub body: String,
}

/// A help file, as `help.rs` reads it.
pub fn parse(source: &str) -> Option<Section> {
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

    Some(Section {
        id: id?.to_owned(),
        title: title?.to_owned(),
        body: body.to_owned(),
    })
}

/// A rendered page.
pub struct Page {
    pub html: String,
    /// Each heading's anchor and text, in order.
    pub headings: Vec<(String, String)>,
}

/// The site address of a link in a help body, given every section id.
pub fn link(dest: &str, sections: &BTreeSet<String>) -> Result<String, String> {
    if dest.contains("://") || dest.starts_with("mailto:") {
        return Ok(dest.to_owned());
    }

    let (id, fragment) = match dest.split_once('#') {
        Some((id, fragment)) => (id, format!("#{fragment}")),
        None => (dest, String::new()),
    };

    if sections.contains(id) {
        return Ok(format!("#/manual/{id}{fragment}"));
    }

    Err(format!("link to unknown section `{dest}`"))
}

/// The address of a picture stored beside the help files.
pub fn image(dest: &str) -> String {
    if dest.contains("://") || dest.starts_with('/') {
        return dest.to_owned();
    }

    format!("{IMAGES}{}", dest.trim_start_matches("./"))
}

/// GitHub's anchor for a heading's text.
pub fn slug(text: &str) -> String {
    text.chars()
        .filter_map(|c| match c {
            ' ' => Some('-'),
            c if c.is_alphanumeric() || c == '-' || c == '_' => Some(c),
            _ => None,
        })
        .flat_map(char::to_lowercase)
        .collect()
}

pub fn escape(text: &str) -> String {
    let mut out = String::with_capacity(text.len());

    for c in text.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&#39;"),
            c => out.push(c),
        }
    }

    out
}

/// Draws the app's icons and buttons as the site's own markup. `icon` gives
/// an icon's SVG by name.
pub struct Samples<'a> {
    pub icon: &'a dyn Fn(&str) -> Option<String>,
}

impl Samples<'_> {
    fn svg(&self, name: &str) -> Result<String, String> {
        (self.icon)(name).ok_or_else(|| format!("unknown icon `{name}`"))
    }

    /// An icon inline in the text, `label` for screen readers.
    pub fn glyph(&self, name: &str, label: &str) -> Result<String, String> {
        Ok(format!(
            "<span class=\"site-glyph\" role=\"img\" aria-label=\"{}\">{}</span>",
            escape(label),
            self.svg(name)?.trim()
        ))
    }

    /// A button as it looks in the app: its icon, then `label` when there is
    /// one. `modifier` is one of the key's variant classes.
    pub fn key(&self, name: &str, label: &str, modifier: Option<&str>) -> Result<String, String> {
        let class = match modifier {
            Some(modifier) => format!("site-key {modifier}"),
            None => String::from("site-key"),
        };

        let text = match label {
            "" => String::new(),
            label => format!("<span>{}</span>", escape(label)),
        };

        Ok(format!(
            "<span class=\"{class}\">{}{text}</span>",
            self.svg(name)?.trim()
        ))
    }

    /// One of the app's illustrations (`markdown.rs`'s `demo`), drawn from
    /// keys and hidden from screen readers like the app's inert ones.
    pub fn demo(&self, name: &str) -> Result<Option<String>, String> {
        let keys = match name {
            "tracked" => [
                self.key("eye", "Tracking", Some("site-key-on"))?,
                self.key("eye-slash", "Not tracking", None)?,
            ]
            .join(""),
            "media-kind" => [
                self.key("tv", "Shows", Some("site-key-on"))?,
                self.key("film", "Movies", None)?,
            ]
            .join(""),
            "mark-watched" => format!(
                "<span class=\"site-split\">{}{}</span>",
                self.key("check", "Mark watched", Some("site-key-primary"))?,
                self.key("chevron-down", "", Some("site-key-primary"))?,
            ),
            _ => return Ok(None),
        };

        Ok(Some(format!(
            "<div class=\"site-demo\" aria-hidden=\"true\">{keys}</div>"
        )))
    }
}

/// A help body as HTML.
pub fn render(
    body: &str,
    sections: &BTreeSet<String>,
    samples: &Samples<'_>,
) -> Result<Page, String> {
    let events: Vec<Event> = Parser::new_ext(body, Options::ENABLE_TABLES).collect();

    let mut out = Vec::with_capacity(events.len());
    let mut headings = Vec::new();
    let mut seen = HashMap::<String, usize>::new();
    let mut iter = events.into_iter();

    while let Some(event) = iter.next() {
        match event {
            Event::Start(Tag::Heading {
                level,
                classes,
                attrs,
                ..
            }) => {
                let mut inner = Vec::new();
                let mut text = String::new();

                for event in iter.by_ref() {
                    if let Event::End(TagEnd::Heading(..)) = event {
                        break;
                    }

                    if let Event::Text(t) | Event::Code(t) = &event {
                        text.push_str(t);
                    }

                    inner.push(event);
                }

                let base = slug(&text);
                let count = seen.entry(base.clone()).or_default();

                let id = match *count {
                    0 => base,
                    n => format!("{base}-{n}"),
                };

                *count += 1;
                headings.push((id.clone(), text));

                out.push(Event::Start(Tag::Heading {
                    level,
                    id: Some(CowStr::from(id)),
                    classes,
                    attrs,
                }));
                out.extend(inner);
                out.push(Event::End(TagEnd::Heading(level)));
            }
            Event::Start(Tag::Link {
                link_type,
                dest_url,
                title,
                id,
            }) => {
                out.push(Event::Start(Tag::Link {
                    link_type,
                    dest_url: CowStr::from(link(&dest_url, sections)?),
                    title,
                    id,
                }));
            }
            Event::Start(Tag::Image {
                link_type,
                dest_url,
                title,
                id,
            }) => {
                let sample = dest_url
                    .strip_prefix("icon:")
                    .map(|name| (true, name))
                    .or_else(|| dest_url.strip_prefix("button:").map(|name| (false, name)));

                let Some((inline, name)) = sample else {
                    out.push(Event::Start(Tag::Image {
                        link_type,
                        dest_url: CowStr::from(image(&dest_url)),
                        title,
                        id,
                    }));
                    continue;
                };

                let mut alt = String::new();

                for event in iter.by_ref() {
                    match event {
                        Event::End(TagEnd::Image) => break,
                        Event::Text(t) | Event::Code(t) => alt.push_str(&t),
                        _ => {}
                    }
                }

                let html = match inline {
                    true => samples.glyph(name, &alt)?,
                    false => samples.key(name, &alt, None)?,
                };

                out.push(Event::InlineHtml(CowStr::from(html)));
            }
            Event::Start(Tag::CodeBlock(CodeBlockKind::Fenced(lang)))
                if lang.as_ref() == DEMO_LANG =>
            {
                let mut name = String::new();

                for event in iter.by_ref() {
                    match event {
                        Event::End(TagEnd::CodeBlock) => break,
                        Event::Text(t) => name.push_str(&t),
                        _ => {}
                    }
                }

                let name = name.trim();

                match samples.demo(name)? {
                    Some(html) => out.push(Event::Html(CowStr::from(html))),
                    None => return Err(format!("unknown illustration `{name}`")),
                }
            }
            Event::Html(html) | Event::InlineHtml(html) => out.push(Event::Text(html)),
            event => out.push(event),
        }
    }

    let mut html = String::new();
    pulldown_cmark::html::push_html(&mut html, out.into_iter());
    Ok(Page { html, headings })
}
