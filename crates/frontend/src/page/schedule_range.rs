use std::collections::HashMap;

use api::TimeInfo;
use musli_web::web03::prelude::*;
use yew::prelude::*;

use crate::SetupChannel;
use crate::background::Background;
use crate::error::{CustomContext, Error, Message};
use crate::router::{Route, Router, ShowDetailQuery};
use crate::ui::{Button, Image, Skeleton};

#[derive(Properties, PartialEq)]
pub(crate) struct Props {
    /// Days the strip starts from today (0 = starts today; negative reaches into
    /// the past). Persisted in the URL by the parent.
    pub(crate) day_offset: i32,
    /// Navigate to a new day offset (the parent persists it in the URL).
    pub(crate) on_set_range: Callback<i32>,
}

/// A compact upcoming-days strip: a configurable number of consecutive days
/// rendered like the schedule but without week breaks, with a poster rail on the
/// left that tracks the hovered entry. Scrolls one day at a time. Sits above the
/// full [`super::Calendar`] on the dashboard. The visible day count lives in
/// [`api::Config::schedule_range_days`].
pub(crate) struct ScheduleRange {
    channel: ws::Channel,
    schedule: Vec<api::ScheduledDay>,
    config: api::Config,
    loading: bool,
    /// Poster of the hovered entry; falls back to the first upcoming show.
    hovered_poster: Option<api::Image>,
    time: TimeInfo,
    _time_handle: ContextHandle<TimeInfo>,
    router: Router,
    background: Background,
    _setup: SetupChannel,
    _broadcast: ws::Listener,
    _schedule_req: ws::Request,
    _config_req: ws::Request,
    _set_config_req: ws::Request,
}

pub(crate) enum Msg {
    Channel(Result<ws::Channel, ws::Error>),
    AppBroadcast(Result<ws::Packet<api::AppBroadcast>, ws::Error>),
    ScheduleLoaded(Result<ws::Packet<api::ListSchedule>, ws::Error>),
    ConfigLoaded(Result<ws::Packet<api::GetConfig>, ws::Error>),
    AdjustRangeDays(i32),
    SetConfigDone(Result<ws::Packet<api::SetConfig>, ws::Error>),
    Navigate(Route),
    SetTime(TimeInfo),
    /// Hover an entry: show its poster in the rail and drive the page background
    /// from its backdrop. Carries (poster, backdrop URL).
    Hover(Option<api::Image>, Option<String>),
}

impl Component for ScheduleRange {
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
            config: api::Config::default(),
            loading: false,
            hovered_poster: None,
            time,
            _time_handle,
            router,
            background,
            _setup,
            _broadcast,
            _schedule_req: ws::Request::default(),
            _config_req: ws::Request::default(),
            _set_config_req: ws::Request::default(),
        }
    }

    fn changed(&mut self, ctx: &Context<Self>, old: &Props) -> bool {
        // Refetch only when the start offset actually changed. The day count
        // lives in config and is refetched via its own path.
        if ctx.props().day_offset != old.day_offset && self.channel.id() != ws::ChannelId::NONE {
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
        let offset = ctx.props().day_offset;
        let days_count = self.config.schedule_range_days.max(1) as usize;

        let start = if offset >= 0 {
            today.checked_add_days(offset as u32)
        } else {
            today.checked_sub_days(offset.unsigned_abs())
        }
        .unwrap_or(today);
        let days: Vec<api::Date> = (0..days_count)
            .map(|i| start.checked_add_days(i as u32).unwrap_or(start))
            .collect();

        // Rail poster: the hovered entry, else the first upcoming show.
        let poster = self
            .hovered_poster
            .clone()
            .or_else(|| self.default_poster());

        let schedule_lookup: HashMap<api::Date, &api::ScheduledDay> =
            self.schedule.iter().map(|d| (d.date, d)).collect();

        let link = ctx.link();

        let on_prev = ctx.props().on_set_range.reform(move |_| offset - 1);
        let on_next = ctx.props().on_set_range.reform(move |_| offset + 1);
        let on_reset = ctx.props().on_set_range.reform(|_| 0);
        let on_fewer = link.callback(|_| Msg::AdjustRangeDays(-1));
        let on_more = link.callback(|_| Msg::AdjustRangeDays(1));

        html! {
            <div class="schedule-range">
                <div class="row center">
                    <Button icon="chevron-left" title="Previous day" onclick={on_prev} />

                    <div class="input-group mobile-fill">
                        <Button icon="minus" title="Fewer days" onclick={on_fewer} disabled={days_count <= 1} />
                        <span class="input-text has-text fill">{format!("{days_count} {}", if days_count == 1 { "day" } else { "days" })}</span>
                        <Button icon="plus" title="More days" onclick={on_more} />
                    </div>

                    <Button icon="chevron-right" title="Next day" onclick={on_next} />
                </div>

                <div class={classes!("schedule-range-grid", (offset != 0).then_some("has-reset"))} style={format!("--range-days: {days_count}")}>
                    <div class="schedule-range-poster desktop-only">
                        <Image src={poster.clone()} />
                    </div>

                    if offset != 0 {
                        <div class="schedule-range-reset clickable" title="Back to today" onclick={on_reset}>
                            <span class="item-inline-lg">
                                <span class={classes!("desktop-only", "icon", if offset > 0 { "chevron-double-left" } else { "chevron-double-right" })} />
                                <span class={classes!("mobile-only", "icon", if offset > 0 { "chevron-double-up" } else { "chevron-double-down" })} />
                            </span>

                            <span>{offset.abs()}</span>
                        </div>
                    }

                    { for days.iter().map(|&day| self.view_day(ctx, day, today, &schedule_lookup)) }
                </div>
            </div>
        }
    }
}

impl ScheduleRange {
    fn try_update(&mut self, ctx: &Context<Self>, msg: Msg) -> Result<bool, Error> {
        match msg {
            Msg::Channel(result) => {
                self.channel = result?;
                if self.channel.id() != ws::ChannelId::NONE {
                    self.load_schedule(ctx, true);
                    self.load_config(ctx);
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
                    api::AppEventKind::ConfigChanged { config } => {
                        // Adopt the fresh config; reload the schedule only when the
                        // visible day count actually changed.
                        let days_changed =
                            config.schedule_range_days != self.config.schedule_range_days;
                        self.config = config;
                        if days_changed && self.channel.id() != ws::ChannelId::NONE {
                            self.load_schedule(ctx, false);
                        }
                        Ok(true)
                    }
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
            Msg::ConfigLoaded(result) => {
                let config = result
                    .context(Message::LoadingSchedule)?
                    .decode()
                    .context(Message::LoadingSchedule)?
                    .config;

                let days_changed = config.schedule_range_days != self.config.schedule_range_days;
                self.config = config;
                if days_changed && self.channel.id() != ws::ChannelId::NONE {
                    self.load_schedule(ctx, false);
                }
                Ok(true)
            }
            Msg::AdjustRangeDays(delta) => {
                self.config.schedule_range_days = self
                    .config
                    .schedule_range_days
                    .saturating_add_signed(delta)
                    .max(1);

                // Reload directly: our own SetConfig broadcast is filtered out.
                if self.channel.id() != ws::ChannelId::NONE {
                    self.load_schedule(ctx, true);
                }

                self._set_config_req = self
                    .channel
                    .request()
                    .body(api::SetConfigRequest {
                        config: self.config.clone(),
                    })
                    .on_packet(ctx.link().callback(Msg::SetConfigDone))
                    .send();
                Ok(true)
            }
            Msg::SetConfigDone(result) => {
                result.context(Message::SavingConfig)?;
                Ok(false)
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
            Msg::Hover(poster, backdrop) => {
                // Keep the last hovered background (no revert on mouse-leave).
                self.background.background(backdrop);
                if self.hovered_poster == poster {
                    return Ok(false);
                }
                self.hovered_poster = poster;
                Ok(true)
            }
        }
    }

    /// Poster shown in the rail when nothing is hovered: the first show of the
    /// soonest loaded day (falling back to the first movie).
    fn default_poster(&self) -> Option<api::Image> {
        self.schedule.iter().find_map(|d| {
            d.shows
                .iter()
                .find_map(|s| s.poster.clone())
                .or_else(|| d.movies.iter().find_map(|m| m.poster.clone()))
        })
    }

    fn view_day(
        &self,
        ctx: &Context<Self>,
        day: api::Date,
        today: api::Date,
        lookup: &HashMap<api::Date, &api::ScheduledDay>,
    ) -> Html {
        let link = ctx.link();

        let is_yesterday = day == today.checked_sub_days(1).unwrap_or(day);
        let is_today = day == today;
        let is_tomorrow = day == today.checked_add_days(1).unwrap_or(day);
        let items = lookup.get(&day).map(|d| d.items()).unwrap_or_default();

        let label = if is_yesterday {
            "Yesterday"
        } else if is_today {
            "Today"
        } else if is_tomorrow {
            "Tomorrow"
        } else {
            day.weekday().long_name()
        };

        html! {
            <div class={classes!("calendar-cell", is_today.then_some("today"))}>
                <div class="calendar-day-number">
                    <span class="bullet">{day.day()}</span>
                    <div class="day-of-week">
                        <span>{label}</span>
                    </div>
                </div>

                if self.loading {
                    <div class="calendar-items">
                        <Skeleton />
                    </div>
                } else if !items.is_empty() {
                    <div class="calendar-items">
                        { for items.iter().map(|item| match item {
                            api::ScheduleItem::Show(entry) => {
                                let show_id = entry.show_id;
                                let episode = entry.episodes.last().map(|ep| ep.code());
                                let onclick = link.callback(move |_| {
                                    let season = episode.map(|e| e.season).unwrap_or_default();
                                    Msg::Navigate(Route::ShowDetail(show_id, ShowDetailQuery { season, episode, orphaned: false }))
                                });

                                let hover_poster = entry.poster.clone();
                                let hover_bg = entry.backdrop.as_ref().map(|i| i.proxy_url());
                                let onmouseover = link.callback(move |_| Msg::Hover(hover_poster.clone(), hover_bg.clone()));

                                html! {
                                    <div key={format!("show-{show_id}")} class="calendar-item" title={format!("Open {}", entry.show_title)} {onmouseover}>
                                        <div class="calendar-item-title clickable" {onclick}>
                                            <span class="item-inline">
                                                <span class="icon tv" />
                                            </span>

                                            {&entry.show_title}
                                        </div>

                                        { for entry.episodes.iter().enumerate().map(|(index, ep)| {
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
                                        }) }
                                    </div>
                                }
                            }
                            api::ScheduleItem::Movie(movie) => {
                                let movie_id = movie.movie_id;

                                let on_click = link.callback(move |_|
                                    Msg::Navigate(Route::MovieDetail(movie_id))
                                );

                                let hover_poster = movie.poster.clone();
                                let hover_bg = movie.backdrop.as_ref().map(|i| i.proxy_url());
                                let onmouseover = link.callback(move |_| Msg::Hover(hover_poster.clone(), hover_bg.clone()));

                                html! {
                                    <div key={format!("movie-{movie_id}")} class="calendar-item clickable" onclick={on_click} title={movie.title.clone()} {onmouseover}>
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
                            }
                        }) }
                    </div>
                }
            </div>
        }
    }

    fn load_schedule(&mut self, ctx: &Context<Self>, show_loading: bool) {
        self.loading = show_loading;

        self._schedule_req = self
            .channel
            .request()
            .body(api::ListScheduleRequest {
                tz: self.time.tz().iana_name(),
                start_offset_days: ctx.props().day_offset,
                days: self.config.schedule_range_days.max(1),
            })
            .on_packet(ctx.link().callback(Msg::ScheduleLoaded))
            .send();
    }

    fn load_config(&mut self, ctx: &Context<Self>) {
        self._config_req = self
            .channel
            .request()
            .body(api::GetConfigRequest)
            .on_packet(ctx.link().callback(Msg::ConfigLoaded))
            .send();
    }
}
