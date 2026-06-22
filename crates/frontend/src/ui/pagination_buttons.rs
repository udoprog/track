use web_sys::MouseEvent;
use yew::prelude::*;

#[derive(Properties, PartialEq)]
pub(crate) struct Props {
    pub(crate) page: usize,
    pub(crate) total_pages: usize,
    pub(crate) on_page: Callback<usize>,
}

#[function_component]
pub(crate) fn PaginationButtons(props: &Props) -> Html {
    let page = props.page.min(props.total_pages.saturating_sub(1));
    let prev = page.checked_sub(1);
    let next = page.checked_add(1).filter(|&v| v < props.total_pages);

    let on_back = prev.map(|prev| props.on_page.reform(move |_: MouseEvent| prev));
    let on_next = next.map(|next| props.on_page.reform(move |_: MouseEvent| next));

    html! {
        <>
            <button class={classes!("btn", prev.is_none().then_some("disabled"))} onclick={on_back}>
                <span class="icon chevron-left" />
            </button>

            <span class="input-text">
                {format!("{} / {}", page.saturating_add(1), props.total_pages)}
            </span>

            <button class={classes!("btn", next.is_none().then_some("disabled"))} onclick={on_next}>
                <span class="icon chevron-right" />
            </button>
        </>
    }
}
