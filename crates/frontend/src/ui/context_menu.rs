//! A small dialog beside the control that opened it: sockel's popover, with
//! the prompt it asks ("Displayed language", "View options") over its body.
//! Focus moves to its first control and Tab stays inside; Escape or a press
//! outside closes it, and focus goes back. Phones show it as a sheet.

use yew::prelude::*;

#[derive(Properties, PartialEq)]
pub(crate) struct Props {
    #[prop_or_default]
    pub(crate) icon: Option<AttrValue>,
    #[prop_or_default]
    pub(crate) prompt: Option<AttrValue>,
    /// What the prompt asks about, in bold after it.
    #[prop_or_default]
    pub(crate) label: Option<AttrValue>,
    /// The element it opens beside.
    pub(crate) anchor: NodeRef,
    /// Asked to close it.
    pub(crate) on_close: Callback<()>,
    pub(crate) children: Children,
}

#[function_component]
pub(crate) fn ContextMenu(props: &Props) -> Html {
    let title = match (&props.prompt, &props.label) {
        (Some(prompt), Some(label)) => Some(AttrValue::from(format!("{prompt} {label}"))),
        (prompt, _) => prompt.clone(),
    };

    html! {
        <sockel::Popover anchor={props.anchor.clone()} open=true {title} head=false trap=true focus_first=true
            on_dismiss={props.on_close.reform(|_| ())} testid="context-menu">
            <div class="sockel-stack sockel-loose">
                if let Some(prompt) = &props.prompt {
                    <div class="sockel-cluster sockel-snug">
                        if let Some(icon) = &props.icon {
                            <span class="item-inline" aria-hidden="true">
                                <span class={classes!("icon", icon.clone())} aria-hidden="true" />
                            </span>
                        }

                        <span>{prompt.clone()}</span>

                        if let Some(label) = &props.label {
                            <span><span class="context-menu-label">{label.clone()}</span>{"?"}</span>
                        }
                    </div>
                }

                { for props.children.iter() }
            </div>
        </sockel::Popover>
    }
}
