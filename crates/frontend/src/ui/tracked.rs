use yew::prelude::*;

#[derive(Properties, PartialEq)]
pub(crate) struct Props {
    pub(crate) tracked: bool,
    pub(crate) ontoggle: Callback<bool>,
}

#[function_component]
pub(crate) fn Tracked(props: &Props) -> Html {
    let tracked = props.tracked;

    html! {
        <button class="btn" onclick={props.ontoggle.reform(move |_| !tracked)} title="Track movie">
            <span class={classes!("icon", if tracked { "eye" } else { "eye-slash" })} />
            <span class="hide-desktop">{if tracked { "Tracking" } else { "Not tracking" }}</span>
        </button>
    }
}
