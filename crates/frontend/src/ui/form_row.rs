use std::sync::atomic::{AtomicUsize, Ordering};

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
    pub(crate) children: Children,
}

/// One setting in a `.form-rows` grid: its label beside the control, with an
/// optional hint under the control. The labels of a grid share one column, and
/// on phones each label sits above its control. The row is a group named by
/// its label, so the control inside is announced with it.
#[function_component]
pub(crate) fn FormRow(props: &Props) -> Html {
    let id = use_memo((), |_| {
        format!("form-label-{}", NEXT_ID.fetch_add(1, Ordering::Relaxed))
    });

    html! {
        <div class="form-row" role="group" aria-labelledby={(*id).clone()}>
            <span class="form-label" id={(*id).clone()}>
                {props.label.clone()}

                if let Some(section) = props.help {
                    <Help {section} />
                }
            </span>

            <div class="form-control">
                <div class="form-inputs">
                    { for props.children.iter() }
                </div>

                if let Some(hint) = &props.hint {
                    <span class="hint">{hint.clone()}</span>
                }
            </div>
        </div>
    }
}
