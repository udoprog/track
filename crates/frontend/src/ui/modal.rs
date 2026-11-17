use sockel::{ButtonSize, Dialog, DialogSize};
use yew::prelude::*;

#[derive(Properties, PartialEq)]
pub(crate) struct Props {
    pub(crate) icon: AttrValue,
    pub(crate) title: AttrValue,
    #[prop_or_default]
    pub(crate) size: DialogSize,
    #[prop_or_default]
    pub(crate) flush: bool,
    #[prop_or_default]
    pub(crate) class: Classes,
    #[prop_or_default]
    pub(crate) testid: Option<AttrValue>,
    pub(crate) on_close: Callback<()>,
    #[prop_or_default]
    pub(crate) children: Html,
}

/// sockel's dialog, open while rendered, with an app icon by name before
/// its title and a regular close button.
#[function_component]
pub(crate) fn Modal(props: &Props) -> Html {
    let icon = html! {
        <span class="item-inline">
            <span class={classes!("icon", "muted", props.icon.clone())} />
        </span>
    };

    html! {
        <Dialog open=true on_dismiss={props.on_close.reform(|_| ())} title={props.title.clone()} {icon}
            size={props.size} flush={props.flush} close_size={ButtonSize::Regular}
            class={props.class.clone()} testid={props.testid.clone()}>
            { props.children.clone() }
        </Dialog>
    }
}
