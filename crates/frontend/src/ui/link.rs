use web_sys::MouseEvent;
use yew::prelude::*;

use crate::router::{Route, Router};

#[derive(Properties, PartialEq)]
pub(crate) struct Props {
    pub(crate) to: Route,
    #[prop_or_default]
    pub(crate) class: Classes,
    #[prop_or_default]
    pub(crate) title: Option<AttrValue>,
    /// Marks the link as the page being shown (`aria-current`).
    #[prop_or_default]
    pub(crate) current: bool,
    /// A second way to the same page as a labelled link nearby, such as a
    /// card's poster beside its title: clickable, but skipped by Tab and
    /// screen readers.
    #[prop_or_default]
    pub(crate) decorative: bool,
    /// Runs on a plain click before navigating, e.g. to close a menu.
    #[prop_or_default]
    pub(crate) onclick: Callback<()>,
    pub(crate) children: Children,
}

/// A link to a page of the app: a real `href`, so it can be opened in a new
/// tab and is announced as a link, while a plain click navigates in place.
#[function_component]
pub(crate) fn Link(props: &Props) -> Html {
    let router = use_context::<Router>().expect("router in context");

    let onclick = {
        let to = props.to.clone();
        let then = props.onclick.clone();

        Callback::from(move |e: MouseEvent| {
            // Modified and middle clicks are the browser's (new tab, window).
            if e.button() != 0 || e.ctrl_key() || e.meta_key() || e.shift_key() || e.alt_key() {
                return;
            }

            e.prevent_default();
            then.emit(());
            router.push(to.clone());
        })
    };

    html! {
        <a
            href={props.to.to_string()}
            class={props.class.clone()}
            title={props.title.clone()}
            aria-current={props.current.then_some("page")}
            tabindex={props.decorative.then_some("-1")}
            aria-hidden={props.decorative.then_some("true")}
            {onclick}
        >
            { for props.children.iter() }
        </a>
    }
}
