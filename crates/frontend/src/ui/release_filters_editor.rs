use web_sys::MouseEvent;
use yew::prelude::*;

use super::CountryPicker;

/// Release types that may contribute to a movie's release date (excludes `Unknown`).
const RELEASE_TYPES: &[api::ReleaseType] = &[
    api::ReleaseType::Premiere,
    api::ReleaseType::TheatricalLimited,
    api::ReleaseType::Theatrical,
    api::ReleaseType::Digital,
    api::ReleaseType::Physical,
    api::ReleaseType::Tv,
];

#[derive(Properties, PartialEq)]
pub(crate) struct Props {
    pub(crate) filters: Vec<api::ReleaseFilter>,
    pub(crate) on_change: Callback<Vec<api::ReleaseFilter>>,
}

/// Editor for a set of [`api::ReleaseFilter`]s: a checkbox per release type and,
/// when enabled, a [`CountryPicker`] restricting that type to certain countries.
#[function_component]
pub(crate) fn ReleaseFiltersEditor(props: &Props) -> Html {
    html! {
        <div class="form">
            {
                for RELEASE_TYPES.iter().copied().map(|rt| {
                    let existing = props.filters.iter().find(|f| f.release_type == rt);
                    let enabled = existing.is_some();
                    let countries = existing.map(|f| f.countries.clone()).unwrap_or_default();

                    let on_toggle = {
                        let filters = props.filters.clone();
                        let cb = props.on_change.clone();
                        Callback::from(move |_: MouseEvent| {
                            let mut next = filters.clone();
                            if let Some(pos) = next.iter().position(|f| f.release_type == rt) {
                                next.remove(pos);
                            } else {
                                next.push(api::ReleaseFilter { release_type: rt, countries: Vec::new() });
                            }
                            cb.emit(next);
                        })
                    };

                    let on_countries = {
                        let filters = props.filters.clone();
                        let cb = props.on_change.clone();
                        Callback::from(move |countries: Vec<api::Country>| {
                            let mut next = filters.clone();
                            if let Some(f) = next.iter_mut().find(|f| f.release_type == rt) {
                                f.countries = countries;
                            }
                            cb.emit(next);
                        })
                    };

                    html! {
                        <div class="field">
                            <label class="clickable" onclick={on_toggle.clone()}>{rt.as_str()}</label>
                            <div class="row input-group">
                                <span class={classes!("input-checkbox", enabled.then_some("checked"))} onclick={on_toggle}>
                                    <span class="mark" />
                                </span>

                                if enabled {
                                    <CountryPicker current={countries} on_change={on_countries} />
                                }
                            </div>
                        </div>
                    }
                })
            }
        </div>
    }
}
