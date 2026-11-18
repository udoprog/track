use auth::yew023::AccountPanel;
use yew::prelude::*;

use crate::background::Background;
use crate::class;

/// The signed-in user's own account.
#[function_component]
pub(crate) fn Account() -> Html {
    use_page_title("Account");

    html! {
        <>
            <h1 class={sockel::class::VISUALLY_HIDDEN}>{"Account"}</h1>

            <div class={class::SETTINGS}>
                <AccountPanel />
            </div>
        </>
    }
}

/// Names the page in the tab title while it is shown.
#[hook]
pub(super) fn use_page_title(title: &'static str) {
    let background = use_context::<Background>().expect("Expected background handle in context");

    use_effect_with((), move |()| {
        background.title(Some(title.to_owned()));
        move || background.title(None)
    });
}
