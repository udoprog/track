use web_sys::{Event, MouseEvent};
use yew::prelude::*;

use super::{Button, ConfirmDanger, ContextMenu, CountryPicker, DragHandle, Reorder, Variant};

/// Predicate kinds offered when editing movie release filters.
pub(crate) const RELEASE_KINDS: &[api::PredicateKind] = &[
    api::PredicateKind::Sources,
    api::PredicateKind::Countries,
    api::PredicateKind::ReleaseTypes,
];

/// Sources that can contribute movie releases (movies currently sync from TMDB).
pub(crate) const RELEASE_SOURCES: &[api::RemoteSource] = &[
    api::RemoteSource::Tvmaze,
    api::RemoteSource::Tmdb,
    api::RemoteSource::Tvdb,
];

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
    pub(crate) rules: api::FilterRules,
    pub(crate) on_change: Callback<api::FilterRules>,
    /// Predicate kinds the editor may add (release vs air-date contexts differ).
    pub(crate) kinds: &'static [api::PredicateKind],
    /// Sources offered to a `Sources` predicate.
    pub(crate) sources: &'static [api::RemoteSource],
}

#[derive(Clone, Copy, PartialEq)]
enum Mode {
    /// Read-only: rules are shown as a human-readable summary. The only per-rule
    /// action is removal (confirmed via a context menu).
    View,
    /// Full editing: names, predicate selectors, reordering and removal.
    Edit,
}

pub(crate) enum Msg {
    Edit,
    Save,
    AddRule,
    AskRemove(usize),
    CancelRemove,
    ConfirmRemove(usize),
}

/// Editor for the [`api::FilterRules`] shared by release and air-date filters.
/// Rules are AND'd, the predicates within a rule are AND'd, and each predicate is
/// an OR over its set. An empty list accepts nothing. Emptying a predicate's set
/// removes it (= "no constraint of this kind").
///
/// The rule *order* is purely informational here (rules are OR'd); the editor
/// only lets it be changed for presentation.
pub(crate) struct FiltersEditor {
    mode: Mode,
    /// The rule whose removal confirmation popover is open, if any.
    confirming_remove: Option<usize>,
    /// One anchor per rule, positioning that rule's removal popover.
    anchors: Vec<NodeRef>,
}

impl FiltersEditor {
    fn sync_anchors(&mut self, len: usize) {
        if self.anchors.len() != len {
            self.anchors.resize_with(len, NodeRef::default);
        }
    }
}

impl Component for FiltersEditor {
    type Message = Msg;
    type Properties = Props;

    fn create(ctx: &Context<Self>) -> Self {
        let len = ctx.props().rules.len();

        Self {
            mode: Mode::View,
            confirming_remove: None,
            anchors: (0..len).map(|_| NodeRef::default()).collect(),
        }
    }

    fn changed(&mut self, ctx: &Context<Self>, _old: &Props) -> bool {
        let len = ctx.props().rules.len();
        self.sync_anchors(len);

        if self.confirming_remove.is_some_and(|i| i >= len) {
            self.confirming_remove = None;
        }

        true
    }

    fn update(&mut self, ctx: &Context<Self>, msg: Msg) -> bool {
        match msg {
            Msg::Edit => {
                self.mode = Mode::Edit;
                true
            }
            Msg::Save => {
                self.mode = Mode::View;
                self.confirming_remove = None;
                true
            }
            Msg::AddRule => {
                let mut next = ctx.props().rules.clone();
                next.push(api::FilterRule::default());
                ctx.props().on_change.emit(next);
                self.mode = Mode::Edit;
                true
            }
            Msg::AskRemove(index) => {
                self.confirming_remove = Some(index);
                true
            }
            Msg::CancelRemove => {
                self.confirming_remove = None;
                true
            }
            Msg::ConfirmRemove(index) => {
                let mut next = ctx.props().rules.clone();

                if index < next.len() {
                    next.remove(index);
                }

                ctx.props().on_change.emit(next);
                self.confirming_remove = None;
                true
            }
        }
    }

    fn view(&self, ctx: &Context<Self>) -> Html {
        let link = ctx.link();
        let props = ctx.props();

        let rules = props
            .rules
            .iter()
            .enumerate()
            .map(|(index, rule)| match self.mode {
                Mode::View => self.view_summary(ctx, index, rule),
                Mode::Edit => self.view_rule(props, index, rule),
            });

        let toggle = match self.mode {
            Mode::View => html! {
                <Button icon="pencil-square" label="Edit rules" title="Edit rules" onclick={link.callback(|_| Msg::Edit)} />
            },
            Mode::Edit => html! {
                <Button icon="check" label="Save rules" title="Save rules" variant={Variant::Primary} onclick={link.callback(|_| Msg::Save)} />
            },
        };

        let on_move = {
            let rules = props.rules.clone();

            props.on_change.reform(move |(from, to): (usize, usize)| {
                let mut next = rules.clone();
                let rule = next.remove(from);
                next.insert(to, rule);
                next
            })
        };

        html! {
            <div class="form">
                if let Mode::Edit = self.mode {
                    <Reorder class="form" {on_move}>
                        {for rules}
                    </Reorder>
                } else {
                    {for rules}
                }

                <div class="row">
                    {toggle}
                    <Button icon="plus" label="Add rule" title="Add rule" onclick={link.callback(|_| Msg::AddRule)} />
                </div>
            </div>
        }
    }
}

impl FiltersEditor {
    /// A rule rendered read-only in view mode: its name (or a positional
    /// fallback) and a human-readable line per predicate, plus a remove control
    /// that opens a confirmation popover.
    fn view_summary(&self, ctx: &Context<Self>, index: usize, rule: &api::FilterRule) -> Html {
        let link = ctx.link();
        let anchor = self.anchors[index].clone();
        let name = rule_label(rule, index);

        let summaries = rule.predicates.iter().map(|p| {
            html! {
                <div class="row">
                    <span class="input-label has-text">{p.kind().label()}</span>
                    {self.predicate_summary(p)}
                </div>
            }
        });

        html! {
            <rule>
                <div class="row-split">
                    <span class="has-text">{name.clone()}</span>

                    <div ref={anchor.clone()} class="input-group">
                        <Button
                            icon="trash"
                            variant={Variant::Danger}
                            title="Remove rule"
                            expanded={Some(self.confirming_remove == Some(index))}
                            haspopup="dialog"
                            onclick={link.callback(move |_| Msg::AskRemove(index))}
                        />
                    </div>

                    if self.confirming_remove == Some(index) {
                        <ContextMenu prompt="Remove" label={name} anchor={anchor} on_close={link.callback(|_| Msg::CancelRemove)}>
                            <ConfirmDanger
                                on_confirm={link.callback(move |_| Msg::ConfirmRemove(index))}
                                on_cancel={link.callback(|_| Msg::CancelRemove)}
                            />
                        </ContextMenu>
                    }
                </div>

                if rule.predicates.is_empty() {
                    <span class="has-text">{"Matches everything"}</span>
                } else {
                    {for summaries}
                }
            </rule>
        }
    }

    /// A comma-separated, human-readable rendering of a predicate's set.
    fn predicate_summary(&self, p: &api::FilterPredicate) -> Html {
        match p {
            api::FilterPredicate::Sources(v) => {
                html! {
                    {for v.iter().map(|s| {
                        html! {
                            <span class="item-inline">
                                <span class={classes!("logo", s.as_id())} title={s.as_label()} />
                            </span>
                        }
                    })}
                }
            }
            api::FilterPredicate::Countries(v) => {
                if v.is_empty() {
                    html! {
                        <span class="input-text has-text">
                            <span class="icon globe-alt" />
                            <span>{"All countries"}</span>
                        </span>
                    }
                } else {
                    html! {
                        {for v.iter().map(|s| {
                            if let Some(s) = s.to_iso() && s.has_flag {
                                html! {
                                    <span class={classes!("item-inline", "flag", s.alpha2)} title={s.name} />
                                }
                            } else {
                                html! {
                                    <span class="item-inline">{s.to_string()}</span>
                                }
                            }
                        })}
                    }
                }
            }
            api::FilterPredicate::ReleaseTypes(v) => {
                html!({ v.iter().map(|t| t.as_str()).collect::<Vec<_>>().join(", ") })
            }
            api::FilterPredicate::Networks(v) => html!({ v.join(", ") }),
        }
    }

    /// A rule rendered in edit mode: a drag handle, a name field, a direct remove
    /// control, and the full predicate editors.
    fn view_rule(&self, props: &Props, index: usize, rule: &api::FilterRule) -> Html {
        let on_name = {
            let rules = props.rules.clone();
            let cb = props.on_change.clone();
            Callback::from(move |e: Event| {
                let input: web_sys::HtmlInputElement = e.target_unchecked_into();
                let mut next = rules.clone();
                if let Some(r) = next.get_mut(index) {
                    r.name = input.value();
                }
                cb.emit(next);
            })
        };

        let on_remove = {
            let rules = props.rules.clone();
            let cb = props.on_change.clone();
            Callback::from(move |_: MouseEvent| {
                let mut next = rules.clone();
                next.remove(index);
                cb.emit(next);
            })
        };

        let available: Vec<api::PredicateKind> = props
            .kinds
            .iter()
            .copied()
            .filter(|k| !rule.predicates.iter().any(|p| p.kind() == *k))
            .collect();

        html! {
            <rule>
                <div class="row-split">
                    <DragHandle {index} />

                    <input
                        class="input-text fill"
                        type="text"
                        placeholder={format!("Rule {}", index + 1)}
                        value={rule.name.clone()}
                        onchange={on_name}
                    />

                    <div class="input-group">
                        <Button icon="trash" variant={Variant::Danger} title="Remove rule" onclick={on_remove} />
                    </div>
                </div>

                {for rule.predicates.iter().enumerate().map(|(pi, p)| self.view_predicate(props, index, pi, p))}

                if !available.is_empty() {
                    { self.view_add_predicate(props, index, available) }
                }
            </rule>
        }
    }

    fn view_add_predicate(
        &self,
        props: &Props,
        index: usize,
        available: Vec<api::PredicateKind>,
    ) -> Html {
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
        &self,
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
            <input-label-controls>
                <span class="input-group">
                    <span class="input-label has-text fill">{p.kind().label()}</span>

                    <Button icon="trash" title="Remove predicate" variant={Variant::Danger} onclick={on_remove} />
                </span>

                <controls>
                    { self.view_selector(props, rule_index, predicate, p) }
                </controls>
            </input-label-controls>
        }
    }

    fn view_selector(
        &self,
        props: &Props,
        index: usize,
        predicate: usize,
        p: &api::FilterPredicate,
    ) -> Html {
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
                            <Button class={classes!("input-checkbox", "has-text", checked.then_some("checked"))} role="switch" checked={Some(checked)} title={source.as_label()} onclick={on_toggle}>
                                <span class="mark" />
                                <span class={classes!("logo", source.as_id())} />
                            </Button>
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
                            <Button class={classes!("input-checkbox", "has-text", checked.then_some("checked"))} role="switch" checked={Some(checked)} title={rt.as_str()} onclick={on_toggle}>
                                <span class="mark" />
                                <span>{rt.as_str()}</span>
                            </Button>
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
}

/// The rule's name, or `Rule N` when it has none.
fn rule_label(rule: &api::FilterRule, index: usize) -> String {
    if rule.name.trim().is_empty() {
        format!("Rule {}", index + 1)
    } else {
        rule.name.clone()
    }
}

/// Replace the predicate at `(ri, pi)`, removing it when its set is empty.
fn set_predicate(
    rules: &mut [api::FilterRule],
    index: usize,
    predicate: usize,
    p: api::FilterPredicate,
) {
    if let Some(rule) = rules.get_mut(index)
        && let Some(o) = rule.predicates.get_mut(predicate)
    {
        *o = p;
    }
}
