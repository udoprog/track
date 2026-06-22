use yew::prelude::*;

use crate::error::RcError;

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

            <button class="btn-danger" onclick={props.onclearerror.reform(|_| ())}>
                <span class="icon x-mark" />
            </button>
        </>
    }
}
