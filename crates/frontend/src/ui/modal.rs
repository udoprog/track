use yew::prelude::*;

#[derive(Properties, PartialEq)]
pub(crate) struct Props {
    #[prop_or_default]
    pub(crate) icon: Option<AttrValue>,
    #[prop_or_default]
    pub(crate) title: Option<Html>,
    pub(crate) children: Children,
    pub(crate) on_close: Callback<()>,
}

#[function_component]
pub(crate) fn Modal(props: &Props) -> Html {
    let on_close = props.on_close.reform(|_| ());

    html! {
        <div class="modal-background" onclick={on_close.clone()}>
            <div class="modal" onclick={Callback::from(|e: MouseEvent| e.stop_propagation())}>
                <div class="modal-header">
                    if let Some(ref title) = props.title {
                        <h2 class="row text-gap">
                            if let Some(ref icon) = props.icon {
                                <span class="item-inline">
                                    <span class={classes!("icon", icon)} />
                                </span>
                            }

                            <span>{title.clone()}</span>
                        </h2>
                    }

                    <button class="btn" onclick={on_close.clone()} title="Close">
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
