use plinth::{Menu, MenuRadio, PopoverAlign};
use yew::prelude::*;

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

    let label = props
        .options
        .iter()
        .find(|(value, _)| *value == props.current)
        .map_or(props.current, |(_, label)| *label);

    let on_toggle = {
        let open = open.clone();
        Callback::from(move |on| open.set(on))
    };

    html! {
        <Menu text={html! { label }} icon={html! { <span class="icon arrows-up-down" /> }}
            title="Sort by" label="Sort by" opener_class={classes!("chip", (*open).then_some("selected"))}
            align={PopoverAlign::Start} sheet=true
            open={Some(*open)} {on_toggle} testid="sort-menu">
            { for props.options.iter().map(|&(value, label)| {
                let on_change = props.on_change.clone();

                html! {
                    <MenuRadio checked={value == props.current} on_pick={Callback::from(move |()| on_change.emit(value))}
                        testid={format!("sort-{value}")}>
                        {label}
                    </MenuRadio>
                }
            }) }
        </Menu>
    }
}
