use yew::prelude::*;

use crate::error::RcError;
use crate::ui::Button;

#[derive(Properties, PartialEq)]
pub(crate) struct Props {
    pub(crate) error: RcError,
    pub(crate) onclearerror: Callback<()>,
}

#[function_component]
pub(crate) fn ErrorBox(props: &Props) -> Html {
    let mut sources = props.error.sources();
    let message = sources.next().map(|e| e.to_string()).unwrap_or_default();

    html! {
        <>
            <span class="icon exclamation-triangle" aria-hidden="true" />

            <div class="error-text">
                <strong>{message}</strong>

                // What caused it, most specific last.
                { for sources.map(|e| html! { <span class="error-cause">{e.to_string()}</span> }) }
            </div>

            <Button icon="x-mark" title="Dismiss error" onclick={props.onclearerror.reform(|_| ())} />
        </>
    }
}
