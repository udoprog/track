use std::sync::atomic::{AtomicUsize, Ordering};

use sockel::FieldInputs;
use yew::prelude::*;

use crate::ui::Help;

static NEXT_ID: AtomicUsize = AtomicUsize::new(0);

#[derive(Properties, PartialEq)]
pub(crate) struct Props {
    pub(crate) label: AttrValue,
    #[prop_or_default]
    pub(crate) hint: Option<AttrValue>,
    /// The help section explaining this setting, opened from a mark beside
    /// the label.
    #[prop_or_default]
    pub(crate) help: Option<&'static str>,
    /// Stack the inputs in a column instead of a wrapping line, for lists and
    /// editors that take the width.
    #[prop_or_default]
    pub(crate) stack: bool,
    pub(crate) children: Children,
}

/// One setting in a `sockel::form_rows` grid: sockel's FormRow as a group
/// named by its label, its controls in a wrapping line or a column.
#[function_component]
pub(crate) fn FormRow(props: &Props) -> Html {
    let id = use_memo((), |_| {
        format!("form-row-{}", NEXT_ID.fetch_add(1, Ordering::Relaxed))
    });

    let help = props.help.map(|section| html! { <Help {section} /> });

    let inputs = if props.stack {
        FieldInputs::Stack
    } else {
        FieldInputs::Row
    };

    html! {
        <sockel::FormRow id={(*id).clone()} label={html! { {props.label.clone()} }} {help} hint={props.hint.clone().map(|hint| html! { {hint} })} group=true {inputs}>
            { for props.children.iter() }
        </sockel::FormRow>
    }
}
