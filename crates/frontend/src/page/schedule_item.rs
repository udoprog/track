//! What the Upcoming agenda and the Schedule calendar show for one day and one
//! show or movie in it.

use api::TimeInfo;
use yew::prelude::*;

use crate::router::{Route, ShowDetailQuery};
use crate::ui::{Image, Link};

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

    let backdrop = backdrop.as_ref().map(|i| i.proxy_url());
    let onmouseover = on_hover.reform(move |_: MouseEvent| backdrop.clone());

    let times = match item {
        api::ScheduleItem::Show(entry) => {
            let show_id = entry.show_id;

            html! {
                { for entry.episodes.iter().map(|ep| {
                    let code = ep.code();

                    // The episode, not the show's latest one the title opens.
                    let to = Route::ShowDetail(show_id, ShowDetailQuery { season: code.season, episode: Some(code), orphaned: false });

                    html! {
                        <Link key={code.to_string()} {to} class="schedule-time" title={format!("Open {code}")}>
                            <span>{ep.aired.time_of_day(time.clone())}</span>
                            <span class="badge">{code.to_string()}</span>
                        </Link>
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
        <div {key} class="schedule-item" {onmouseover}>
            <Link to={route.clone()} class="schedule-poster" decorative=true>
                <Image src={poster.clone()} />
            </Link>
            <Link to={route} class="schedule-title" title={format!("Open {title}")}><>{title}</></Link>
            <span class="schedule-times">{times}</span>
        </div>
    }
}
