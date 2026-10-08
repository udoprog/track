use web_sys::{Element, HtmlInputElement, ScrollIntoViewOptions, ScrollLogicalPosition};
use yew::prelude::*;

use crate::help;
use crate::ui::markdown::{self, Highlight};
use crate::ui::{Button, Modal};

#[derive(Properties, PartialEq)]
pub(crate) struct Props {
    /// The section to open at, or the first.
    #[prop_or_default]
    pub(crate) section: Option<AttrValue>,
    pub(crate) on_close: Callback<()>,
}

/// The help sections beside a list of them, which a search narrows.
#[function_component]
pub(crate) fn HelpModal(props: &Props) -> Html {
    let current = use_state(|| props.section.clone());
    let query = use_state(String::new);
    let body = use_node_ref();
    let first_hit = use_node_ref();

    let found = help::search(&query);

    // The chosen section while the search keeps it, else the first match.
    let shown = current
        .as_deref()
        .and_then(|id| found.iter().find(|s| s.id == id))
        .or(found.first())
        .copied();

    let words = query
        .split_whitespace()
        .map(str::to_lowercase)
        .collect::<Vec<_>>();

    {
        let body = body.clone();
        let first_hit = first_hit.clone();
        let id = shown.map(|s| s.id.clone());

        use_effect_with((id, words.clone()), move |_| {
            if let Some(hit) = first_hit.cast::<Element>() {
                let options = ScrollIntoViewOptions::new();
                options.set_block(ScrollLogicalPosition::Nearest);
                hit.scroll_into_view_with_scroll_into_view_options(&options);
            } else if let Some(body) = body.cast::<Element>() {
                body.set_scroll_top(0);
            }
        });
    }

    let oninput = {
        let query = query.clone();

        Callback::from(move |e: InputEvent| {
            if let Some(input) = e.target_dyn_into::<HtmlInputElement>() {
                query.set(input.value());
            }
        })
    };

    let select = |id: &str| {
        let current = current.clone();
        let id = AttrValue::from(id.to_owned());
        Callback::from(move |_: MouseEvent| current.set(Some(id.clone())))
    };

    let go = |id: &str| -> Option<Callback<MouseEvent>> {
        help::section(id)?;

        let current = current.clone();
        let query = query.clone();
        let id = AttrValue::from(id.to_owned());

        Some(Callback::from(move |e: MouseEvent| {
            e.prevent_default();
            query.set(String::new());
            current.set(Some(id.clone()));
        }))
    };

    let highlight = Highlight {
        words: &words,
        first: &first_hit,
    };

    html! {
        <Modal icon="question-mark-circle" title={html! { "Help" }} class="help-modal" on_close={props.on_close.clone()}>
            <div class="help" data-test="help">
                <nav class="help-nav" aria-label="Help sections">
                    <input
                        type="search"
                        class="input-text"
                        placeholder="Search help…"
                        aria-label="Search help"
                        data-test="help-search"
                        value={(*query).clone()}
                        {oninput}
                    />

                    <div class="help-sections">
                        { for found.iter().map(|section| {
                            let selected = shown.is_some_and(|s| s.id == section.id);

                            html! {
                                <Button
                                    class={classes!("help-section", selected.then_some("selected"))}
                                    label={section.title.clone()}
                                    title={section.title.clone()}
                                    current={selected}
                                    onclick={select(&section.id)}
                                />
                            }
                        }) }
                    </div>
                </nav>

                <article class="help-body" ref={body} data-section={shown.map(|s| s.id.clone())}>
                    if let Some(section) = shown {
                        <h3 data-test="help-title">{section.title.clone()}</h3>
                        { markdown::render(&section.body, &go, Some(&highlight)) }
                    } else {
                        <p class="text-muted" data-test="help-empty">{format!("No help mentions “{}”.", query.trim())}</p>
                    }
                </article>
            </div>
        </Modal>
    }
}
