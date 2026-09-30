use yew::prelude::*;

use crate::ui::{Button, ContextMenu};

#[derive(Properties, PartialEq)]
pub(crate) struct Props {
    /// The fields to sort by, as `(value, label)`.
    pub(crate) options: &'static [(&'static str, &'static str)],
    /// The value of the field sorted by now.
    pub(crate) current: &'static str,
    pub(crate) on_change: Callback<&'static str>,
}

/// A chip naming the field a list is sorted by, which opens a menu of the
/// fields to choose from.
#[function_component]
pub(crate) fn SortMenu(props: &Props) -> Html {
    let open = use_state(|| false);
    let anchor = use_node_ref();

    let label = props
        .options
        .iter()
        .find(|(value, _)| *value == props.current)
        .map_or(props.current, |(_, label)| *label);

    let on_toggle = {
        let open = open.clone();
        Callback::from(move |_| open.set(!*open))
    };

    let on_close = {
        let open = open.clone();
        Callback::from(move |()| open.set(false))
    };

    html! {
        <>
            <Button node_ref={anchor.clone()} icon="arrows-up-down" label={label} title="Sort by" class={classes!("chip", (*open).then_some("selected"))} expanded={Some(*open)} haspopup="menu" onclick={on_toggle} />

            if *open {
                <ContextMenu {anchor} on_close={on_close.clone()}>
                    <div class="menu-list" role="menu" aria-label="Sort by">
                        { for props.options.iter().map(|&(value, label)| {
                            let checked = value == props.current;
                            let on_change = props.on_change.clone();
                            let on_close = on_close.clone();

                            let onclick = Callback::from(move |_| {
                                on_close.emit(());
                                on_change.emit(value);
                            });

                            html! {
                                <Button role="menuitemradio" checked={Some(checked)} icon="check" class={classes!((!checked).then_some("unchecked"))} label={label} title={format!("Sort by {}", label.to_lowercase())} {onclick} />
                            }
                        }) }
                    </div>
                </ContextMenu>
            }
        </>
    }
}
