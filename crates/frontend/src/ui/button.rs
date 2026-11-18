use sockel::{ButtonSize, ButtonVariant};
use web_sys::MouseEvent;
use yew::html::IntoPropValue;
use yew::prelude::*;

use crate::class;
use crate::router::{Route, Router};
use crate::ui::link;

/// The colour variant of a [`Button`]; [`Variant::Secondary`] is the plain
/// button.
#[derive(Clone, Copy, PartialEq, Default)]
pub(crate) enum Variant {
    #[default]
    Secondary,
    Primary,
    Success,
    Danger,
}

impl From<Variant> for ButtonVariant {
    fn from(variant: Variant) -> Self {
        match variant {
            Variant::Secondary => ButtonVariant::Secondary,
            Variant::Primary => ButtonVariant::Primary,
            Variant::Success => ButtonVariant::Success,
            Variant::Danger => ButtonVariant::Danger,
        }
    }
}

/// The size of a [`Button`]: [`Size::Small`] for actions in a dense head or
/// list row; phones keep the touch height.
#[derive(Clone, Copy, PartialEq, Default)]
pub(crate) enum Size {
    #[default]
    Normal,
    Small,
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
    /// Optional label shown inside the button on mobile; on desktop the
    /// button is its icon alone.
    #[prop_or_default]
    pub(crate) text: Option<AttrValue>,
    /// Optional label shown inside the button at every width.
    #[prop_or_default]
    pub(crate) label: Option<AttrValue>,
    #[prop_or_default]
    pub(crate) variant: Variant,
    #[prop_or_default]
    pub(crate) size: Size,
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
    /// Whether a toggle button is on (`aria-pressed`).
    #[prop_or_default]
    pub(crate) pressed: Option<bool>,
    /// Whether what the button opens is showing (`aria-expanded`).
    #[prop_or_default]
    pub(crate) expanded: Option<bool>,
    /// What the button opens: `menu` or `dialog` (`aria-haspopup`).
    #[prop_or_default]
    pub(crate) haspopup: Option<AttrValue>,
    /// An ARIA role other than button, such as `menuitem` or `switch`.
    #[prop_or_default]
    pub(crate) role: Option<AttrValue>,
    /// Whether a `switch` is on (`aria-checked`).
    #[prop_or_default]
    pub(crate) checked: Option<bool>,
    /// A link styled as a button, to another document or a fragment.
    #[prop_or_default]
    pub(crate) href: Option<AttrValue>,
    /// A link styled as a button, to a page of the app.
    #[prop_or_default]
    pub(crate) to: Option<Route>,
    /// Extra content after the icon and label, e.g. a flag.
    #[prop_or_default]
    pub(crate) children: Children,
}

/// sockel's button with an app icon by name, a required `title` tooltip, and
/// a `label` shown at every width or a `text` shown on mobile only.
#[function_component]
pub(crate) fn Button(props: &Props) -> Html {
    // A switch without an icon shows whether it is on: a check or a cross.
    let icon = if !props.icon.is_empty() {
        Some(
            html! { <span class={classes!(class::ICON, props.icon.clone(), props.spin.then_some(class::SPIN))} /> },
        )
    } else {
        props.checked.map(
            |on| html! { <span class={classes!(class::ICON, if on { class::ICON_CHECK } else { class::ICON_X_MARK })} /> },
        )
    };
    let router = use_context::<Router>();
    let href = props
        .to
        .as_ref()
        .map(|to| AttrValue::from(to.to_string()))
        .or_else(|| props.href.clone());
    let onclick = match (&props.to, router) {
        (Some(to), Some(router)) => {
            let to = to.clone();
            Callback::from(move |e: MouseEvent| {
                link::follow(&router, &to, &e);
            })
        }
        _ => props.onclick.clone(),
    };
    let size = match props.size {
        Size::Normal => ButtonSize::Regular,
        Size::Small => ButtonSize::Small,
    };

    let label = props.label.clone().or_else(|| props.text.clone());

    // A button of the app's own making, its content composed by the caller
    // (a picture, a logo, a count), keeps the app's `button` look.
    if props.icon.is_empty() && label.is_none() && props.checked.is_none() {
        let variant = match props.variant {
            Variant::Secondary => None,
            Variant::Primary => Some(sockel::class::PRIMARY),
            Variant::Success => Some(sockel::class::SUCCESS),
            Variant::Danger => Some(sockel::class::DANGER),
        };
        let class = classes!(variant, props.class.clone());

        return html! {
            <button type="button" ref={props.node_ref.clone()} {class} title={props.title.clone()} disabled={props.disabled} aria-current={props.current.then_some("page")} aria-pressed={props.pressed.map(bool_attr)} aria-expanded={props.expanded.map(bool_attr)} aria-haspopup={props.haspopup.clone()} role={props.role.clone()} aria-checked={props.checked.map(bool_attr)} {onclick}>
                { for icon }
                { for props.children.iter() }
            </button>
        };
    }

    // A plain switch is drawn as a field: sockel's ghost, given the field's
    // ground and edge in main.scss.
    let variant = match (props.variant, props.checked) {
        (Variant::Secondary, Some(_)) => ButtonVariant::Ghost,
        (variant, _) => ButtonVariant::from(variant),
    };

    html! {
        <sockel::Button {variant} {size} {icon}
            fold_wide={props.label.is_none() && props.text.is_some()}
            class={props.class.clone()} title={props.title.clone()} node_ref={props.node_ref.clone()}
            {href} disabled={props.disabled} {onclick}
            current={props.current.then_some(AttrValue::Static("page"))}
            pressed={props.pressed} expanded={props.expanded} haspopup={props.haspopup.clone()}
            role={props.role.clone()} checked={props.checked}>
            if let Some(label) = label {
                {label}
            }
            { for props.children.iter() }
        </sockel::Button>
    }
}

fn bool_attr(value: bool) -> AttrValue {
    AttrValue::Static(if value { "true" } else { "false" })
}
