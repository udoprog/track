use yew::prelude::*;

use crate::error::RcError;
use crate::ui::{Button, Variant};

#[derive(Properties, PartialEq)]
pub(crate) struct Props {
    pub(crate) error: RcError,
    pub(crate) onclearerror: Callback<()>,
}

#[function_component]
pub(crate) fn ErrorBox(props: &Props) -> Html {
    html! {
        <>
            <div class="column fill">
                { for props.error.sources().map(|e| html! { <p>{e.to_string()}</p> }) }
            </div>

            <Button icon="x-mark" title="Dismiss error" variant={Variant::Danger} onclick={props.onclearerror.reform(|_| ())} />
        </>
    }
}
