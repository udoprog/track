use crate::class;
use yew::prelude::*;

use sockel::{Reorder, ReorderHandle, ReorderItem};

#[derive(Properties, PartialEq)]
pub(crate) struct Props {
    pub(crate) sources: Vec<api::RemoteSource>,
    pub(crate) on_change: Callback<Vec<api::RemoteSource>>,
}

/// The configured lookup sources in their stored order, with any missing one
/// appended, so every source has a position.
fn ordered(sources: &[api::RemoteSource]) -> Vec<api::RemoteSource> {
    let mut out = sources
        .iter()
        .copied()
        .filter(|s| api::RemoteSource::XEM_LOOKUP.contains(s))
        .collect::<Vec<_>>();

    for &source in api::RemoteSource::XEM_LOOKUP {
        if !out.contains(&source) {
            out.push(source);
        }
    }

    out
}

/// The order a show's remotes are looked up on XEM in, changed by dragging a
/// row.
#[function_component]
pub(crate) fn XemLookupEditor(props: &Props) -> Html {
    let sources = ordered(&props.sources);

    let on_move = {
        let sources = sources.clone();

        props.on_change.reform(move |(from, to): (usize, usize)| {
            let mut sources = sources.clone();
            let source = sources.remove(from);
            sources.insert(to, source);
            sources
        })
    };

    html! {
        <Reorder class={classes!(class::FORM, "xem-lookup")} {on_move} quick_keys=true titles=true>
            {
                for sources.iter().map(|source| html_nested! {
                    <ReorderItem id={source.as_id()} name={source.as_label()} class={classes!(class::INPUT_GROUP, sockel::class::CLUSTER)}>
                        <ReorderHandle />

                        <span class={classes!(class::INPUT_LABEL, class::HAS_TEXT)}>
                            <span class={classes!(class::LOGO, class::logo(source.as_id()))} />
                        </span>

                        <span class={classes!(class::INPUT_LABEL, class::HAS_TEXT)}>{format!("{} id", source.as_label())}</span>
                    </ReorderItem>
                })
            }
        </Reorder>
    }
}
