use yew::prelude::*;

#[derive(Properties, PartialEq)]
pub(super) struct Props {
    #[prop_or_default]
    pub(super) title: Option<Html>,
    pub(super) children: Children,
    pub(super) on_close: Callback<()>,
}

#[function_component]
pub(super) fn Modal(props: &Props) -> Html {
    let on_close = props.on_close.reform(|_| ());

    html! {
        <div class="modal-background" onclick={on_close.clone()}>
            <div class="modal" onclick={Callback::from(|e: MouseEvent| e.stop_propagation())}>
                <div class="modal-header">
                    if let Some(ref title) = props.title {
                        <h2 class="row">{title.clone()}</h2>
                    }

                    <button class="btn end" onclick={on_close.clone()} title="Close">
                        <span class="icon x-mark" />
                    </button>
                </div>

                <div class="modal-content">
                    { for props.children.iter() }
                </div>
            </div>
        </div>
    }
}
