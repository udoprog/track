use yew::prelude::*;

use crate::ui::Button;

#[derive(Properties, PartialEq)]
pub(crate) struct Props {
    pub(crate) tracked: bool,
    pub(crate) ontoggle: Callback<bool>,
}

#[function_component]
pub(crate) fn Tracked(props: &Props) -> Html {
    let tracked = props.tracked;

    html! {
        <Button
            icon={if tracked { "eye" } else { "eye-slash" }}
            title="Track movie"
            text={if tracked { "Tracking" } else { "Not tracking" }}
            onclick={props.ontoggle.reform(move |_| !tracked)}
        />
    }
}
