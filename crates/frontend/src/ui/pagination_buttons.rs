use std::iter;

use web_sys::MouseEvent;
use yew::prelude::*;

const BUTTONS: usize = 3;

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

    let last_page = props.total_pages.saturating_sub(1);

    let window = BUTTONS.min(props.total_pages);
    let mut start = page.saturating_sub(BUTTONS / 2);
    let mut end = start + window.saturating_sub(1);

    if end > last_page {
        end = last_page;
        start = (end + 1).saturating_sub(window);
    }

    // First page, the window around the current page, and the last page, in
    // ascending order. Each is paired with its distance from the previously
    // emitted page; a distance of zero is a duplicate where the window meets an
    // edge (skipped), and a distance above one is a gap rendered as an ellipsis.
    let pages = iter::once(0)
        .chain(start..=end)
        .chain(iter::once(last_page))
        .scan(None, |last: &mut Option<usize>, p| {
            let dist = last.map_or(1, |prev| p - prev);
            *last = Some(p);
            Some((p, dist))
        })
        .filter(|&(_, dist)| dist != 0);

    html! {
        <pagination>
            <button class={classes!(prev.is_none().then_some("disabled"))} onclick={on_back}>
                <span class="icon chevron-left" />
            </button>

            <pages>
                {for pages.map(|(p, dist)| html! {
                    <>
                        if dist > 1 {
                            <ellipsis>{"…"}</ellipsis>
                        }

                        <page class={classes!((page == p).then_some("current"))} onclick={props.on_page.reform(move |_: MouseEvent| p)}>
                            {p.saturating_add(1)}
                        </page>
                    </>
                })}
            </pages>

            <button class={classes!(next.is_none().then_some("disabled"))} onclick={on_next}>
                <span class="icon chevron-right" />
            </button>
        </pagination>
    }
}
