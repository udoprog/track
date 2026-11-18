use yew::prelude::*;

use crate::class;
use crate::router::MediaSelection;
use crate::ui::Button;

#[derive(Properties, PartialEq)]
pub(crate) struct Props {
    pub(crate) selection: MediaSelection,
    pub(crate) on_change: Callback<MediaSelection>,
}

/// The shows/movies filter chips shared by the dashboard, the media list and
/// search. Renders two chips (no wrapper) so it drops into an existing `.chips`
/// row.
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
            <Button icon={class::ICON_TV} label="Shows" title={if selection.shows { "Showing series" } else { "Hiding series" }} class={classes!(class::CHIP, selection.shows.then_some(class::SELECTED))} pressed={Some(selection.shows)} onclick={on_shows} />
            <Button icon={class::ICON_FILM} label="Movies" title={if selection.movies { "Showing movies" } else { "Hiding movies" }} class={classes!(class::CHIP, selection.movies.then_some(class::SELECTED))} pressed={Some(selection.movies)} onclick={on_movies} />
        </>
    }
}
