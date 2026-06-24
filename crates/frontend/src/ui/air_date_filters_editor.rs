use web_sys::{Event, MouseEvent};
use yew::prelude::*;

use super::CountryPicker;

/// Enriching sources that can contribute episode air dates.
const AIR_DATE_SOURCES: &[api::RemoteSource] = &[
    api::RemoteSource::Tvmaze,
    api::RemoteSource::Tmdb,
    api::RemoteSource::Tvdb,
];

#[derive(Properties, PartialEq)]
pub(crate) struct Props {
    pub(crate) filters: Vec<api::AirDateFilter>,
    pub(crate) on_change: Callback<Vec<api::AirDateFilter>>,
}

/// Editor for a set of [`api::AirDateFilter`]s: a checkbox per source and, when
/// enabled, a [`CountryPicker`] and a network text input restricting which of
/// that source's air dates qualify. Priority between sources is the remote order.
#[function_component]
pub(crate) fn AirDateFiltersEditor(props: &Props) -> Html {
    html! {
        <div class="form">
            {
                for AIR_DATE_SOURCES.iter().copied().map(|source| {
                    let existing = props.filters.iter().find(|f| f.source == source);
                    let enabled = existing.is_some();
                    let countries = existing.map(|f| f.countries.clone()).unwrap_or_default();
                    let networks = existing.map(|f| f.networks.clone()).unwrap_or_default();

                    let on_toggle = {
                        let filters = props.filters.clone();
                        let cb = props.on_change.clone();
                        Callback::from(move |_: MouseEvent| {
                            let mut next = filters.clone();
                            if let Some(pos) = next.iter().position(|f| f.source == source) {
                                next.remove(pos);
                            } else {
                                next.push(api::AirDateFilter { source, countries: Vec::new(), networks: Vec::new() });
                            }
                            cb.emit(next);
                        })
                    };

                    let on_countries = {
                        let filters = props.filters.clone();
                        let cb = props.on_change.clone();

                        Callback::from(move |countries: Vec<api::Country>| {
                            let mut next = filters.clone();

                            if let Some(f) = next.iter_mut().find(|f| f.source == source) {
                                f.countries = countries;
                            }

                            cb.emit(next);
                        })
                    };

                    let on_networks = {
                        let filters = props.filters.clone();
                        let cb = props.on_change.clone();
                        Callback::from(move |e: Event| {
                            let input: web_sys::HtmlInputElement = e.target_unchecked_into();
                            let networks = input
                                .value()
                                .split(',')
                                .map(|s| s.trim().to_owned())
                                .filter(|s| !s.is_empty())
                                .collect::<Vec<_>>();
                            let mut next = filters.clone();
                            if let Some(f) = next.iter_mut().find(|f| f.source == source) {
                                f.networks = networks;
                            }
                            cb.emit(next);
                        })
                    };

                    html! {
                        <div class="field">
                            <div class="row input-group">
                                <span class={classes!("input-checkbox", enabled.then_some("checked"))} onclick={on_toggle}>
                                    <span class="mark" />
                                    <span class={classes!("logo", source.as_id())} title={source.as_label()} / >
                                </span>

                                if enabled {
                                    <CountryPicker current={countries} on_change={on_countries} />
                                    <input class="input-text fill" type="text" placeholder="Networks (comma separated)" value={networks.join(", ")} onchange={on_networks} />
                                }
                            </div>
                        </div>
                    }
                })
            }
        </div>
    }
}
