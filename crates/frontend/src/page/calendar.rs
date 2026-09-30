use core::array;
use std::collections::HashMap;

use api::TimeInfo;
use musli_web::web03::prelude::*;
use yew::prelude::*;

use crate::SetupChannel;
use crate::background::Background;
use crate::error::{CustomContext, Error, Message};
use crate::router::MediaSelection;
use crate::ui::{Button, ContextMenu, Skeleton};

use super::schedule_item::{view_day_heading, view_schedule_item};

#[derive(Properties, PartialEq)]
pub(crate) struct Props {
    /// Offset of the visible window from the current week, in weeks (negative
    /// reaches into the past). Stored in the URL by the parent.
    pub(crate) week_offset: i32,
    /// Mobile-only: reveal the past days of the current week. Stored in the URL
    /// by the parent.
    pub(crate) week_start: bool,
    /// Navigate to the given week offset (the parent persists it in the URL).
    pub(crate) on_set_week: Callback<i32>,
    /// Set whether the start of the week is revealed (parent persists it in the
    /// URL).
    pub(crate) on_set_week_start: Callback<bool>,
    /// Reset the visible window back to the current week (parent clears the
    /// offset and week-start reveal in the URL).
    pub(crate) on_reset: Callback<()>,
    /// Which media kinds are shown (owned by the parent's URL query).
    pub(crate) selection: MediaSelection,
}

pub(crate) struct Calendar {
    channel: ws::Channel,
    schedule: Vec<api::ScheduledDay>,
    /// `schedule` indices keyed by date, maintained when `schedule` changes so
    /// `view` can look days up without building a map every render.
    schedule_index: HashMap<api::Date, usize>,
    config: api::Config,
    loading: bool,
    time: TimeInfo,
    _time_handle: ContextHandle<TimeInfo>,
    background: Background,
    _setup: SetupChannel,
    _broadcast: ws::Listener,
    _schedule_req: ws::Request,
    _config_req: ws::Request,
    _set_config_req: ws::Request,
    /// Whether the view options popover (weeks shown) is open.
    options_open: bool,
    options_anchor: NodeRef,
}

pub(crate) enum Msg {
    Channel(Result<ws::Channel, ws::Error>),
    AppBroadcast(Result<ws::Packet<api::AppBroadcast>, ws::Error>),
    ScheduleLoaded(Result<ws::Packet<api::ListSchedule>, ws::Error>),
    ConfigLoaded(Result<ws::Packet<api::GetConfig>, ws::Error>),
    AdjustScheduleWeeks(i32),
    SetConfigDone(Result<ws::Packet<api::SetConfig>, ws::Error>),
    SetTime(TimeInfo),
    /// Hover an entry: drive the page background from its backdrop.
    Hover(Option<String>),
    ToggleOptions,
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

        let (background, _) = ctx
            .link()
            .context::<Background>(Callback::noop())
            .expect("Expected Background in context");

        Self {
            channel: ws::Channel::default(),
            schedule: Vec::new(),
            schedule_index: HashMap::new(),
            config: api::Config::default(),
            loading: false,
            time,
            _time_handle,
            background,
            _setup,
            _broadcast,
            _schedule_req: ws::Request::default(),
            _config_req: ws::Request::default(),
            _set_config_req: ws::Request::default(),
            options_open: false,
            options_anchor: NodeRef::default(),
        }
    }

    fn changed(&mut self, ctx: &Context<Self>, old: &Props) -> bool {
        let props = ctx.props();

        // Refetch only when the visible window offset actually changed, not on
        // every incidental prop/callback change. The week count lives in config
        // and is refetched via its own path.
        if props.week_offset != old.week_offset && self.channel.id() != ws::ChannelId::NONE {
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
        let week_count = self.config.schedule_weeks.max(1);
        let week_offset = ctx.props().week_offset;
        let week_start = ctx.props().week_start;
        let selection = ctx.props().selection;
        let window_start = window_start(today, week_offset);
        let weeks = build_weeks(window_start, week_count);

        // Past days are hidden on mobile by default; reveal them when the start
        // of the current week is toggled or when navigating into the past.
        let show_past = week_start || week_offset < 0;
        // The reveal toggle only makes sense on the current week when there are
        // hidden past days (i.e. today is not the start of the week).
        let can_reveal_week_start = week_offset == 0 && today.weekday() != api::Weekday::Monday;

        let loading = self.loading;

        let link = ctx.link();

        let on_fewer = link.callback(|_| Msg::AdjustScheduleWeeks(-1));
        let on_more = link.callback(|_| Msg::AdjustScheduleWeeks(1));
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

        let on_hover = link.callback(Msg::Hover);

        let first = weeks[0].1[0];
        let last = weeks[weeks.len() - 1].1[6];

        html! {
            <div class="column">
                <div class="page-controls">
                    <span class="text-muted">
                        { format!("{} {} – {} {}", first.day(), first.month_name(), last.day(), last.month_name()) }
                    </span>

                    <div class="row">
                        <Button icon="chevron-left" title="Previous week" class="ghost" onclick={on_prev} />
                        <Button icon="calendar" label="This week" title="Back to this week" class="chip" disabled={week_offset == 0 && !week_start} onclick={on_reset} />
                        <Button icon="chevron-right" title="Next week" class="ghost" onclick={on_next} />
                        <Button node_ref={self.options_anchor.clone()} icon="adjustments-horizontal" title="View options" class={classes!("chip", self.options_open.then_some("selected"))} expanded={Some(self.options_open)} haspopup="dialog" onclick={link.callback(|_| Msg::ToggleOptions)} />
                    </div>
                </div>

                if self.options_open {
                    <ContextMenu icon="adjustments-horizontal" prompt="View options" anchor={self.options_anchor.clone()} on_close={link.callback(|_| Msg::ToggleOptions)}>
                        <div class="form">
                            <div class="field">
                                <label>{"Weeks shown"}</label>

                                <div class="row">
                                    <Button icon="minus" title="Fewer weeks" onclick={on_fewer} disabled={week_count <= 1} />
                                    <span class="page-size">{week_count}</span>
                                    <Button icon="plus" title="More weeks" onclick={on_more} />
                                </div>
                            </div>

                            if can_reveal_week_start {
                                <div class="field mobile-only">
                                    <label>{"Earlier this week"}</label>
                                    <Button icon={if week_start { "eye-slash" } else { "eye" }} label={if week_start { "Hide past days" } else { "Show past days" }} title="Show the days of this week before today" onclick={on_toggle_week_start} />
                                </div>
                            }
                        </div>
                    </ContextMenu>
                }

                <div class="calendar-grid">
                    { for weeks.iter().enumerate().map(|(index, (month_band, days))| {
                        html! {
                            <>
                            if let Some(month_band) = month_band {
                                <div key={format!("calendar-month-{index}")} class="calendar-month">
                                    {month_band}
                                </div>
                            }

                            if index == 0 {
                                <div key="calendar-weekdays" class="calendar-weekdays desktop-only">
                                    { for days.iter().map(|day| html! { <span>{day.weekday().short_name()}</span> }) }
                                </div>
                            }

                            <div key={format!("calendar-week-{index}")} class="calendar-week">
                                { for days.iter().enumerate().map(|(index, &day)| {
                                    let is_today = day == today;
                                    let is_past  = day < today;
                                    let mut items = self.schedule_index.get(&day).map(|&i| self.schedule[i].items()).unwrap_or_default();
                                    items.retain(|i| selection.contains(i.kind()));

                                    html! {
                                        <div key={index} class={classes!(
                                            "calendar-cell",
                                            is_today.then_some("today"),
                                            is_past.then_some("past"),
                                            (is_past && !show_past).then_some("desktop-only"),
                                            (!loading && items.is_empty()).then_some("desktop-only"),
                                        )}>
                                            <span class="calendar-day-number desktop-only">{day.day()}</span>

                                            <div class="mobile-only">
                                                { view_day_heading(day, today) }
                                            </div>

                                            if loading {
                                                <Skeleton class="line" />
                                            } else if !items.is_empty() {
                                                <div class="agenda-items">
                                                    { for items.iter().map(|item| view_schedule_item(item, &self.time, &on_hover)) }
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
    /// Rebuild the date→index lookup after `schedule` changes.
    fn rebuild_schedule_index(&mut self) {
        self.schedule_index.clear();
        self.schedule_index
            .extend(self.schedule.iter().enumerate().map(|(i, d)| (d.date, i)));
    }

    fn try_update(&mut self, ctx: &Context<Self>, msg: Msg) -> Result<bool, Error> {
        match msg {
            Msg::Channel(result) => {
                self.channel = result?;
                if self.channel.id() != ws::ChannelId::NONE {
                    self.load_schedule(ctx, true);
                    self.load_config(ctx);
                } else {
                    self.schedule.clear();
                    self.rebuild_schedule_index();
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
                        // visible week count actually changed.
                        let weeks_changed = config.schedule_weeks != self.config.schedule_weeks;
                        self.config = config;
                        if weeks_changed && self.channel.id() != ws::ChannelId::NONE {
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
                self.rebuild_schedule_index();
                Ok(true)
            }
            Msg::ConfigLoaded(result) => {
                let config = result
                    .context(Message::LoadingSchedule)?
                    .decode()
                    .context(Message::LoadingSchedule)?
                    .config;

                // A larger/smaller week count changes the visible window; reload
                // the schedule if the count differs from the default we started with.
                let weeks_changed = config.schedule_weeks != self.config.schedule_weeks;
                self.config = config;
                if weeks_changed && self.channel.id() != ws::ChannelId::NONE {
                    self.load_schedule(ctx, false);
                }
                Ok(true)
            }
            Msg::AdjustScheduleWeeks(delta) => {
                self.config.schedule_weeks = self
                    .config
                    .schedule_weeks
                    .saturating_add_signed(delta)
                    .max(1);

                // Reload directly: our own SetConfig broadcast is filtered out.
                if self.channel.id() != ws::ChannelId::NONE {
                    self.load_schedule(ctx, true);

                    self._set_config_req = self
                        .channel
                        .request()
                        .body(api::SetConfigRequest {
                            config: self.config.clone(),
                        })
                        .on_packet(ctx.link().callback(Msg::SetConfigDone))
                        .send();
                }

                Ok(true)
            }
            Msg::SetConfigDone(result) => {
                result.context(Message::SavingConfig)?;
                Ok(false)
            }
            Msg::Hover(backdrop) => {
                // Keep the last hovered background (no revert on mouse-leave).
                self.background.background(backdrop);
                Ok(false)
            }
            Msg::ToggleOptions => {
                self.options_open = !self.options_open;
                Ok(true)
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
        if self.channel.id() == ws::ChannelId::NONE {
            return;
        }

        self.loading = show_loading;
        let weeks = self.config.schedule_weeks.max(1);
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

    fn load_config(&mut self, ctx: &Context<Self>) {
        if self.channel.id() == ws::ChannelId::NONE {
            return;
        }

        self._config_req = self
            .channel
            .request()
            .body(api::GetConfigRequest)
            .on_packet(ctx.link().callback(Msg::ConfigLoaded))
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

/// The weeks shown from `window_start`, each with the month heading placed
/// above it: the first week, and every week in which a month begins.
fn build_weeks(window_start: api::Date, weeks: u32) -> Vec<(Option<String>, [api::Date; 7])> {
    let mut out = Vec::new();
    let mut d = window_start;
    let mut last_shown_month: Option<u8> = None;

    for _ in 0..weeks.max(1) {
        let days: [api::Date; 7] = array::from_fn(|i| d.checked_add_days(i as u32).unwrap_or(d));

        let band_day = days.iter().find(|day| day.day() == 1).unwrap_or(&days[0]);

        let month_band = if last_shown_month != Some(band_day.month()) {
            last_shown_month = Some(band_day.month());
            Some(month_range(days[0], days[6]))
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

/// Name the months a week spans, such as `"September – October 2026"`.
fn month_range(first: api::Date, last: api::Date) -> String {
    if first.month() == last.month() {
        format!("{} {}", first.month_name(), first.year())
    } else if first.year() == last.year() {
        format!(
            "{} – {} {}",
            first.month_name(),
            last.month_name(),
            last.year()
        )
    } else {
        format!(
            "{} {} – {} {}",
            first.month_name(),
            first.year(),
            last.month_name(),
            last.year()
        )
    }
}
