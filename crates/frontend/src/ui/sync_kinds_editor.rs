use web_sys::MouseEvent;
use yew::prelude::*;

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

/// Global editor for the per-source sync-kind defaults: a checkbox per kind a
/// source can contribute. Graphics always accumulate from every source and are
/// not selectable here. Per-remote overrides live in the remote editor.
#[function_component]
pub(crate) fn SyncKindsEditor(props: &Props) -> Html {
    html! {
        <div class="form">
            {
                for SYNC_KIND_SOURCES.iter().copied().map(|source| {
                    let capability = source.default_sync_kinds();
                    let current = props
                        .kinds
                        .iter()
                        .find(|s| s.source == source)
                        .map(|s| s.kinds)
                        .unwrap_or(capability)
                        .intersect(capability);

                    html! {
                        <div class="row input-group">
                            <span class="input-label">
                                <span class={classes!("logo", source.as_id())} />
                            </span>

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
                    }
                })
            }
        </div>
    }
}
