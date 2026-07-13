use yew::prelude::*;

#[derive(Properties, PartialEq)]
pub(crate) struct Props {
    pub(crate) value: api::Duration,
}

/// Renders a duration in the largest unit it is at least one of, such as
/// `24 hours` or `1.5 minutes`.
#[function_component]
pub(crate) fn DurationLabel(props: &Props) -> Html {
    html! {
        { props.value.human().to_string() }
    }
}
