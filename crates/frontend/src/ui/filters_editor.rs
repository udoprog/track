use web_sys::{Event, MouseEvent};
use yew::prelude::*;

use super::CountryPicker;

/// Predicate kinds offered when editing movie release filters.
pub(crate) const RELEASE_KINDS: &[api::PredicateKind] = &[
    api::PredicateKind::Sources,
    api::PredicateKind::Countries,
    api::PredicateKind::ReleaseTypes,
];

/// Sources that can contribute movie releases (movies currently sync from TMDB).
pub(crate) const RELEASE_SOURCES: &[api::RemoteSource] = &[api::RemoteSource::Tmdb];

/// Predicate kinds offered when editing episode air-date filters.
pub(crate) const AIR_DATE_KINDS: &[api::PredicateKind] = &[
    api::PredicateKind::Sources,
    api::PredicateKind::Countries,
    api::PredicateKind::Networks,
];

/// Enriching sources that can contribute episode air dates.
pub(crate) const AIR_DATE_SOURCES: &[api::RemoteSource] = &[
    api::RemoteSource::Tvmaze,
    api::RemoteSource::Tmdb,
    api::RemoteSource::Tvdb,
];

/// Release types offered by a `ReleaseTypes` predicate (excludes `Unknown`).
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
    pub(crate) rules: Vec<api::FilterRule>,
    pub(crate) on_change: Callback<Vec<api::FilterRule>>,
    /// Predicate kinds the editor may add (release vs air-date contexts differ).
    pub(crate) kinds: &'static [api::PredicateKind],
    /// Sources offered to a `Sources` predicate.
    pub(crate) sources: &'static [api::RemoteSource],
}

/// Editor for a list of [`api::FilterRule`]s shared by release and air-date
/// filters. Rules are OR'd, the predicates within a rule are AND'd, and each
/// predicate is an OR over its set. An empty list accepts everything. Emptying a
/// predicate's set removes it (= "no constraint of this kind").
#[function_component]
pub(crate) fn FiltersEditor(props: &Props) -> Html {
    let on_add_rule = {
        let rules = props.rules.clone();
        let cb = props.on_change.clone();
        Callback::from(move |_: MouseEvent| {
            let mut next = rules.clone();
            next.push(api::FilterRule::default());
            cb.emit(next);
        })
    };

    html! {
        <div class="form">
            {for props.rules.iter().enumerate().map(|(ri, rule)| view_rule(props, ri, rule))}

            <button class="has-text" onclick={on_add_rule}>
                <span class="icon plus" />
                <span>{"Add rule"}</span>
            </button>
        </div>
    }
}

/// Replace the predicate at `(ri, pi)`, removing it when its set is empty.
fn set_predicate(
    rules: &mut [api::FilterRule],
    index: usize,
    predicate: usize,
    p: api::FilterPredicate,
) {
    if p.is_empty() {
        rules[index].predicates.remove(predicate);
    } else {
        rules[index].predicates[predicate] = p;
    }
}

fn view_rule(props: &Props, index: usize, rule: &api::FilterRule) -> Html {
    let on_remove = {
        let rules = props.rules.clone();
        let cb = props.on_change.clone();
        Callback::from(move |_: MouseEvent| {
            let mut next = rules.clone();
            next.remove(index);
            cb.emit(next);
        })
    };

    let present: Vec<api::PredicateKind> = rule.predicates.iter().map(|p| p.kind()).collect();

    let available: Vec<api::PredicateKind> = props
        .kinds
        .iter()
        .copied()
        .filter(|k| !present.contains(k))
        .collect();

    html! {
        <div class="field">
            <div class="row input-group">
                <span class="input-label has-text fill">{format!("Rule {}", index + 1)}</span>

                <button class="danger" onclick={on_remove} title="Remove rule">
                    <span class="icon x-mark" />
                </button>
            </div>

            {for rule.predicates.iter().enumerate().map(|(pi, p)| view_predicate(props, index, pi, p))}

            if !available.is_empty() {
                {view_add_predicate(props, index, available)}
            }
        </div>
    }
}

fn view_add_predicate(props: &Props, index: usize, available: Vec<api::PredicateKind>) -> Html {
    let on_add = {
        let rules = props.rules.clone();
        let cb = props.on_change.clone();
        let available = available.clone();
        Callback::from(move |e: Event| {
            let select: web_sys::HtmlSelectElement = e.target_unchecked_into();
            if let Ok(idx) = select.value().parse::<usize>()
                && let Some(kind) = available.get(idx)
            {
                let mut next = rules.clone();
                next[index].predicates.push(kind.empty());
                cb.emit(next);
            }
            select.set_value("");
        })
    };

    html! {
        <div class="row input-group">
            <select class="input-select fill" onchange={on_add}>
                <option value="" selected={true}>{"Add predicate…"}</option>

                {for available.iter().enumerate().map(|(i, kind)| html! {
                    <option value={i.to_string()}>{kind.label()}</option>
                })}
            </select>
        </div>
    }
}

fn view_predicate(
    props: &Props,
    rule_index: usize,
    predicate: usize,
    p: &api::FilterPredicate,
) -> Html {
    let on_remove = {
        let rules = props.rules.clone();
        let cb = props.on_change.clone();

        Callback::from(move |_: MouseEvent| {
            let mut next = rules.clone();
            next[rule_index].predicates.remove(predicate);
            cb.emit(next);
        })
    };

    html! {
        <div class="row input-group">
            <span class="input-label has-text">{p.kind().label()}</span>

            {view_selector(props, rule_index, predicate, p)}

            <button class="danger" onclick={on_remove} title="Remove predicate">
                <span class="icon x-mark" />
            </button>
        </div>
    }
}

fn view_selector(props: &Props, index: usize, predicate: usize, p: &api::FilterPredicate) -> Html {
    match p {
        api::FilterPredicate::Sources(selected) => html! {
            <>
                {for props.sources.iter().copied().map(|source| {
                    let checked = selected.contains(&source);
                    let selected = selected.clone();

                    let on_toggle = {
                        let rules = props.rules.clone();
                        let cb = props.on_change.clone();

                        Callback::from(move |_: MouseEvent| {
                            let mut set = selected.clone();
                            if let Some(pos) = set.iter().position(|s| *s == source) {
                                set.remove(pos);
                            } else {
                                set.push(source);
                            }
                            let mut next = rules.clone();
                            set_predicate(&mut next, index, predicate, api::FilterPredicate::Sources(set));
                            cb.emit(next);
                        })
                    };

                    html! {
                        <span class={classes!("input-checkbox", "has-text", checked.then_some("checked"))} onclick={on_toggle}>
                            <span class="mark" />
                            <span class={classes!("logo", source.as_id())} title={source.as_label()} />
                        </span>
                    }
                })}
            </>
        },
        api::FilterPredicate::ReleaseTypes(selected) => html! {
            <>
                {for RELEASE_TYPES.iter().copied().map(|rt| {
                    let checked = selected.contains(&rt);
                    let selected = selected.clone();

                    let on_toggle = {
                        let rules = props.rules.clone();
                        let cb = props.on_change.clone();

                        Callback::from(move |_: MouseEvent| {
                            let mut set = selected.clone();
                            if let Some(pos) = set.iter().position(|t| *t == rt) {
                                set.remove(pos);
                            } else {
                                set.push(rt);
                            }
                            let mut next = rules.clone();
                            set_predicate(&mut next, index, predicate, api::FilterPredicate::ReleaseTypes(set));
                            cb.emit(next);
                        })
                    };

                    html! {
                        <span class={classes!("input-checkbox", "has-text", checked.then_some("checked"))} onclick={on_toggle}>
                            <span class="mark" />
                            <span>{rt.as_str()}</span>
                        </span>
                    }
                })}
            </>
        },
        api::FilterPredicate::Countries(selected) => {
            let on_change = {
                let rules = props.rules.clone();
                let cb = props.on_change.clone();

                Callback::from(move |countries: Vec<api::Country>| {
                    let mut next = rules.clone();
                    set_predicate(
                        &mut next,
                        index,
                        predicate,
                        api::FilterPredicate::Countries(countries),
                    );
                    cb.emit(next);
                })
            };

            html! { <CountryPicker current={selected.clone()} on_change={on_change} /> }
        }
        api::FilterPredicate::Networks(selected) => {
            let on_change = {
                let rules = props.rules.clone();
                let cb = props.on_change.clone();

                Callback::from(move |e: Event| {
                    let input: web_sys::HtmlInputElement = e.target_unchecked_into();

                    let networks = input
                        .value()
                        .split(',')
                        .map(|s| s.trim().to_owned())
                        .filter(|s| !s.is_empty())
                        .collect::<Vec<_>>();

                    let mut next = rules.clone();
                    set_predicate(
                        &mut next,
                        index,
                        predicate,
                        api::FilterPredicate::Networks(networks),
                    );
                    cb.emit(next);
                })
            };

            html! {
                <input class="input-text fill" type="text" placeholder="Networks (comma separated)" value={selected.join(", ")} onchange={on_change} />
            }
        }
    }
}
