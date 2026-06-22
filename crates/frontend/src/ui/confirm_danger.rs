use web_sys::MouseEvent;
use yew::prelude::*;

#[derive(Properties, PartialEq)]
pub(crate) struct Props {
    pub(crate) prompt: AttrValue,
    #[prop_or_default]
    pub(crate) icon: Option<AttrValue>,
    #[prop_or_default]
    pub(crate) label: Option<AttrValue>,
    pub(crate) on_confirm: Callback<()>,
    pub(crate) on_cancel: Callback<()>,
    #[prop_or_default]
    pub(crate) btn_class: Classes,
}

#[function_component]
pub(crate) fn ConfirmDanger(props: &Props) -> Html {
    let on_confirm = props.on_confirm.reform(move |e: MouseEvent| {
        e.stop_propagation();
    });

    let on_cancel = props.on_cancel.reform(move |e: MouseEvent| {
        e.stop_propagation();
    });

    html! {
        <div class="row-split fill">
            <div class="row text-gap">
                if let Some(ref icon) = props.icon {
                    <span class="item-inline">
                        <span class={classes!("icon", icon)} />
                    </span>
                }

                <span>{&props.prompt}</span>

                if let Some(ref label) = props.label {
                    <span>{label}{"?"}</span>
                }
            </div>

            <div class="input-group end">
                <button onclick={on_cancel} class={classes!("btn", &props.btn_class)} title="No">
                    <span class="icon x-mark" />
                </button>

                <button onclick={on_confirm} class={classes!("btn-danger", &props.btn_class)} title="Yes">
                    <span class="icon check" />
                </button>
            </div>
        </div>
    }
}
