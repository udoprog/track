use web_sys::MouseEvent;
use yew::prelude::*;

use crate::ui::{Button, Variant};

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
                <Button icon="x-mark" title="No" class={props.btn_class.clone()} onclick={on_cancel} />

                <Button icon="check" title="Yes" variant={Variant::Danger} class={props.btn_class.clone()} onclick={on_confirm} />
            </div>
        </div>
    }
}
