use auth::yew023::AdminUsers;
use yew::prelude::*;

use super::account::use_page_title;
use crate::class;

/// Managing users, for administrators.
#[function_component]
pub(crate) fn Users() -> Html {
    use_page_title("Users");

    html! {
        <>
            <h1 class={sockel::class::VISUALLY_HIDDEN}>{"Users"}</h1>

            <div class={classes!(class::SETTINGS, class::WIDE)}>
                <AdminUsers />
            </div>
        </>
    }
}
