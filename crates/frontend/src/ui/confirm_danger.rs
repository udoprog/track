use web_sys::MouseEvent;
use yew::prelude::*;

#[derive(Properties, PartialEq)]
pub(crate) struct Props {
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
        <div class="row align-end">
            <div class="input-group">
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
