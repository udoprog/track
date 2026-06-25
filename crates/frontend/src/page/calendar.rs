use core::array;
use std::collections::HashMap;

use api::TimeInfo;
use musli_web::web03::prelude::*;
use yew::prelude::*;

use crate::SetupChannel;
use crate::background::Background;
use crate::error::{CustomContext, Error, Message};
use crate::router::{Route, Router, ShowDetailQuery};
use crate::ui::DOT;

pub(crate) struct Calendar {
    channel: ws::Channel,
    schedule: Vec<api::ScheduledDay>,
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
    type Properties = ();

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
            time,
            _time_handle,
            router,
            background,
            _setup,
            _broadcast,
            _schedule_req: ws::Request::default(),
        }
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
        let weeks = build_weeks(&self.schedule, today);

        let schedule_lookup: HashMap<api::Date, &api::ScheduledDay> =
            self.schedule.iter().map(|d| (d.date, d)).collect();

        let link = ctx.link();

        html! {
            <div class="calendar-grid">
                { for weeks.iter().map(|(month_band, days)| {
                    html! {
                        <>
                        if let Some((month_name, year)) = month_band {
                            <div class="calendar-month">
                                {format!("{month_name} {year}")}
                            </div>

                            <div class="calendar-weekdays desktop-only">
                                { for api::Weekday::ALL.iter().map(|wd| html! {
                                    <div class="calendar-weekday">{wd.short_name()}</div>
                                }) }
                            </div>
                        }

                        <div class="calendar-week">
                            { for days.iter().map(|&day| {
                                let is_today = day == today;
                                let is_tomorrow = day == today.checked_add_days(1).unwrap_or(day);
                                let is_past  = day < today;
                                let shows = schedule_lookup.get(&day).map(|d| d.shows.as_slice()).unwrap_or(&[]);
                                let movies = schedule_lookup.get(&day).map(|d| d.movies.as_slice()).unwrap_or(&[]);

                                html! {
                                    <div class={classes!(
                                        "calendar-cell",
                                        is_today.then_some("today"),
                                        is_past.then_some("past"),
                                        is_past.then_some("desktop-only"),
                                        (shows.is_empty() && movies.is_empty()).then_some("desktop-only"),
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

                                        if !shows.is_empty() || !movies.is_empty() {
                                            <div class="calendar-items">
                                                { for shows.iter().map(|entry| {
                                                    let show_id = entry.show_id;
                                                    let episode = entry.episodes.last().map(|ep| ep.code());
                                                    let onclick = link.callback(move |_| {
                                                        let season = episode.map(|e| e.season).unwrap_or_default();
                                                        Msg::Navigate(Route::ShowDetail(show_id, ShowDetailQuery { season, episode, orphaned: false }))
                                                    });

                                                    html! {
                                                        <div class="calendar-item" title={format!("Open {}", entry.show_title)}>
                                                            <div class="calendar-item-title clickable" {onclick}>
                                                                <span class="item-inline">
                                                                    <span class="icon tv" />
                                                                </span>

                                                                {&entry.show_title}
                                                            </div>

                                                            {for entry.episodes.iter().map(move |ep| {
                                                                let episode = ep.code();
                                                                let onclick = link.callback(move |_|
                                                                    Msg::Navigate(Route::ShowDetail(show_id, ShowDetailQuery { season: episode.season, episode: Some(episode), orphaned: false }))
                                                                );

                                                                html! {
                                                                    <div class="calendar-item-code clickable" onclick={onclick} title={format!("Open {} {}", entry.show_title, ep.code())}>
                                                                        <span>{ep.aired.time_of_day(self.time.clone())}</span>
                                                                        <span>{ep.code().to_string()}</span>
                                                                    </div>
                                                                }
                                                            })}
                                                        </div>
                                                    }
                                                }) }

                                                { for movies.iter().map(|movie| {
                                                    let movie_id = movie.movie_id;

                                                    let on_click = link.callback(move |_|
                                                        Msg::Navigate(Route::MovieDetail(movie_id))
                                                    );

                                                    html! {
                                                        <div class="calendar-item clickable" onclick={on_click} title={movie.title.clone()}>
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
        }
    }
}

impl Calendar {
    fn try_update(&mut self, ctx: &Context<Self>, msg: Msg) -> Result<bool, Error> {
        match msg {
            Msg::Channel(result) => {
                self.channel = result?;
                if self.channel.id() != ws::ChannelId::NONE {
                    self.load_schedule(ctx);
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
                            self.load_schedule(ctx);
                        }
                        Ok(false)
                    }
                    _ => Ok(false),
                }
            }
            Msg::ScheduleLoaded(result) => {
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
                    self.load_schedule(ctx);
                }

                Ok(true)
            }
        }
    }

    fn load_schedule(&mut self, ctx: &Context<Self>) {
        self._schedule_req = self
            .channel
            .request()
            .body(api::ListScheduleRequest {
                tz: self.time.tz().iana_name(),
                days: 28,
            })
            .on_packet(ctx.link().callback(Msg::ScheduleLoaded))
            .send();
    }
}

fn build_weeks(
    schedule: &[api::ScheduledDay],
    today: api::Date,
) -> Vec<(Option<(&'static str, i16)>, [api::Date; 7])> {
    // Start on Monday of today's week (may include past days)
    let Some(start) = today.checked_sub_days(today.weekday().from_monday()) else {
        return Vec::new();
    };

    // End on Sunday of the week containing the last scheduled date; always
    // extend to at least 4 weeks from today so the grid is never empty.
    let Some(floor) = today.checked_add_days(27) else {
        return Vec::new();
    };

    let last_date = schedule.last().map(|d| d.date).unwrap_or(today);
    let last_date = if last_date > floor { last_date } else { floor };

    let Some(end) = last_date.checked_add_days(6 - last_date.weekday().from_monday()) else {
        return Vec::new();
    };

    let mut weeks = Vec::new();
    let mut d = start;
    let mut last_shown_month: Option<u8> = None;

    while d <= end {
        let days: [api::Date; 7] = array::from_fn(|i| d.checked_add_days(i as u32).unwrap_or(d));

        let band_day = days.iter().find(|day| day.day() == 1).unwrap_or(&days[0]);

        let month_band = if last_shown_month != Some(band_day.month()) {
            last_shown_month = Some(band_day.month());
            Some((band_day.month_name(), band_day.year()))
        } else {
            None
        };

        weeks.push((month_band, days));

        let Some(next) = d.checked_add_days(7) else {
            break;
        };

        d = next;
    }

    weeks
}
