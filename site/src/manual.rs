use yew::prelude::*;

use sockel::{Card, CardGrid, Gap, GridSize, NavItem, NavList, PageHead, SectionHead, Stack};

use crate::{GITHUB, class};

pub struct Page {
    /// The section's id, its route under `#/manual/`.
    pub id: &'static str,
    /// The file under `crates/frontend/help/`.
    pub file: &'static str,
    pub title: &'static str,
    pub html: &'static str,
    /// Each heading's anchor and text.
    pub headings: &'static [(&'static str, &'static str)],
}

include!(concat!(env!("OUT_DIR"), "/manual.rs"));

pub fn find(id: &str) -> Option<&'static Page> {
    PAGES.iter().find(|page| page.id == id)
}

fn href(id: &str) -> String {
    format!("#/manual/{id}")
}

#[derive(Properties, PartialEq)]
pub struct ManualProps {
    pub route: AttrValue,
}

#[function_component]
pub fn Manual(props: &ManualProps) -> Html {
    let article = if props.route.is_empty() {
        contents()
    } else {
        match find(&props.route) {
            Some(page) => html! {
                <>
                    <PageHead title={html! { page.title }} />
                    <div class={class::MARKDOWN}>
                        { Html::from_html_unchecked(AttrValue::Static(page.html)) }
                    </div>
                    <p class={class::MUTED}>
                        { "This page is also the app's built-in help. " }
                        <a href={format!("{GITHUB}/blob/main/crates/frontend/help/{}", page.file)}>
                            { "View it on GitHub" }
                        </a>
                    </p>
                </>
            },
            None => html! {
                <>
                    <PageHead title={html! { "Page not found" }} />
                    <p>
                        { "There is no such page in the manual. Start from " }
                        <a href={href("")}>{ "its contents" }</a>
                        { "." }
                    </p>
                </>
            },
        }
    };

    let mut items = vec![NavItem::new("", "Contents").href(href(""))];
    items.extend(
        PAGES
            .iter()
            .map(|page| NavItem::new(page.id, page.title).href(href(page.id))),
    );

    let current = find(&props.route).filter(|page| !page.headings.is_empty());

    html! {
        <div class={class::MANUAL}>
            <aside class={class::MANUAL_NAV}>
                <Stack gap={Gap::Snug}>
                    <SectionHead title={html! { "The manual" }} class={sockel::class::LABEL} />
                    <NavList label="The manual" value={props.route.clone()} {items} />
                    if let Some(page) = current {
                        <Stack gap={Gap::Snug} class={class::NAV_BRANCH}>
                            <NavList label={page.title} items={page.headings.iter().map(|(anchor, text)| {
                                NavItem::new(*anchor, *text).href(format!("{}#{anchor}", href(page.id)))
                            }).collect::<Vec<_>>()} />
                        </Stack>
                    }
                </Stack>
            </aside>
            <main class={class::MANUAL_PAGE}>
                <Stack gap={Gap::Loose}>{ article }</Stack>
            </main>
        </div>
    }
}

/// The manual's front page: every page, with the headings on it.
fn contents() -> Html {
    html! {
        <>
            <PageHead title={html! { "The track manual" }} />
            <p class={class::LEAD}>
                { "The same pages track shows in its help panel: press the question mark in \
                   the app's toolbar to search them while you use it." }
            </p>
            <CardGrid size={GridSize::Wide}>
                { for PAGES.iter().map(|page| html! {
                    <Card tag="li" gap={Gap::Snug}>
                        <a href={href(page.id)} class={class::CARD_TITLE}>{ page.title }</a>
                        if !page.headings.is_empty() {
                            <p class={class::MUTED}>
                                { page.headings.iter().map(|(_, text)| *text).collect::<Vec<_>>().join(" · ") }
                            </p>
                        }
                    </Card>
                }) }
            </CardGrid>
        </>
    }
}
