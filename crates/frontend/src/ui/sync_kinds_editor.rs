use web_sys::MouseEvent;
use yew::prelude::*;

use super::Button;

/// Sources that can contribute syncable data, with the kinds they support fixed
/// by [`api::RemoteSource::default_sync_kinds`].
const SYNC_KIND_SOURCES: &[api::RemoteSource] = &[
    api::RemoteSource::Tmdb,
    api::RemoteSource::Tvdb,
    api::RemoteSource::Tvmaze,
];

#[derive(Properties, PartialEq)]
pub(crate) struct Props {
    pub(crate) kinds: Vec<api::SourceSyncKinds>,
    pub(crate) on_change: Callback<Vec<api::SourceSyncKinds>>,
}

/// The configured entries in their stored order, with any source missing from
/// the configuration appended (using its full capability). This gives every
/// source a position so the order is a complete priority and reordering covers
/// all of them.
fn ordered_entries(kinds: &[api::SourceSyncKinds]) -> Vec<api::SourceSyncKinds> {
    let mut out = kinds.to_vec();

    for &source in SYNC_KIND_SOURCES {
        if !out.iter().any(|e| e.source == source) {
            out.push(api::SourceSyncKinds {
                source,
                kinds: source.default_sync_kinds(),
            });
        }
    }

    out
}

/// Global editor for the per-source sync-kind defaults and their priority: each
/// source is a row with a checkbox per kind it can contribute, and the row order
/// (changed with the up/down controls) is the default source priority used during
/// sync. Graphics always accumulate from every source and are not selectable here.
/// Per-remote overrides live in the remote editor; per-show remote order overrides
/// this default.
#[function_component]
pub(crate) fn SyncKindsEditor(props: &Props) -> Html {
    let entries = ordered_entries(&props.kinds);
    let count = entries.len();

    html! {
        <div class="form">
            {
                for entries.iter().enumerate().map(|(index, entry)| {
                    let source = entry.source;
                    let capability = source.default_sync_kinds();
                    let current = entry.kinds.intersect(capability);

                    let on_move = |delta: isize| {
                        let entries = entries.clone();

                        if let Some(target) = index.checked_add_signed(delta) && target < entries.len() {
                            props.on_change.reform(move |_| {
                                let mut entries = entries.clone();
                                entries.swap(index, target);
                                entries
                            })
                        } else {
                            Callback::noop()
                        }
                    };

                    html! {
                        <div class="row input-group">
                            <span class="input-label has-text">
                                <span class={classes!("logo", source.as_id())} />
                            </span>

                            { for capability.iter().map(|kind| {
                                let on = current.contains(kind);
                                let next_kinds = current.with(kind, !on);

                                let on_toggle = {
                                    let entries = entries.clone();
                                    let cb = props.on_change.clone();
                                    Callback::from(move |_: MouseEvent| {
                                        let mut next = entries.clone();
                                        next[index].kinds = next_kinds;
                                        cb.emit(next);
                                    })
                                };

                                html! {
                                    <span class={classes!("input-checkbox", "has-text", on.then_some("checked"))} onclick={on_toggle} title={kind.as_label()}>
                                        <span class="mark" />
                                        <span>{kind.as_label()}</span>
                                    </span>
                                }
                            }) }

                            <Button icon="chevron-up" title="Higher priority" disabled={index == 0} onclick={on_move(-1)} />
                            <Button icon="chevron-down" title="Lower priority" disabled={index + 1 == count} onclick={on_move(1)} />
                        </div>
                    }
                })
            }
        </div>
    }
}
