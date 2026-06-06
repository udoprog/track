use yew::prelude::*;

#[derive(Properties, PartialEq)]
pub(super) struct ConfirmDangerProps {
    pub(super) label: AttrValue,
    pub(super) on_confirm: Callback<()>,
    pub(super) on_cancel: Callback<()>,
    pub(super) prompt: AttrValue,
    #[prop_or_default]
    pub(super) btn_class: Classes,
}

#[function_component]
pub(super) fn ConfirmDanger(props: &ConfirmDangerProps) -> Html {
    let on_confirm = {
        let cb = props.on_confirm.clone();
        Callback::from(move |e: MouseEvent| {
            e.stop_propagation();
            cb.emit(());
        })
    };

    let on_cancel = {
        let cb = props.on_cancel.clone();
        Callback::from(move |e: MouseEvent| {
            e.stop_propagation();
            cb.emit(());
        })
    };

    html! {
        <div class="row-fill">
            <span>{&props.prompt}{" "}<strong>{&props.label}</strong>{"?"}</span>
            <div class="input-group end">
                <button onclick={on_cancel} class={classes!("btn-icon", &props.btn_class)} title="No">
                    <span class="icon x-mark" />
                </button>
                <button onclick={on_confirm} class={classes!("btn-icon-danger", &props.btn_class)} title="Yes">
                    <span class="icon check" />
                </button>
            </div>
        </div>
    }
}
