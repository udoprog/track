//! What the Upcoming agenda and the Schedule calendar show for one day and one
//! show or movie in it.

use api::TimeInfo;
use yew::prelude::*;

use crate::router::{Route, ShowDetailQuery};
use crate::ui::Image;

/// A day's heading: its relative name when it has one ("Today"), and its date.
pub(super) fn view_day_heading(day: api::Date, today: api::Date) -> Html {
    let relative = if day == today {
        Some("Today")
    } else if Some(day) == today.checked_add_days(1) {
        Some("Tomorrow")
    } else if Some(day) == today.checked_sub_days(1) {
        Some("Yesterday")
    } else {
        None
    };

    let date = format!(
        "{} {} {}",
        day.weekday().short_name(),
        day.day(),
        day.month_name()
    );

    html! {
        <h3 class={classes!("agenda-day-title", (day == today).then_some("today"))}>
            if let Some(relative) = relative {
                <span>{relative}</span>
                <span class="text-muted">{date}</span>
            } else {
                <span>{date}</span>
            }
        </h3>
    }
}

/// One show or movie on a day: a small poster, its title, and when each of its
/// episodes airs. The title opens the show at its latest episode that day, an
/// episode opens that episode.
pub(super) fn view_schedule_item(
    item: &api::ScheduleItem,
    time: &TimeInfo,
    on_navigate: &Callback<Route>,
    on_hover: &Callback<Option<String>>,
) -> Html {
    let (key, title, poster, backdrop, route) = match item {
        api::ScheduleItem::Show(entry) => {
            let episode = entry.episodes.last().map(|ep| ep.code());

            let route = Route::ShowDetail(
                entry.show_id,
                ShowDetailQuery {
                    season: episode.map(|e| e.season).unwrap_or_default(),
                    episode,
                    orphaned: false,
                },
            );

            (
                format!("show-{}", entry.show_id),
                &entry.show_title,
                &entry.poster,
                &entry.backdrop,
                route,
            )
        }
        api::ScheduleItem::Movie(movie) => (
            format!("movie-{}", movie.movie_id),
            &movie.title,
            &movie.poster,
            &movie.backdrop,
            Route::MovieDetail(movie.movie_id),
        ),
    };

    let onclick = on_navigate.reform(move |_: MouseEvent| route.clone());
    let backdrop = backdrop.as_ref().map(|i| i.proxy_url());
    let onmouseover = on_hover.reform(move |_: MouseEvent| backdrop.clone());

    let times = match item {
        api::ScheduleItem::Show(entry) => {
            let show_id = entry.show_id;

            html! {
                { for entry.episodes.iter().map(|ep| {
                    let code = ep.code();

                    // The episode, not the show's latest one the whole entry opens.
                    let onclick = on_navigate.reform(move |e: MouseEvent| {
                        e.stop_propagation();
                        Route::ShowDetail(show_id, ShowDetailQuery { season: code.season, episode: Some(code), orphaned: false })
                    });

                    html! {
                        <span key={code.to_string()} class="schedule-time clickable" title={format!("Open {code}")} {onclick}>
                            <span>{ep.aired.time_of_day(time.clone())}</span>
                            <span class="badge">{code.to_string()}</span>
                        </span>
                    }
                }) }
            }
        }
        api::ScheduleItem::Movie(movie) => html! {
            <span class="schedule-time">
                <span>{movie.released.time_of_day(time.clone())}</span>
                <span class="badge">{"Movie"}</span>
            </span>
        },
    };

    html! {
        <div {key} class="schedule-item clickable" title={format!("Open {title}")} {onclick} {onmouseover}>
            <Image class="schedule-poster" src={poster.clone()} />
            <span class="schedule-title">{title}</span>
            <span class="schedule-times">{times}</span>
        </div>
    }
}
