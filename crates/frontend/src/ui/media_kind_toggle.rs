use yew::prelude::*;

use crate::router::MediaSelection;

#[derive(Properties, PartialEq)]
pub(crate) struct Props {
    pub(crate) selection: MediaSelection,
    pub(crate) on_change: Callback<MediaSelection>,
}

/// The shows/movies checkbox pair shared by the media list and search. Renders
/// as two `input-checkbox` spans (no wrapper) so it drops into an existing
/// `input-group`.
#[function_component]
pub(crate) fn MediaKindToggle(props: &Props) -> Html {
    let selection = props.selection;

    let on_shows = props.on_change.reform(move |_| MediaSelection {
        shows: !selection.shows,
        ..selection
    });

    let on_movies = props.on_change.reform(move |_| MediaSelection {
        movies: !selection.movies,
        ..selection
    });

    html! {
        <>
            <span
                class={classes!("input-checkbox", "fill", selection.shows.then_some("checked"))}
                title="Show series"
                onclick={on_shows}>
                <span class="mark" />
                <span class="icon tv" />
                <span class="hide-desktop">{"Shows"}</span>
            </span>

            <span
                class={classes!("input-checkbox", "fill", selection.movies.then_some("checked"))}
                title="Show movies"
                onclick={on_movies}>
                <span class="mark" />
                <span class="icon film" />
                <span class="hide-desktop">{"Movies"}</span>
            </span>
        </>
    }
}
