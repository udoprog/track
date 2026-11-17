//! What the Upcoming agenda and the Schedule calendar show for one day and one
//! show or movie in it.

use api::TimeInfo;
use yew::prelude::*;

use crate::router::{Route, ShowDetailQuery};
use crate::ui::{Button, Image, Link};

/// What the controls line above a schedule view shows and does; shared by the
/// week calendar and the day agenda so the two stay in step.
pub(super) struct RangeControls<'a> {
    /// The span shown, as text.
    pub(super) label: String,
    pub(super) prev_title: &'a str,
    pub(super) next_title: &'a str,
    /// The button that returns to the present, and whether it is a no-op now.
    pub(super) reset_label: &'a str,
    pub(super) reset_title: &'a str,
    pub(super) reset_disabled: bool,
    pub(super) options_anchor: &'a NodeRef,
    pub(super) options_open: bool,
    pub(super) on_prev: Callback<MouseEvent>,
    pub(super) on_next: Callback<MouseEvent>,
    pub(super) on_reset: Callback<MouseEvent>,
    pub(super) on_options: Callback<MouseEvent>,
}

/// The line above a schedule view: the span shown, then paging and options.
pub(super) fn view_range_controls(c: RangeControls<'_>) -> Html {
    html! {
        <div class="page-controls">
            <span class="text-muted">{c.label}</span>

            <div class="sockel-cluster sockel-nowrap sockel-loose">
                <Button icon="chevron-left" title={c.prev_title.to_string()} onclick={c.on_prev} />
                <Button icon="calendar" label={c.reset_label.to_string()} title={c.reset_title.to_string()} class="chip" disabled={c.reset_disabled} onclick={c.on_reset} />
                <Button icon="chevron-right" title={c.next_title.to_string()} onclick={c.on_next} />
                <Button node_ref={c.options_anchor.clone()} icon="adjustments-horizontal" title="View options" class={classes!("chip", c.options_open.then_some("selected"))} expanded={Some(c.options_open)} haspopup="dialog" onclick={c.on_options} />
            </div>
        </div>
    }
}

/// A labelled minus / count / plus field for the options menu.
pub(super) fn view_stepper(
    label: &str,
    fewer_title: &str,
    more_title: &str,
    count: u32,
    on_fewer: Callback<MouseEvent>,
    on_more: Callback<MouseEvent>,
) -> Html {
    html! {
        <div class="field">
            <label>{label.to_string()}</label>

            <div class="sockel-cluster sockel-nowrap sockel-loose">
                <Button icon="minus" title={fewer_title.to_string()} onclick={on_fewer} disabled={count <= 1} />
                <span class="page-size sockel-numeric">{count}</span>
                <Button icon="plus" title={more_title.to_string()} onclick={on_more} />
            </div>
        </div>
    }
}

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
        <h3 class={"agenda-day-title"}>
            if let Some(relative) = relative {
                <span class={(day == today).then_some("accent")}>{relative}</span>
                <span class="text-muted">{date}</span>
            } else {
                <span class={(day == today).then_some("accent")}>{date}</span>
            }
        </h3>
    }
}

/// One show or movie on a day: a small poster, its title, and when each of its
/// episodes airs. The title opens the show at its latest episode that day, an
/// episode opens that episode. `compact` is the stacked form for a narrow
/// calendar tile on wide screens (no poster); on phones it changes nothing.
pub(super) fn view_schedule_item(
    item: &api::ScheduleItem,
    time: &TimeInfo,
    on_hover: &Callback<Option<String>>,
    compact: bool,
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
                        <Link key={code.to_string()} {to} class={classes!("schedule-time", sockel::class::NUMERIC, compact.then_some("compact"))} title={format!("Open {code}")}>
                            <span>{ep.aired.time_of_day(time.clone())}</span>
                            <span class="badge">{code.to_string()}</span>
                        </Link>
                    }
                }) }
            }
        }
        api::ScheduleItem::Movie(movie) => html! {
            <span class={classes!("schedule-time", sockel::class::NUMERIC, compact.then_some("compact"))}>
                <span>{movie.released.time_of_day(time.clone())}</span>
                <span class="badge">{"Movie"}</span>
            </span>
        },
    };

    html! {
        <div {key} class={classes!("schedule-item", compact.then_some("compact"))} {onmouseover}>
            <Link to={route.clone()} class={classes!("schedule-poster", compact.then_some("compact"))} decorative=true>
                <Image src={poster.clone()} />
            </Link>
            <Link to={route} class={classes!("schedule-title", compact.then_some("compact"))} title={format!("Open {title}")}><>{title}</></Link>
            <span class={classes!("schedule-times", compact.then_some("compact"))}>{times}</span>
        </div>
    }
}
