use std::sync::atomic::{AtomicUsize, Ordering};

use web_sys::{HtmlElement, KeyboardEvent};
use yew::prelude::*;

use crate::ui::Button;
use crate::ui::focus;

static NEXT_ID: AtomicUsize = AtomicUsize::new(0);

#[derive(Properties, PartialEq)]
pub(crate) struct Props {
    #[prop_or_default]
    pub(crate) icon: Option<AttrValue>,
    #[prop_or_default]
    pub(crate) title: Option<Html>,
    pub(crate) children: Children,
    pub(crate) on_close: Callback<()>,
}

/// A dialog over the page. It takes focus when it opens, keeps Tab inside,
/// closes on Escape and hands focus back to whatever had it before.
#[function_component]
pub(crate) fn Modal(props: &Props) -> Html {
    let on_close = props.on_close.reform(|_| ());
    let dialog = use_node_ref();
    let content = use_node_ref();
    let title_id = use_memo((), |_| {
        format!("modal-title-{}", NEXT_ID.fetch_add(1, Ordering::Relaxed))
    });

    {
        let dialog = dialog.clone();
        let content = content.clone();

        use_effect_with((), move |_| {
            let opener = focus::active();

            if let Some(dialog) = dialog.cast::<HtmlElement>() {
                focus::focus_first(&dialog, content.cast().as_ref());
            }

            move || focus::restore(opener)
        });
    }

    let onkeydown = {
        let dialog = dialog.clone();
        let on_close = props.on_close.clone();

        Callback::from(move |e: KeyboardEvent| {
            if e.key() == "Escape" {
                e.stop_propagation();
                on_close.emit(());
                return;
            }

            if let Some(dialog) = dialog.cast::<HtmlElement>() {
                focus::trap_tab(&e, &dialog);
            }
        })
    };

    html! {
        <div class="modal-background" onclick={on_close.clone()}>
            <div
                class="modal"
                ref={dialog}
                role="dialog"
                aria-modal="true"
                aria-labelledby={props.title.is_some().then(|| (*title_id).clone())}
                tabindex="-1"
                {onkeydown}
                onclick={Callback::from(|e: MouseEvent| e.stop_propagation())}
            >
                <div class="modal-header">
                    if let Some(ref title) = props.title {
                        <h2 class="row text-gap" id={(*title_id).clone()}>
                            if let Some(ref icon) = props.icon {
                                <span class="item-inline" aria-hidden="true">
                                    <span class={classes!("icon", icon)} />
                                </span>
                            }

                            <span>{title.clone()}</span>
                        </h2>
                    }

                    <Button icon="x-mark" title="Close" class="ghost" onclick={on_close.clone()} />
                </div>

                <div class="modal-content" ref={content}>
                    { for props.children.iter() }
                </div>
            </div>
        </div>
    }
}
