use std::rc::Rc;

use web_sys::{HtmlInputElement, InputEvent};
use yew::prelude::*;

use crate::router::Route;
use crate::ui::{Image, Link, Modal};

#[derive(Properties)]
pub(crate) struct Props {
    pub(crate) credits: Rc<Vec<api::Credit>>,
    pub(crate) on_close: Callback<()>,
}

impl PartialEq for Props {
    fn eq(&self, other: &Self) -> bool {
        Rc::ptr_eq(&self.credits, &other.credits) && self.on_close == other.on_close
    }
}

/// The subtitle under a credit's name: the character for cast, the job for crew.
pub(crate) fn credit_subtitle(credit: &api::Credit) -> Option<&str> {
    credit.role.character().or(credit.role.job())
}

/// A clickable credit card - photo, name and subtitle - that navigates to the
/// person's page.
pub(crate) fn cast_card(credit: &api::Credit) -> Html {
    cast_card_on(credit, false)
}

/// A credit card for a dialog's surface when `raised`.
pub(crate) fn cast_card_on(credit: &api::Credit, raised: bool) -> Html {
    let name = credit.name.title_or_any().unwrap_or("Unknown").to_owned();
    let subtitle = credit_subtitle(credit).map(str::to_owned);

    html! {
        <Link to={Route::PersonDetail(credit.person_id)} class={classes!("cast-card", "plinth-card", "plinth-inset", "plinth-flush", raised.then_some("raised"))}>
            <Image class="cast-photo" placeholder={true} placeholder_icon="user" src={credit.profile.clone()} alt={name.clone()} />

            <div class="cast-info">
                <div class="cast-name">{ name }</div>

                if let Some(subtitle) = subtitle {
                    <div class="cast-character">{ subtitle }</div>
                }
            </div>
        </Link>
    }
}

/// Whether every word of `query` appears in the credit's name or subtitle.
fn matches(credit: &api::Credit, words: &[String]) -> bool {
    let haystack = format!(
        "{} {}",
        credit.name.title_or_any().unwrap_or_default(),
        credit_subtitle(credit).unwrap_or_default()
    )
    .to_lowercase();

    words.iter().all(|w| haystack.contains(w.as_str()))
}

/// The full cast in a dialog, filtered by person or character name.
#[function_component]
pub(crate) fn CastModal(props: &Props) -> Html {
    let query = use_state(String::new);

    let oninput = {
        let query = query.clone();

        Callback::from(move |e: InputEvent| {
            let input: HtmlInputElement = e.target_unchecked_into();
            query.set(input.value());
        })
    };

    let words = query
        .split_whitespace()
        .map(str::to_lowercase)
        .collect::<Vec<_>>();

    let found = props
        .credits
        .iter()
        .filter(|c| matches(c, &words))
        .collect::<Vec<_>>();

    html! {
        <Modal icon="user-group" title="Cast & crew" size={plinth::DialogSize::WideTall} testid="cast-modal" on_close={props.on_close.clone()}>
            <div class="cast-search plinth-stack">
                <input
                    type="search"
                    class={plinth::class::INPUT}
                    placeholder="Search by name or character…"
                    aria-label="Search cast"
                    data-test="cast-search"
                    value={(*query).clone()}
                    {oninput}
                />
            </div>

            if found.is_empty() {
                <p class="text-muted" data-test="cast-empty">{format!("No cast matches “{}”.", query.trim())}</p>
            } else {
                <div class="cast-grid">
                    { for found.iter().map(|c| cast_card_on(c, true)) }
                </div>
            }
        </Modal>
    }
}
