use yew::prelude::*;

#[derive(Properties, PartialEq)]
pub(crate) struct Props {
    pub(crate) label: AttrValue,
    #[prop_or_default]
    pub(crate) hint: Option<AttrValue>,
    pub(crate) children: Children,
}

/// One setting in a `.form-rows` grid: its label beside the control, with an
/// optional hint under the control. The labels of a grid share one column, and
/// on phones each label sits above its control.
#[function_component]
pub(crate) fn FormRow(props: &Props) -> Html {
    html! {
        <div class="form-row">
            <span class="form-label">{props.label.clone()}</span>

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
