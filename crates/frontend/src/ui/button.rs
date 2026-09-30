use web_sys::MouseEvent;
use yew::html::IntoPropValue;
use yew::prelude::*;

/// The colour variant of a [`Button`]. [`Variant::Secondary`] is the default
/// (plain) button; the others map to the `primary`/`success`/`danger` element
/// modifiers.
#[derive(Clone, Copy, PartialEq, Default)]
pub(crate) enum Variant {
    #[default]
    Secondary,
    Primary,
    Success,
    Danger,
}

impl Variant {
    fn class(self) -> Option<&'static str> {
        match self {
            Variant::Secondary => None,
            Variant::Primary => Some("primary"),
            Variant::Success => Some("success"),
            Variant::Danger => Some("danger"),
        }
    }
}

impl IntoPropValue<Variant> for &str {
    fn into_prop_value(self) -> Variant {
        match self {
            "primary" => Variant::Primary,
            "success" => Variant::Success,
            "danger" => Variant::Danger,
            _ => Variant::Secondary,
        }
    }
}

#[derive(Properties, PartialEq)]
pub(crate) struct Props {
    /// The icon name rendered inside the button (e.g. `"check"`).
    pub(crate) icon: AttrValue,
    /// The tooltip / accessible label. Always present.
    pub(crate) title: AttrValue,
    /// Optional label shown inside the button on mobile and hidden on desktop.
    #[prop_or_default]
    pub(crate) text: Option<AttrValue>,
    /// Optional label shown inside the button at every width.
    #[prop_or_default]
    pub(crate) label: Option<AttrValue>,
    #[prop_or_default]
    pub(crate) variant: Variant,
    /// Extra classes for the button (e.g. `fill`, `selected`).
    #[prop_or_default]
    pub(crate) class: Classes,
    /// Forwarded to the inner `<button>`, e.g. to anchor a popover to it.
    #[prop_or_default]
    pub(crate) node_ref: NodeRef,
    #[prop_or_default]
    pub(crate) disabled: bool,
    #[prop_or_default]
    pub(crate) onclick: Callback<MouseEvent>,
    #[prop_or_default]
    pub(crate) spin: bool,
}

/// A standard action button: a required icon, a required `title` tooltip, an
/// optional `text` label that is shown on mobile and hidden on desktop, and an
/// optional `label` shown at every width.
#[function_component]
pub(crate) fn Button(props: &Props) -> Html {
    let class = classes!(
        props.variant.class(),
        props.text.is_some().then_some("mobile-has-text"),
        props.label.is_some().then_some("has-text"),
        props.class.clone(),
    );

    html! {
        <button ref={props.node_ref.clone()} {class} title={props.title.clone()} disabled={props.disabled} onclick={props.onclick.clone()}>
            <span class={classes!("icon", props.icon.clone(), props.spin.then_some("spin"))} />

            if let Some(label) = &props.label {
                <span>{label}</span>
            } else if let Some(text) = &props.text {
                <span class="mobile-only">{text}</span>
            }
        </button>
    }
}
