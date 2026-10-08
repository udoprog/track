use yew::prelude::*;

/// How many names show before "+N more".
const FIRST: usize = 3;

#[derive(Properties, PartialEq)]
pub(crate) struct Props {
    pub(crate) names: Vec<api::AltName>,
}

/// A show's other names: the first few with their language, and the rest
/// behind a button that shows them in place.
#[function_component]
pub(crate) fn AlsoKnownAs(props: &Props) -> Html {
    let expanded = use_state(|| false);

    if props.names.is_empty() {
        return html! {};
    }

    let shown = if *expanded {
        props.names.len()
    } else {
        props.names.len().min(FIRST)
    };

    let more = props.names.len() - shown;

    let on_more = {
        let expanded = expanded.clone();
        Callback::from(move |_: MouseEvent| expanded.set(true))
    };

    html! {
        <p class="also-known-as">
            <span class="also-known-as-label">{"Also known as"}</span>

            { for props.names[..shown].iter().map(|n| html! {
                <span class="alt-name">
                    <span>{n.name.clone()}</span>

                    if let Some(language) = &n.language {
                        <span class="alt-name-language" title="Language">{language.to_uppercase()}</span>
                    }
                </span>
            }) }

            if more > 0 {
                <button type="button" class="link-button" title="Show every name" onclick={on_more}>
                    {format!("+{more} more")}
                </button>
            }
        </p>
    }
}
