use core::array;
use std::collections::HashMap;

use api::TimeZone;
use musli_web::web03::prelude::*;
use yew::prelude::*;

use crate::SetupChannel;
use crate::error::{CustomContext, Error, Message};
use crate::router::{Route, SeriesDetailQuery};

pub(super) struct Calendar {
    channel: ws::Channel,
    schedule: Vec<api::ScheduledDay>,
    tz: TimeZone,
    _tz_handle: ContextHandle<TimeZone>,
    _setup: SetupChannel,
    _broadcast: ws::Listener,
    _schedule_req: ws::Request,
}

pub(super) enum Msg {
    Channel(Result<ws::Channel, ws::Error>),
    AppBroadcast(Result<ws::Packet<api::AppBroadcast>, ws::Error>),
    ScheduleLoaded(Result<ws::Packet<api::ListSchedule>, ws::Error>),
    Navigate(Route),
    SetTz(TimeZone),
}

#[derive(Properties, PartialEq)]
pub(super) struct Props {
    pub(super) onerror: Callback<Option<Error>>,
    pub(super) on_navigate: Callback<Route>,
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

        let (tz, _tz_handle) = ctx
            .link()
            .context::<TimeZone>(ctx.link().callback(Msg::SetTz))
            .expect("Expected a configured time zone");

        Self {
            channel: ws::Channel::default(),
            schedule: Vec::new(),
            tz,
            _tz_handle,
            _setup,
            _broadcast,
            _schedule_req: ws::Request::default(),
        }
    }

    fn update(&mut self, ctx: &Context<Self>, msg: Self::Message) -> bool {
        match self.try_update(ctx, msg) {
            Ok(render) => render,
            Err(e) => {
                ctx.props().onerror.emit(Some(e));
                false
            }
        }
    }

    fn view(&self, ctx: &Context<Self>) -> Html {
        let today = api::Date::today();
        let weeks = build_weeks(&self.schedule, today);

        let schedule_lookup: HashMap<api::Date, &[api::ScheduledEntry]> = self
            .schedule
            .iter()
            .map(|d| (d.date, d.entries.as_slice()))
            .collect();

        let link = ctx.link();

        html! {
            <div class="calendar-grid">
                <div class="calendar-weekday-header hide-mobile">
                    { for api::Weekday::ALL.iter().map(|wd| html! {
                        <div class="calendar-weekday">{wd.short_name()}</div>
                    }) }
                </div>

                { for weeks.iter().map(|(month_band, days)| {
                    html! {
                        <>
                        if let Some((month_name, year)) = month_band {
                            <div class="calendar-month">
                                {format!("{month_name} {year}")}
                            </div>
                        }
                        <div class="calendar-week">
                            { for days.iter().map(|&day| {
                                let is_today = day == today;
                                let is_past  = day < today;
                                let entries  = schedule_lookup.get(&day).copied().unwrap_or(&[]);

                                html! {
                                    <div class={classes!(
                                        "calendar-cell",
                                        is_today.then_some("today"),
                                        is_past.then_some("past"),
                                        is_past.then_some("hide-mobile"),
                                    )}>
                                        <div class="calendar-day-number">
                                            <span class="bullet">{day.day()}</span>
                                        </div>

                                        if !entries.is_empty() {
                                            <div class="calendar-items">
                                                { for entries.iter().map(|entry| {
                                                    let series_id = entry.series_id;
                                                    let season = entry.episodes.first().map(|ep| ep.season);

                                                    let on_click = link.callback(move |_|
                                                        Msg::Navigate(Route::SeriesDetail(series_id, SeriesDetailQuery { season }))
                                                    );

                                                    let codes = entry.episodes.iter()
                                                        .map(|ep| format!("{}E{:02}", ep.season.short(), ep.episode))
                                                        .collect::<Vec<_>>()
                                                        .join(" ");

                                                    html! {
                                                        <div class="calendar-item clickable" onclick={on_click}>
                                                            <div class="calendar-item-title">{&entry.series_title}</div>
                                                            <div class="calendar-item-code">{codes}</div>
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
                    | api::AppEventKind::SeriesChanged { .. }
                    | api::AppEventKind::SeriesCreated { .. }
                    | api::AppEventKind::SeriesDeleted { .. }
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
                ctx.props().on_navigate.emit(route);
                Ok(false)
            }
            Msg::SetTz(tz) => {
                self.tz = tz;

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
                tz: self.tz.iana_name(),
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
