use yew::prelude::*;

use super::LOADING;

/// Loading indicator placed inside the shared page container (rendered by
/// `App`). Use [`LoadingPage`] for standalone, full-page loading screens.
#[function_component]
pub(crate) fn Loading() -> Html {
    html! {
        <div class="box info">
            <span class="item-inline">
                <span class="icon arrow-path spin" />
            </span>

            <span>{LOADING}</span>
        </div>
    }
}
