use auth::yew023::AdminUsers;
use yew::prelude::*;

use super::account::use_page_title;

/// Managing users, for administrators.
#[function_component]
pub(crate) fn Users() -> Html {
    use_page_title("Users");

    html! {
        <>
            <h1 class="plinth-visually-hidden">{"Users"}</h1>

            <div class="settings wide">
                <AdminUsers />
            </div>
        </>
    }
}
