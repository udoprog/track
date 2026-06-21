use web_sys::{Event, MouseEvent};
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
pub(crate) struct ReleaseFiltersEditorProps {
    pub(crate) filters: Vec<api::ReleaseFilter>,
    pub(crate) on_change: Callback<Vec<api::ReleaseFilter>>,
}

/// Editor for a set of [`api::ReleaseFilter`]s: a checkbox per release type and,
/// when enabled, a [`CountryPicker`] restricting that type to certain countries.
#[function_component]
pub(crate) fn ReleaseFiltersEditor(props: &ReleaseFiltersEditorProps) -> Html {
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

/// Enriching sources that can contribute episode air dates.
const AIR_DATE_SOURCES: &[(api::RemoteSource, &str)] = &[
    (api::RemoteSource::Tvmaze, "TVmaze"),
    (api::RemoteSource::Tmdb, "TMDB"),
    (api::RemoteSource::Tvdb, "TVDB"),
];

#[derive(Properties, PartialEq)]
pub(crate) struct AirDateFiltersEditorProps {
    pub(crate) filters: Vec<api::AirDateFilter>,
    pub(crate) on_change: Callback<Vec<api::AirDateFilter>>,
}

/// Editor for a set of [`api::AirDateFilter`]s: a checkbox per source and, when
/// enabled, a [`CountryPicker`] and a network text input restricting which of
/// that source's air dates qualify. Priority between sources is the remote order.
#[function_component]
pub(crate) fn AirDateFiltersEditor(props: &AirDateFiltersEditorProps) -> Html {
    html! {
        <div class="form">
            {
                for AIR_DATE_SOURCES.iter().copied().map(|(source, label)| {
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
                            <label class="clickable" onclick={on_toggle.clone()}>{label}</label>
                            <div class="row input-group">
                                <span class={classes!("input-checkbox", enabled.then_some("checked"))} onclick={on_toggle}>
                                    <span class="mark" />
                                </span>

                                if enabled {
                                    <CountryPicker current={countries} on_change={on_countries} />
                                    <input
                                        type="text"
                                        class="input-text fill"
                                        placeholder="Networks (comma separated)"
                                        value={networks.join(", ")}
                                        onchange={on_networks}
                                    />
                                }
                            </div>
                        </div>
                    }
                })
            }
        </div>
    }
}

/// Sources that can contribute syncable data, with the kinds they support fixed
/// by [`api::RemoteSource::default_sync_kinds`].
const SYNC_KIND_SOURCES: &[(api::RemoteSource, &str)] = &[
    (api::RemoteSource::Tmdb, "TMDB"),
    (api::RemoteSource::Tvdb, "TVDB"),
    (api::RemoteSource::Tvmaze, "TVmaze"),
];

#[derive(Properties, PartialEq)]
pub(crate) struct SyncKindsEditorProps {
    pub(crate) kinds: Vec<api::SourceSyncKinds>,
    pub(crate) on_change: Callback<Vec<api::SourceSyncKinds>>,
}

/// Global editor for the per-source sync-kind defaults: a checkbox per kind a
/// source can contribute. Graphics always accumulate from every source and are
/// not selectable here. Per-remote overrides live in the remote editor.
#[function_component]
pub(crate) fn SyncKindsEditor(props: &SyncKindsEditorProps) -> Html {
    html! {
        <div class="form">
            {
                for SYNC_KIND_SOURCES.iter().copied().map(|(source, label)| {
                    let capability = source.default_sync_kinds();
                    let current = props
                        .kinds
                        .iter()
                        .find(|s| s.source == source)
                        .map(|s| s.kinds)
                        .unwrap_or(capability)
                        .intersect(capability);

                    html! {
                        <div class="field">
                            <label>{label}</label>
                            <div class="row input-group">
                                { for capability.iter().map(|kind| {
                                    let on = current.contains(kind);
                                    let next_kinds = current.with(kind, !on);

                                    let on_toggle = {
                                        let all = props.kinds.clone();
                                        let cb = props.on_change.clone();
                                        Callback::from(move |_: MouseEvent| {
                                            let mut next = all.clone();
                                            if let Some(e) = next.iter_mut().find(|s| s.source == source) {
                                                e.kinds = next_kinds;
                                            } else {
                                                next.push(api::SourceSyncKinds { source, kinds: next_kinds });
                                            }
                                            cb.emit(next);
                                        })
                                    };

                                    html! {
                                        <span
                                            class={classes!("input-checkbox", on.then_some("checked"))}
                                            onclick={on_toggle}
                                            title={kind.as_label()}
                                        >
                                            <span class="mark" />
                                            <span>{kind.as_label()}</span>
                                        </span>
                                    }
                                }) }
                            </div>
                        </div>
                    }
                })
            }
        </div>
    }
}
