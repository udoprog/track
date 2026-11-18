use yew::prelude::*;

use crate::class;
use crate::error::RcError;

#[derive(Properties, PartialEq)]
pub(crate) struct Props {
    pub(crate) error: RcError,
    pub(crate) onclearerror: Callback<()>,
}

/// sockel's ErrorBox: what was being done, then its causes, most specific
/// last, and a dismiss button.
#[function_component]
pub(crate) fn ErrorBox(props: &Props) -> Html {
    let mut sources = props.error.sources();
    let message = sources.next().map(|e| e.to_string()).unwrap_or_default();
    let causes = sources
        .map(|e| AttrValue::from(e.to_string()))
        .collect::<Vec<_>>();

    html! {
        <sockel::ErrorBox
            icon={html! { <span class={classes!(class::ICON, class::ICON_EXCLAMATION_TRIANGLE, class::DANGER)} /> }}
            {causes}
            ondismiss={props.onclearerror.reform(|_| ())}
            dismiss_label="Dismiss error">
            {message}
        </sockel::ErrorBox>
    }
}
