use core::array;
use core::fmt;
use std::collections::HashMap;

use api::TimeInfo;
use musli_web::web03::prelude::*;
use yew::prelude::*;

use crate::SetupChannel;
use crate::background::Background;
use crate::error::{CustomContext, Error, Message};
use crate::router::{Route, Router, ShowDetailQuery};
use crate::ui::{Button, DOT, Skeleton};

#[derive(Properties, PartialEq)]
pub(crate) struct Props {
    /// Number of weeks shown in the grid (always at least 1).
    pub(crate) weeks: u32,
    /// Offset of the visible window from the current week, in weeks (negative
    /// reaches into the past). Stored in the URL by the parent.
    pub(crate) week_offset: i32,
    /// Mobile-only: reveal the past days of the current week. Stored in the URL
    /// by the parent.
    pub(crate) week_start: bool,
    /// Adjust the persisted week count by the given signed delta.
    pub(crate) on_adjust_weeks: Callback<i32>,
    /// Navigate to the given week offset (the parent persists it in the URL).
    pub(crate) on_set_week: Callback<i32>,
    /// Set whether the start of the week is revealed (parent persists it in the
    /// URL).
    pub(crate) on_set_week_start: Callback<bool>,
    /// Reset the visible window back to the current week (parent clears the
    /// offset and week-start reveal in the URL).
    pub(crate) on_reset: Callback<()>,
}

pub(crate) struct Calendar {
    channel: ws::Channel,
    schedule: Vec<api::ScheduledDay>,
    loading: bool,
    time: TimeInfo,
    _time_handle: ContextHandle<TimeInfo>,
    router: Router,
    background: Background,
    _setup: SetupChannel,
    _broadcast: ws::Listener,
    _schedule_req: ws::Request,
}

pub(crate) enum Msg {
    Channel(Result<ws::Channel, ws::Error>),
    AppBroadcast(Result<ws::Packet<api::AppBroadcast>, ws::Error>),
    ScheduleLoaded(Result<ws::Packet<api::ListSchedule>, ws::Error>),
    Navigate(Route),
    SetTime(TimeInfo),
}

impl Component for Calendar {
    type Message = Msg;
    type Properties = Props;

    fn create(ctx: &Context<Self>) -> Self {
        let (ws, _) = ctx
            .link()
            .context::<ws::Handle>(Callback::noop())
            .expect("Expected ws::Handle in context");

        let _setup = SetupChannel::new(ws.clone(), ctx.link().callback(Msg::Channel));
        let _broadcast = ws.on_broadcast(ctx.link().callback(Msg::AppBroadcast));

        let (time, _time_handle) = ctx
            .link()
            .context::<TimeInfo>(ctx.link().callback(Msg::SetTime))
            .expect("Expected a configured time zone");

        let (router, _) = ctx
            .link()
            .context::<Router>(Callback::noop())
            .expect("Expected router in context");

        let (background, _) = ctx
            .link()
            .context::<Background>(Callback::noop())
            .expect("Expected Background in context");

        Self {
            channel: ws::Channel::default(),
            schedule: Vec::new(),
            loading: false,
            time,
            _time_handle,
            router,
            background,
            _setup,
            _broadcast,
            _schedule_req: ws::Request::default(),
        }
    }

    fn changed(&mut self, ctx: &Context<Self>, old: &Props) -> bool {
        let props = ctx.props();

        // Refetch only when the visible window actually changed (week count or
        // offset), not on every incidental prop/callback change.
        if (props.weeks, props.week_offset) != (old.weeks, old.week_offset)
            && self.channel.id() != ws::ChannelId::NONE
        {
            self.load_schedule(ctx, true);
        }

        true
    }

    fn update(&mut self, ctx: &Context<Self>, msg: Self::Message) -> bool {
        match self.try_update(ctx, msg) {
            Ok(render) => render,
            Err(e) => {
                self.background.error(e);
                false
            }
        }
    }

    fn view(&self, ctx: &Context<Self>) -> Html {
        let today = api::Date::today();
        let week_count = ctx.props().weeks.max(1);
        let week_offset = ctx.props().week_offset;
        let week_start = ctx.props().week_start;
        let window_start = window_start(today, week_offset);
        let weeks = build_weeks(window_start, week_count);

        // Past days are hidden on mobile by default; reveal them when the start
        // of the current week is toggled or when navigating into the past.
        let show_past = week_start || week_offset < 0;
        // The reveal toggle only makes sense on the current week when there are
        // hidden past days (i.e. today is not the start of the week).
        let can_reveal_week_start = week_offset == 0 && today.weekday() != api::Weekday::Monday;

        let loading = self.loading;

        let schedule_lookup: HashMap<api::Date, &api::ScheduledDay> =
            self.schedule.iter().map(|d| (d.date, d)).collect();

        let link = ctx.link();

        let on_fewer = {
            let cb = ctx.props().on_adjust_weeks.clone();
            Callback::from(move |_| cb.emit(-1))
        };
        let on_more = {
            let cb = ctx.props().on_adjust_weeks.clone();
            Callback::from(move |_| cb.emit(1))
        };
        let on_prev = {
            let cb = ctx.props().on_set_week.clone();
            Callback::from(move |_| cb.emit(week_offset - 1))
        };
        let on_next = {
            let cb = ctx.props().on_set_week.clone();
            Callback::from(move |_| cb.emit(week_offset + 1))
        };
        let on_toggle_week_start = {
            let cb = ctx.props().on_set_week_start.clone();
            Callback::from(move |_| cb.emit(!week_start))
        };
        let on_reset = {
            let cb = ctx.props().on_reset.clone();
            Callback::from(move |_| cb.emit(()))
        };

        html! {
            <div class="column">
                <Button
                    key="toggle-week-start"
                    icon={if week_start { "chevron-double-down" } else { "chevron-double-right" }}
                    class="mobile-only"
                    disabled={!can_reveal_week_start}
                    title="Show start of week"
                    text={if week_start { "Hide week start" } else { "Show week start" }}
                    onclick={on_toggle_week_start}
                />

                <div class="row center">
                    <Button icon="chevron-left" title="Previous week" onclick={on_prev} />

                    <div class="input-group mobile-fill">
                        <Button icon="minus" title="Fewer weeks" onclick={on_fewer} disabled={week_count <= 1} />
                        <span class="input-text has-text fill">{week_count_display(week_count).to_string()}</span>
                        <Button icon="plus" title="More weeks" onclick={on_more} />
                    </div>

                    <Button icon="arrow-uturn-left" title="Reset to current week" onclick={on_reset} disabled={week_offset == 0 && !week_start} />
                    <Button icon="chevron-right" title="Next week" onclick={on_next} />
                </div>

                <div class="calendar-grid">
                    { for weeks.iter().enumerate().map(|(index, (month_band, days))| {
                        html! {
                            <>
                            if let Some((month_name, year)) = month_band {
                                <div key={format!("calendar-month-{index}")} class="calendar-month">
                                    {format!("{month_name} {year}")}
                                </div>

                                <div key={format!("calendar-weekdays-{index}")} class="calendar-weekdays desktop-only">
                                    { for api::Weekday::ALL.iter().map(|wd| html! {
                                        <div class="calendar-weekday">{wd.short_name()}</div>
                                    }) }
                                </div>
                            }

                            <div key={format!("calendar-week-{index}")} class="calendar-week">
                                { for days.iter().enumerate().map(|(index, &day)| {
                                    let is_today = day == today;
                                    let is_tomorrow = day == today.checked_add_days(1).unwrap_or(day);
                                    let is_past  = day < today;
                                    let shows = schedule_lookup.get(&day).map(|d| d.shows.as_slice()).unwrap_or(&[]);
                                    let movies = schedule_lookup.get(&day).map(|d| d.movies.as_slice()).unwrap_or(&[]);

                                    html! {
                                        <div key={index} class={classes!(
                                            "calendar-cell",
                                            is_today.then_some("today"),
                                            is_past.then_some("past"),
                                            (is_past && !show_past).then_some("desktop-only"),
                                            (!loading && shows.is_empty() && movies.is_empty()).then_some("desktop-only"),
                                        )}>
                                            <div class="calendar-day-number">
                                                <span class="bullet">{day.day()}</span>

                                                if is_today {
                                                    <div class="day-of-week">
                                                        <span>{"Today"}</span>
                                                        <span class="mobile-only">{DOT}</span>
                                                        <span class="mobile-only">{day.weekday().long_name()}</span>
                                                    </div>
                                                } else if is_tomorrow {
                                                    <div class="day-of-week">
                                                        <span>{"Tomorrow"}</span>
                                                        <span class="mobile-only">{DOT}</span>
                                                        <span class="mobile-only">{day.weekday().long_name()}</span>
                                                    </div>
                                                } else {
                                                    <span class="day-of-week mobile-only">{day.weekday().short_name()}</span>
                                                }
                                            </div>

                                            if loading {
                                                <div class="calendar-items">
                                                    <Skeleton />
                                                </div>
                                            } else if !shows.is_empty() || !movies.is_empty() {
                                                <div class="calendar-items">
                                                    { for shows.iter().enumerate().map(|(index, entry)| {
                                                        let show_id = entry.show_id;
                                                        let episode = entry.episodes.last().map(|ep| ep.code());
                                                        let onclick = link.callback(move |_| {
                                                            let season = episode.map(|e| e.season).unwrap_or_default();
                                                            Msg::Navigate(Route::ShowDetail(show_id, ShowDetailQuery { season, episode, orphaned: false }))
                                                        });

                                                        html! {
                                                            <div key={format!("show-{index}")} class="calendar-item" title={format!("Open {}", entry.show_title)}>
                                                                <div class="calendar-item-title clickable" {onclick}>
                                                                    <span class="item-inline">
                                                                        <span class="icon tv" />
                                                                    </span>

                                                                    {&entry.show_title}
                                                                </div>

                                                                {for entry.episodes.iter().enumerate().map(|(index, ep)| {
                                                                    let episode = ep.code();
                                                                    let onclick = link.callback(move |_|
                                                                        Msg::Navigate(Route::ShowDetail(show_id, ShowDetailQuery { season: episode.season, episode: Some(episode), orphaned: false }))
                                                                    );

                                                                    html! {
                                                                        <div key={format!("episode-{index}")} class="calendar-item-code clickable" onclick={onclick} title={format!("Open {} {}", entry.show_title, ep.code())}>
                                                                            <span>{ep.aired.time_of_day(self.time.clone())}</span>
                                                                            <span>{ep.code().to_string()}</span>
                                                                        </div>
                                                                    }
                                                                })}
                                                            </div>
                                                        }
                                                    }) }

                                                    { for movies.iter().enumerate().map(|(index, movie)| {
                                                        let movie_id = movie.movie_id;

                                                        let on_click = link.callback(move |_|
                                                            Msg::Navigate(Route::MovieDetail(movie_id))
                                                        );

                                                        html! {
                                                            <div key={format!("movie-{index}")} class="calendar-item clickable" onclick={on_click} title={movie.title.clone()}>
                                                                <div class="calendar-item-title">
                                                                    <span class="item-inline">
                                                                        <span class="icon film" />
                                                                    </span>

                                                                    {&movie.title}
                                                                </div>

                                                                <div class="calendar-item-code">
                                                                    {movie.released.time_of_day(self.time.clone())}
                                                                </div>
                                                            </div>
                                                        }
                                                    }) }
                                                </div>
                                            }
                                        </div>
                                    }
                                }) }
                            </div>
                            </>
                        }
                    }) }
                </div>
            </div>
        }
    }
}

impl Calendar {
    fn try_update(&mut self, ctx: &Context<Self>, msg: Msg) -> Result<bool, Error> {
        match msg {
            Msg::Channel(result) => {
                self.channel = result?;
                if self.channel.id() != ws::ChannelId::NONE {
                    self.load_schedule(ctx, true);
                } else {
                    self.schedule.clear();
                }
                Ok(true)
            }
            Msg::AppBroadcast(packet) => {
                let event = packet?.decode_event()?;
                if event.channel == self.channel.id() {
                    return Ok(false);
                }
                match event.kind {
                    api::AppEventKind::EpisodesChanged { .. }
                    | api::AppEventKind::ShowChanged { .. }
                    | api::AppEventKind::ShowCreated { .. }
                    | api::AppEventKind::ShowDeleted { .. }
                    | api::AppEventKind::MovieChanged { .. }
                    | api::AppEventKind::MovieCreated { .. }
                    | api::AppEventKind::MovieDeleted { .. }
                    | api::AppEventKind::WatchedChanged { .. }
                    | api::AppEventKind::TaskCompleted { .. } => {
                        if self.channel.id() != ws::ChannelId::NONE {
                            self.load_schedule(ctx, false);
                        }
                        Ok(false)
                    }
                    _ => Ok(false),
                }
            }
            Msg::ScheduleLoaded(result) => {
                self.loading = false;
                self.schedule = result
                    .context(Message::LoadingSchedule)?
                    .decode()
                    .context(Message::LoadingSchedule)?
                    .days;
                Ok(true)
            }
            Msg::Navigate(route) => {
                self.router.push(route);
                Ok(false)
            }
            Msg::SetTime(time) => {
                self.time = time;

                if self.channel.id() != ws::ChannelId::NONE {
                    self.load_schedule(ctx, false);
                }

                Ok(true)
            }
        }
    }

    fn load_schedule(&mut self, ctx: &Context<Self>, show_loading: bool) {
        self.loading = show_loading;
        let weeks = ctx.props().weeks.max(1);
        let today = api::Date::today();

        // Start on the Monday of the visible window, then span whole weeks.
        let start_offset_days = ctx.props().week_offset * 7 - today.weekday().from_monday() as i32;

        self._schedule_req = self
            .channel
            .request()
            .body(api::ListScheduleRequest {
                tz: self.time.tz().iana_name(),
                start_offset_days,
                days: weeks * 7,
            })
            .on_packet(ctx.link().callback(Msg::ScheduleLoaded))
            .send();
    }
}

/// Monday of the visible window: the Monday of the current week shifted by
/// `week_offset` whole weeks.
fn window_start(today: api::Date, week_offset: i32) -> api::Date {
    let offset = week_offset * 7 - today.weekday().from_monday() as i32;

    let result = if offset >= 0 {
        today.checked_add_days(offset as u32)
    } else {
        today.checked_sub_days(offset.unsigned_abs())
    };

    result.unwrap_or(today)
}

fn build_weeks(
    window_start: api::Date,
    weeks: u32,
) -> Vec<(Option<(&'static str, i16)>, [api::Date; 7])> {
    let mut out = Vec::new();
    let mut d = window_start;
    let mut last_shown_month: Option<u8> = None;

    for _ in 0..weeks.max(1) {
        let days: [api::Date; 7] = array::from_fn(|i| d.checked_add_days(i as u32).unwrap_or(d));

        let band_day = days.iter().find(|day| day.day() == 1).unwrap_or(&days[0]);

        let month_band = if last_shown_month != Some(band_day.month()) {
            last_shown_month = Some(band_day.month());
            Some((band_day.month_name(), band_day.year()))
        } else {
            None
        };

        out.push((month_band, days));

        let Some(next) = d.checked_add_days(7) else {
            break;
        };

        d = next;
    }

    out
}

fn week_count_display(week_count: u32) -> impl fmt::Display {
    fmt::from_fn(move |f| {
        if week_count == 1 {
            write!(f, "1 week")
        } else {
            write!(f, "{week_count} weeks")
        }
    })
}
