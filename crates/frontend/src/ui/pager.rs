use yew::prelude::*;

#[derive(Properties, PartialEq)]
pub(crate) struct Props {
    pub(crate) page: usize,
    pub(crate) pages: usize,
    pub(crate) on_page: Callback<usize>,
}

/// plinth's Pager as track lays it out: no first and last page, previous
/// and next hidden on phones, and a tooltip on every button.
#[function_component]
pub(crate) fn Pager(props: &Props) -> Html {
    html! {
        <plinth::Pager page={props.page} pages={props.pages} on_page={props.on_page.clone()}
            edges=false phone_steps=false titles=true />
    }
}
