use std::collections::{BTreeSet, HashSet};

use yew::prelude::*;

use crate::ui::Button;

#[derive(Properties, PartialEq)]
pub(crate) struct Props {
    pub(crate) present: BTreeSet<api::ImageSource>,
    pub(crate) hidden: HashSet<api::ImageSource>,
    pub(crate) on_toggle: Callback<api::ImageSource>,
}

/// A checkbox per present graphics source, letting the user hide/show images by
/// remote. Renders nothing when fewer than two sources are present (a filter
/// over a single source is pointless). Drops into an existing `input-group`.
#[function_component]
pub(crate) fn GraphicsSourceFilter(props: &Props) -> Html {
    if props.present.is_empty() {
        return html!();
    }

    html! {
        <div class="input-group">
            {for props.present.iter().copied().map(|source| {
                let checked = !props.hidden.contains(&source);
                let on_toggle = props.on_toggle.reform(move |_| source);

                html! {
                    <Button class={classes!("input-checkbox", "has-text", checked.then_some("checked"))} role="switch" checked={Some(checked)} title={source.to_string()} onclick={on_toggle}>
                        <span class="mark" />
                        <span class={classes!("logo", source.as_str())} />
                    </Button>
                }
            })}
        </div>
    }
}
