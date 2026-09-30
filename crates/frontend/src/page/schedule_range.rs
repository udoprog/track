use std::collections::HashMap;

use api::TimeInfo;
use musli_web::web03::prelude::*;
use yew::prelude::*;

use crate::SetupChannel;
use crate::background::Background;
use crate::error::{CustomContext, Error, Message};
use crate::router::{MediaSelection, Route, Router};
use crate::ui::{Button, ContextMenu, Skeleton};

use super::schedule_item::{view_day_heading, view_schedule_item};

#[derive(Properties, PartialEq)]
pub(crate) struct Props {
    /// Days the strip starts from today (0 = starts today; negative reaches into
    /// the past). Persisted in the URL by the parent.
    pub(crate) day_offset: i32,
    /// Navigate to a new day offset (the parent persists it in the URL).
    pub(crate) on_set_range: Callback<i32>,
    /// Which media kinds are shown (owned by the parent's URL query).
    pub(crate) selection: MediaSelection,
}

/// The Upcoming agenda: a configurable number of consecutive days, each with
/// what airs on it. Hovering an entry shows its backdrop behind the page. Moves
/// one day at a time. The visible day count lives in
/// [`api::Config::schedule_range_days`].
pub(crate) struct ScheduleRange {
    channel: ws::Channel,
    schedule: Vec<api::ScheduledDay>,
    /// `schedule` indices keyed by date, maintained when `schedule` changes so
    /// `view_day` can look days up without building a map every render.
    schedule_index: HashMap<api::Date, usize>,
    config: api::Config,
    loading: bool,
    time: TimeInfo,
    _time_handle: ContextHandle<TimeInfo>,
    router: Router,
    background: Background,
    _setup: SetupChannel,
    _broadcast: ws::Listener,
    _schedule_req: ws::Request,
    _config_req: ws::Request,
    _set_config_req: ws::Request,
    /// Whether the view options popover (days shown) is open.
    options_open: bool,
    options_anchor: NodeRef,
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
    /// Hover an entry: drive the page background from its backdrop.
    Hover(Option<String>),
    ToggleOptions,
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
            schedule_index: HashMap::new(),
            config: api::Config::default(),
            loading: false,
            time,
            _time_handle,
            router,
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

        let link = ctx.link();

        let on_prev = ctx.props().on_set_range.reform(move |_| offset - 1);
        let on_next = ctx.props().on_set_range.reform(move |_| offset + 1);
        let on_reset = ctx.props().on_set_range.reform(|_| 0);
        let on_fewer = link.callback(|_| Msg::AdjustRangeDays(-1));
        let on_more = link.callback(|_| Msg::AdjustRangeDays(1));

        let first = days[0];
        let last = days[days.len() - 1];

        html! {
            <div class="column">
                <div class="page-controls">
                    <span class="text-muted">
                        { format!("{} {} – {} {}", first.day(), first.month_name(), last.day(), last.month_name()) }
                    </span>

                    <div class="row">
                        <Button icon="chevron-left" title="Previous day" class="ghost" onclick={on_prev} />
                        <Button icon="calendar" label="Today" title="Back to today" class="chip" disabled={offset == 0} onclick={on_reset} />
                        <Button icon="chevron-right" title="Next day" class="ghost" onclick={on_next} />
                        <Button node_ref={self.options_anchor.clone()} icon="adjustments-horizontal" title="View options" class={classes!("chip", self.options_open.then_some("selected"))} onclick={link.callback(|_| Msg::ToggleOptions)} />
                    </div>
                </div>

                if self.options_open {
                    <ContextMenu icon="adjustments-horizontal" prompt="View options" anchor={self.options_anchor.clone()} on_close={link.callback(|_| Msg::ToggleOptions)}>
                        <div class="field">
                            <label>{"Days shown"}</label>

                            <div class="row">
                                <Button icon="minus" title="Fewer days" onclick={on_fewer} disabled={days_count <= 1} />
                                <span class="page-size">{days_count}</span>
                                <Button icon="plus" title="More days" onclick={on_more} />
                            </div>
                        </div>
                    </ContextMenu>
                }

                <div class="agenda">
                    { for days.iter().map(|&day| self.view_day(ctx, day, today)) }
                </div>
            </div>
        }
    }
}

impl ScheduleRange {
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
                self.rebuild_schedule_index();
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
            Msg::Hover(backdrop) => {
                // Keep the last hovered background (no revert on mouse-leave).
                self.background.background(backdrop);
                Ok(false)
            }
            Msg::ToggleOptions => {
                self.options_open = !self.options_open;
                Ok(true)
            }
        }
    }

    /// One day of the agenda: its heading and what airs on it.
    fn view_day(&self, ctx: &Context<Self>, day: api::Date, today: api::Date) -> Html {
        let link = ctx.link();
        let selection = ctx.props().selection;

        let mut items = self
            .schedule_index
            .get(&day)
            .map(|&i| self.schedule[i].items())
            .unwrap_or_default();
        items.retain(|i| selection.contains(i.kind()));

        let on_navigate = link.callback(Msg::Navigate);
        let on_hover = link.callback(Msg::Hover);

        html! {
            <section key={day.to_string()} class="agenda-day">
                { view_day_heading(day, today) }

                if self.loading {
                    <Skeleton class="line" />
                } else if items.is_empty() {
                    <p class="text-muted">{"Nothing airs"}</p>
                } else {
                    <div class="agenda-items">
                        { for items.iter().map(|item| view_schedule_item(item, &self.time, &on_navigate, &on_hover)) }
                    </div>
                }
            </section>
        }
    }

    fn load_schedule(&mut self, ctx: &Context<Self>, show_loading: bool) {
        if self.channel.id() == ws::ChannelId::NONE {
            return;
        }

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
