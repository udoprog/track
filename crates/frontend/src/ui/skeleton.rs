use yew::prelude::*;

#[derive(Properties, PartialEq)]
pub(crate) struct Props {
    /// Extra classes to size or extend the placeholder.
    #[prop_or_default]
    pub(crate) class: Classes,
    /// Inline style, typically to constrain the width of a text-line placeholder.
    #[prop_or_default]
    pub(crate) style: Option<String>,
}

/// Shimmering placeholder shown in place of content that is still loading. Pass
/// `class` to size it for the surrounding context.
#[function_component]
pub(crate) fn Skeleton(props: &Props) -> Html {
    html! {
        <div class={classes!("skeleton", props.class.clone())} style={props.style.clone()} />
    }
}
