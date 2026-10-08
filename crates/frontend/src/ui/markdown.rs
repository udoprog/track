//! Help markdown, rendered into the virtual DOM so raw HTML in a source stays
//! text.
//!
//! Beyond plain markdown a help body may use:
//! - `[text](section-id)`: a link to another help section;
//! - `![label](icon:name)`: a track icon inline, `label` for screen readers;
//! - `![label](button:name)`: a track button with that icon and label;
//! - a fenced block in the `track` language naming one of [`DEMOS`]: a real
//!   component drawn as an illustration, see [`demo`].
//!
//! Illustrations are `inert`, so they cannot be focused or pressed.

use std::rc::Rc;

use pulldown_cmark::{CodeBlockKind, Event, HeadingLevel, Options, Parser, Tag, TagEnd};
use yew::html::onclick;
use yew::prelude::*;
use yew::virtual_dom::{VList, VNode, VTag, VText};

use crate::router::MediaSelection;
use crate::ui::{Button, MarkTimeMenu, MediaKindToggle, Tracked};

/// The fence language a demo is written in.
const DEMO_LANG: &str = "track";

/// Draw the demo called `name`.
pub(crate) fn demo(name: &str) -> Option<Html> {
    let html = match name {
        "tracked" => html! {
            <>
                <Tracked tracked={true} kind="show" ontoggle={Callback::noop()} />
                <Tracked tracked={false} kind="show" ontoggle={Callback::noop()} />
            </>
        },
        "media-kind" => html! {
            <span class="chips">
                <MediaKindToggle selection={MediaSelection { shows: true, movies: false }} on_change={Callback::noop()} />
            </span>
        },
        "mark-watched" => html! {
            <MarkTimeMenu quick={true} class="primary" icon="check" title="Mark watched" prompt="When did you watch this episode?" on_confirm={Callback::noop()} />
        },
        _ => return None,
    };

    Some(html)
}

/// What a section link resolves to: a callback opening that section, or
/// `None` for a name that is not a section.
pub(crate) type Go<'a> = &'a dyn Fn(&str) -> Option<Callback<MouseEvent>>;

/// What the reader searched for, and where the first hit goes.
pub(crate) struct Highlight<'a> {
    /// Lowercased words to mark.
    pub(crate) words: &'a [String],
    /// Given to the first mark, so it can be scrolled to.
    pub(crate) first: &'a NodeRef,
}

enum Frame {
    Tag(VTag),
    Image { dest: String, alt: String },
    Demo { name: String },
}

struct Builder<'a> {
    root: VList,
    stack: Vec<Frame>,
    heading_row: bool,
    go: Go<'a>,
    highlight: Option<&'a Highlight<'a>>,
    marked: bool,
}

impl Builder<'_> {
    fn push(&mut self, node: VNode) {
        match self.stack.last_mut() {
            Some(Frame::Tag(tag)) => tag.add_child(node),
            Some(Frame::Image { .. } | Frame::Demo { .. }) => {}
            None => self.root.push(node),
        }
    }

    fn text(&mut self, text: &str) {
        match self.stack.last_mut() {
            Some(Frame::Image { alt, .. }) => {
                alt.push_str(text);
                return;
            }
            Some(Frame::Demo { name }) => {
                name.push_str(text);
                return;
            }
            _ => {}
        }

        let Some(highlight) = self.highlight.filter(|h| !h.words.is_empty()) else {
            self.push(VNode::VText(VText::new(text.to_owned())));
            return;
        };

        let haystack = text.to_lowercase();

        // Lowercasing can change byte lengths; mark nothing rather than
        // slicing the original at the wrong place.
        if haystack.len() != text.len() {
            self.push(VNode::VText(VText::new(text.to_owned())));
            return;
        }

        let mut rest = 0;

        while let Some((start, end)) = next_hit(&haystack, rest, highlight.words) {
            if start > rest {
                self.push(VNode::VText(VText::new(text[rest..start].to_owned())));
            }

            let mut mark = VTag::new("mark");
            mark.add_attribute("class", "help-hit");
            mark.add_child(VNode::VText(VText::new(text[start..end].to_owned())));

            if !self.marked {
                self.marked = true;
                mark.node_ref = highlight.first.clone();
            }

            self.push(VNode::VTag(Rc::new(mark)));
            rest = end;
        }

        if rest < text.len() {
            self.push(VNode::VText(VText::new(text[rest..].to_owned())));
        }
    }

    fn open(&mut self, tag: &'static str) -> &mut VTag {
        self.stack.push(Frame::Tag(VTag::new(tag)));

        let Some(Frame::Tag(tag)) = self.stack.last_mut() else {
            unreachable!("just pushed a tag")
        };

        tag
    }

    fn close(&mut self) {
        let Some(frame) = self.stack.pop() else {
            return;
        };

        match frame {
            Frame::Tag(tag) => self.push(VNode::VTag(Rc::new(tag))),
            Frame::Image { dest, alt } => {
                if let Some(name) = dest.strip_prefix("icon:") {
                    let mut icon = VTag::new("span");
                    icon.add_attribute("class", format!("icon {name} help-icon"));
                    icon.add_attribute("role", "img");
                    icon.add_attribute("aria-label", alt);
                    self.push(VNode::VTag(Rc::new(icon)));
                } else if let Some(name) = dest.strip_prefix("button:") {
                    let hit = self.highlight.filter(|h| {
                        let alt = alt.to_lowercase();
                        h.words.iter().any(|word| alt.contains(word.as_str()))
                    });

                    let alt = AttrValue::from(alt);
                    let button = html! {
                        <Button icon={AttrValue::from(name.to_owned())} title={alt.clone()} label={alt} />
                    };

                    let mut node = sample("span", button);

                    // The label is inside the component, so the whole sample
                    // is marked instead.
                    if let (Some(hit), VNode::VTag(tag)) = (hit, &mut node) {
                        let tag = Rc::make_mut(tag);
                        tag.add_attribute("class", "help-sample help-hit");

                        if !self.marked {
                            self.marked = true;
                            tag.node_ref = hit.first.clone();
                        }
                    }

                    self.push(node);
                }
            }
            Frame::Demo { name } => {
                let name = name.trim();

                match demo(name) {
                    Some(html) => {
                        let mut node = sample("div", html);

                        if let VNode::VTag(tag) = &mut node {
                            Rc::make_mut(tag).add_attribute("data-demo", name.to_owned());
                        }

                        self.push(node);
                    }
                    None => {
                        let mut code = VTag::new("code");
                        code.add_child(VNode::VText(VText::new(name.to_owned())));
                        let mut pre = VTag::new("pre");
                        pre.add_child(VNode::VTag(Rc::new(code)));
                        self.push(VNode::VTag(Rc::new(pre)));
                    }
                }
            }
        }
    }
}

/// An illustration: real markup that cannot be focused or pressed.
fn sample(tag: &'static str, child: Html) -> VNode {
    let mut wrapper = VTag::new(tag);
    wrapper.add_attribute("class", "help-sample");
    wrapper.add_attribute("inert", "");
    wrapper.add_child(child);
    VNode::VTag(Rc::new(wrapper))
}

/// The earliest occurrence at or after `from` of any of `words`.
fn next_hit(haystack: &str, from: usize, words: &[String]) -> Option<(usize, usize)> {
    words
        .iter()
        .filter(|word| !word.is_empty())
        .filter_map(|word| {
            let start = from + haystack[from..].find(word.as_str())?;
            Some((start, start + word.len()))
        })
        .min_by_key(|&(start, end)| (start, usize::MAX - end))
}

fn heading(level: HeadingLevel) -> &'static str {
    match level {
        HeadingLevel::H1 | HeadingLevel::H2 => "h4",
        _ => "h5",
    }
}

pub(crate) fn parser(source: &str) -> Parser<'_> {
    Parser::new_ext(source, Options::ENABLE_TABLES)
}

/// Every run of text `source` draws, in order.
pub(crate) fn runs(source: &str) -> Vec<String> {
    parser(source)
        .filter_map(|event| match event {
            Event::Text(text) | Event::Code(text) => Some(text.into_string()),
            _ => None,
        })
        .collect()
}

pub(crate) fn render(source: &str, go: Go<'_>, highlight: Option<&Highlight<'_>>) -> Html {
    let mut b = Builder {
        root: VList::default(),
        stack: Vec::new(),
        heading_row: false,
        go,
        highlight,
        marked: false,
    };

    for event in parser(source) {
        match event {
            Event::Start(tag) => match tag {
                Tag::Paragraph => {
                    b.open("p");
                }
                Tag::Heading { level, .. } => {
                    b.open(heading(level));
                }
                Tag::BlockQuote(_) => {
                    b.open("blockquote");
                }
                Tag::CodeBlock(kind) => {
                    if matches!(&kind, CodeBlockKind::Fenced(lang) if lang.as_ref() == DEMO_LANG) {
                        b.stack.push(Frame::Demo {
                            name: String::new(),
                        });
                        continue;
                    }

                    b.open("pre");
                    b.open("code");
                }
                Tag::List(Some(start)) => {
                    let list = b.open("ol");

                    if start != 1 {
                        list.add_attribute("start", start.to_string());
                    }
                }
                Tag::List(None) => {
                    b.open("ul");
                }
                Tag::Item => {
                    b.open("li");
                }
                Tag::Emphasis => {
                    b.open("em");
                }
                Tag::Strong => {
                    b.open("strong");
                }
                Tag::Link { dest_url, .. } => {
                    let go = (b.go)(&dest_url);
                    let link = b.open("a");

                    match go {
                        Some(onclick) => {
                            link.add_attribute("class", "help-link");
                            link.add_attribute("href", format!("#{dest_url}"));
                            link.add_attribute("data-section", dest_url.into_string());
                            link.add_listener(Rc::new(onclick::Wrapper::new(onclick)));
                        }
                        None => {
                            link.add_attribute("href", dest_url.into_string());
                            link.add_attribute("target", "_blank");
                            link.add_attribute("rel", "noopener noreferrer");
                        }
                    }
                }
                Tag::Image { dest_url, .. } => {
                    b.stack.push(Frame::Image {
                        dest: dest_url.into_string(),
                        alt: String::new(),
                    });
                }
                Tag::Table(_) => {
                    b.open("table");
                }
                Tag::TableHead => {
                    b.open("thead");
                    b.open("tr");
                    b.heading_row = true;
                }
                Tag::TableRow => {
                    b.open("tr");
                }
                Tag::TableCell => {
                    b.open(if b.heading_row { "th" } else { "td" });
                }
                _ => {}
            },
            Event::End(end) => match end {
                TagEnd::TableHead => {
                    b.heading_row = false;
                    b.close();
                    b.close();
                }
                TagEnd::CodeBlock => {
                    if matches!(b.stack.last(), Some(Frame::Demo { .. })) {
                        b.close();
                        continue;
                    }

                    b.close();
                    b.close();
                }
                TagEnd::Paragraph
                | TagEnd::Heading(_)
                | TagEnd::BlockQuote(_)
                | TagEnd::List(_)
                | TagEnd::Item
                | TagEnd::Emphasis
                | TagEnd::Strong
                | TagEnd::Link
                | TagEnd::Image
                | TagEnd::Table
                | TagEnd::TableRow
                | TagEnd::TableCell => b.close(),
                _ => {}
            },
            Event::Text(text) => b.text(&text),
            Event::Code(text) => {
                b.open("code");
                b.text(&text);
                b.close();
            }
            Event::SoftBreak => b.text(" "),
            Event::HardBreak => {
                b.open("br");
                b.close();
            }
            Event::Rule => {
                b.open("hr");
                b.close();
            }
            _ => {}
        }
    }

    while !b.stack.is_empty() {
        b.close();
    }

    VNode::VList(Rc::new(b.root))
}

#[cfg(test)]
mod tests {
    use yew::prelude::*;
    use yew::virtual_dom::VNode;

    use super::{Highlight, next_hit, render};

    fn tags(node: &VNode, out: &mut Vec<String>) {
        match node {
            VNode::VTag(tag) => {
                out.push(tag.tag().to_owned());

                if let Some(children) = tag.children() {
                    tags(children, out);
                }
            }
            VNode::VList(list) => {
                for child in list.iter() {
                    tags(child, out);
                }
            }
            _ => {}
        }
    }

    fn rendered(source: &str, highlight: Option<&Highlight<'_>>) -> Vec<String> {
        let mut out = Vec::new();
        tags(&render(source, &|_| None, highlight), &mut out);
        out
    }

    #[test]
    fn markdown_becomes_elements_and_html_stays_text() {
        assert_eq!(
            rendered("# Head\n\nA *word* and **more**.\n\n- one\n", None),
            ["h4", "p", "em", "strong", "ul", "li"]
        );

        let html = rendered("<script>x</script>\n\nplain <b>bold</b>\n", None);
        assert!(!html.iter().any(|t| t == "script" || t == "b"), "{html:?}");
    }

    #[test]
    fn icons_and_buttons_are_samples() {
        assert_eq!(rendered("See ![Watched](icon:check).", None), ["p", "span"]);
        // The button is a component, which this walk does not enter.
        assert_eq!(rendered("![Add](button:plus)", None), ["p", "span"]);
        assert_eq!(rendered("```track\nnope\n```\n", None), ["pre", "code"]);
    }

    #[test]
    fn search_words_are_marked() {
        let words = ["sync".to_owned()];
        let first = NodeRef::default();
        let highlight = Highlight {
            words: &words,
            first: &first,
        };

        assert_eq!(
            rendered("Sync and resync.", Some(&highlight)),
            ["p", "mark", "mark"]
        );

        let words = ["ab".to_owned(), "abc".to_owned()];
        assert_eq!(next_hit("xabcx", 0, &words), Some((1, 4)));
        assert_eq!(next_hit("xabcx", 4, &words), None);

        let words = ["undo".to_owned()];
        let highlight = Highlight {
            words: &words,
            first: &first,
        };

        let node = render(
            "![Undo](button:arrow-uturn-left)",
            &|_| None,
            Some(&highlight),
        );
        let VNode::VList(list) = &node else {
            panic!("a list")
        };
        let Some(VNode::VTag(p)) = list.iter().next() else {
            panic!("a paragraph")
        };
        let Some(VNode::VList(children)) = p.children() else {
            panic!("children")
        };
        let Some(VNode::VTag(sample)) = children.iter().next() else {
            panic!("a sample")
        };
        assert_eq!(
            sample
                .attributes
                .iter()
                .find(|(k, _)| *k == "class")
                .map(|(_, v)| v),
            Some("help-sample help-hit")
        );
    }
}
