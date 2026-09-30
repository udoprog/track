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
    /// The icon name rendered inside the button (e.g. `"check"`). Leave it
    /// out only when the children say what the button is.
    #[prop_or_default]
    pub(crate) icon: AttrValue,
    /// The tooltip / accessible label. Always present.
    pub(crate) title: AttrValue,
    /// Optional label shown inside the button on mobile and hidden on desktop.
    #[prop_or_default]
    pub(crate) text: Option<AttrValue>,
    /// Optional label shown inside the button at every width.
    #[prop_or_default]
    pub(crate) label: Option<AttrValue>,
    /// Optional label shown inside the button on desktop and hidden on mobile.
    #[prop_or_default]
    pub(crate) desktop_text: Option<AttrValue>,
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
    /// Marks the button as the current page, for navigation items.
    #[prop_or_default]
    pub(crate) current: bool,
    /// Extra content after the icon and label, e.g. a flag.
    #[prop_or_default]
    pub(crate) children: Children,
}

/// A standard action button: an icon, a required `title` tooltip, an optional
/// `text` label that is shown on mobile and hidden on desktop (or
/// `desktop_text`, the other way around), and an optional `label` shown at
/// every width.
#[function_component]
pub(crate) fn Button(props: &Props) -> Html {
    let class = classes!(
        props.variant.class(),
        props.text.is_some().then_some("mobile-has-text"),
        props.label.is_some().then_some("has-text"),
        props.desktop_text.is_some().then_some("desktop-has-text"),
        props.class.clone(),
    );

    html! {
        <button ref={props.node_ref.clone()} {class} title={props.title.clone()} disabled={props.disabled} aria-current={props.current.then_some("page")} onclick={props.onclick.clone()}>
            if !props.icon.is_empty() {
                <span class={classes!("icon", props.icon.clone(), props.spin.then_some("spin"))} />
            }

            if let Some(label) = &props.label {
                <span>{label}</span>
            } else if let Some(text) = &props.text {
                <span class="mobile-only">{text}</span>
            } else if let Some(text) = &props.desktop_text {
                <span class="desktop-only">{text}</span>
            }

            { for props.children.iter() }
        </button>
    }
}
