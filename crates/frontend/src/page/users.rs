use auth::yew023::AdminUsers;
use yew::prelude::*;

use super::account::use_page_title;
use crate::ui::PageHead;

/// Managing users, for administrators.
#[function_component]
pub(crate) fn Users() -> Html {
    use_page_title("Users");

    html! {
        <>
            <PageHead hidden=true title="Users" />

            <div class="settings wide">
                <AdminUsers />
            </div>
        </>
    }
}
