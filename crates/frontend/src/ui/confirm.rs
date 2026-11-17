//! Asking before a destructive action, beside the button that asked:
//! sockel's compact confirm, "Remove show **Title**?" over an icon-only
//! No and Yes.

use yew::prelude::*;

#[derive(Properties, PartialEq)]
pub(crate) struct Props {
    pub(crate) anchor: NodeRef,
    pub(crate) prompt: AttrValue,
    /// What it acts on, in bold after the prompt.
    #[prop_or_default]
    pub(crate) label: Option<AttrValue>,
    #[prop_or_default]
    pub(crate) icon: Option<AttrValue>,
    pub(crate) on_confirm: Callback<()>,
    pub(crate) on_cancel: Callback<()>,
}

#[function_component]
pub(crate) fn Confirm(props: &Props) -> Html {
    let (title, details) = match &props.label {
        Some(label) => (
            format!("{} {label}", props.prompt),
            html! { <span><span class="context-menu-label">{label.clone()}</span><span>{"?"}</span></span> },
        ),
        None => (props.prompt.to_string(), Html::default()),
    };

    let icon = props.icon.as_ref().map(|icon| {
        html! {
            <span class="item-inline" aria-hidden="true">
                <span class={classes!("icon", icon.clone())} aria-hidden="true" />
            </span>
        }
    });

    html! {
        <sockel::ConfirmPopover anchor={props.anchor.clone()} open=true compact=true {title}
            explanation={props.prompt.clone()} {details} explanation_icon={icon}
            cancel_label="No" accept="Yes"
            on_accept={props.on_confirm.clone()} on_cancel={props.on_cancel.clone()} />
    }
}
