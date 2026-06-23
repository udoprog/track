//! Anchored date/time picker (`MarkTimeMenu`) used by the mark-watched and
//! mark-pending flows: quick "Now"/"Aired" presets plus a round analog clock
//! and a month calendar for choosing an exact instant.

use web_sys::{Element, PointerEvent};
use yew::prelude::*;

use api::TimeInfo;

use crate::error::Error;
use crate::ui::ContextMenu;

/// Which ring of the clock is being edited.
#[derive(Clone, Copy, PartialEq)]
pub(crate) enum ClockMode {
    Hours,
    Minutes,
}

/// A quick preset that loads an instant into the picker without submitting.
#[derive(Clone, Copy, PartialEq)]
pub(crate) enum Preset {
    /// The current time.
    Now,
    /// The aired/released instant.
    Aired,
    /// A custom time.
    Custom,
}

/// Map a clock angle (degrees clockwise from 12 o'clock) and a radius (as a
/// percentage of the dial) to an `(x%, y%)` position inside the dial. Used both
/// to lay out the numbers and to draw the hand — all geometry lives here in Rust
/// so the stylesheet stays free of per-number rules.
fn polar(angle_deg: f64, radius_pct: f64) -> (f64, f64) {
    let r = angle_deg.to_radians();
    (50.0 + radius_pct * r.sin(), 50.0 - radius_pct * r.cos())
}

const OUTER_RADIUS: f64 = 40.0;
const INNER_RADIUS: f64 = 25.0;

/// A precomputed clock-face number: its value plus the (cheaply cloneable,
/// shared) label and absolute-position style. Built once so renders — including
/// every clock-drag frame — don't reallocate them.
struct ClockNode {
    value: u8,
    label: AttrValue,
    style: AttrValue,
}

impl ClockNode {
    fn new(value: u8, label: String, angle: f64, radius: f64) -> Self {
        let (x, y) = polar(angle, radius);
        Self {
            value,
            label: AttrValue::from(label),
            style: AttrValue::from(format!("left: {x}%; top: {y}%;")),
        }
    }
}

/// Per-thread cache of the constant clock/calendar labels and geometry. wasm is
/// single-threaded, so this is effectively a build-once, process-wide table
/// shared by every [`MarkTimeMenu`] instance and reused across all renders.
struct Labels {
    hours: Vec<ClockNode>,
    minutes: Vec<ClockNode>,
    /// Zero-padded `"00"..="59"`, indexed by value — covers the hour (0-23) and
    /// minute (0-59) shown in the header.
    two_digit: Box<[AttrValue]>,
    /// `"1"..="31"`, indexed by day-of-month minus one.
    days: Box<[AttrValue]>,
}

impl Labels {
    fn build() -> Self {
        let mut hours = Vec::with_capacity(24);

        for p in 0..12u8 {
            let angle = p as f64 * 30.0;
            let outer = if p == 0 { 12 } else { p };
            let inner = if p == 0 { 0 } else { 12 + p };

            hours.push(ClockNode::new(
                outer,
                format!("{outer}"),
                angle,
                OUTER_RADIUS,
            ));

            hours.push(ClockNode::new(
                inner,
                format!("{inner:02}"),
                angle,
                INNER_RADIUS,
            ));
        }

        let minutes = (0..12u8)
            .map(|p| {
                let m = p * 5;
                ClockNode::new(m, format!("{m:02}"), p as f64 * 30.0, OUTER_RADIUS)
            })
            .collect();

        let two_digit = (0..60u8)
            .map(|n| AttrValue::from(format!("{n:02}")))
            .collect();

        let days = (1..=31u8).map(|n| AttrValue::from(n.to_string())).collect();

        Self {
            hours,
            minutes,
            two_digit,
            days,
        }
    }

    fn nodes(&self, mode: ClockMode) -> &[ClockNode] {
        match mode {
            ClockMode::Hours => &self.hours,
            ClockMode::Minutes => &self.minutes,
        }
    }
}

thread_local! {
    static LABELS: Labels = Labels::build();
}

#[derive(Properties, PartialEq)]
pub(crate) struct Props {
    /// Inner content of the trigger button (icons, labels). The component wraps
    /// it in a `<button>` that opens the popover.
    pub(crate) children: Children,
    /// Classes for the trigger button (e.g. `"btn-success"`).
    #[prop_or_default]
    pub(crate) trigger_class: Classes,
    #[prop_or_default]
    pub(crate) title: AttrValue,
    /// Heading shown at the top of the popover.
    pub(crate) prompt: AttrValue,
    /// Label for the "when aired" quick option. Defaults to "Aired"; movies pass
    /// "Released" since "aired" reads oddly for them.
    #[prop_or(AttrValue::Static("Aired"))]
    pub(crate) aired_label: AttrValue,
    /// The aired/released instant, used by the "Aired" quick option to pre-fill
    /// the picker. When present the "Aired" option is shown.
    #[prop_or_default]
    pub(crate) default_at: Option<api::Timestamp>,
    /// Force-show the "Aired" option even without a concrete `default_at` — for
    /// bulk flows where each item resolves its own air date server-side.
    #[prop_or(false)]
    pub(crate) show_aired: bool,
    pub(crate) on_confirm: Callback<api::MarkTime>,
    /// Surfaces a positioning failure to the host page's error handler.
    pub(crate) onerror: Callback<Error>,
}

pub(crate) enum Msg {
    Open,
    Close,
    Confirm,
    SetTime(TimeInfo),
    SelectPreset(Preset),
    PrevMonth,
    NextMonth,
    PickDay(api::Date),
    SetMode(ClockMode),
    DialDown(PointerEvent),
    DialMove(PointerEvent),
    DialUp(PointerEvent),
}

/// A trigger button that opens an anchored popover for choosing a
/// [`api::MarkTime`]: quick "Now"/"Aired" presets plus a round analog clock and a
/// month calendar for an exact instant. Shared by the "mark watched" and "mark
/// pending" flows.
pub(crate) struct MarkTimeMenu {
    /// Open/position state for the popover. `false` is closed; `true` is open.
    context_open: bool,
    time: TimeInfo,
    _time_handle: ContextHandle<TimeInfo>,
    /// First day of the month shown in the calendar.
    view: api::Date,
    /// The selected day.
    date: api::Date,
    hour: u8,
    minute: u8,
    mode: ClockMode,
    /// The active quick preset, if the working value still matches one. Cleared
    /// by any manual edit. Drives which `MarkTime` variant is emitted on confirm.
    preset: Preset,
    dragging: bool,
    dial: NodeRef,
    /// The trigger button, anchored to by the popover.
    anchor: NodeRef,
}

impl MarkTimeMenu {
    /// Load the working date/time fields from an instant in the active timezone.
    fn load_from(&mut self, ts: api::Timestamp) {
        self.date = ts.date(self.time.clone());
        self.view = self.date.first_of_month();
        let (h, m) = ts.hour_minute(self.time.clone());
        self.hour = h;
        self.minute = m;
    }

    /// The hand's `(angle, radius)` for the current mode and value.
    fn hand(&self) -> (f64, f64) {
        match self.mode {
            ClockMode::Hours => match self.hour {
                0 => (0.0, INNER_RADIUS),
                12 => (0.0, OUTER_RADIUS),
                1..=11 => (self.hour as f64 * 30.0, OUTER_RADIUS),
                _ => ((self.hour - 12) as f64 * 30.0, INNER_RADIUS),
            },
            ClockMode::Minutes => (self.minute as f64 * 6.0, OUTER_RADIUS),
        }
    }

    /// Map a pointer position over the dial to the value it points at.
    fn dial_value(&mut self, e: &PointerEvent) -> Option<()> {
        let el = self.dial.cast::<Element>()?;
        let rect = el.get_bounding_client_rect();
        let cx = rect.left() + rect.width() / 2.0;
        let cy = rect.top() + rect.height() / 2.0;
        let dx = e.client_x() as f64 - cx;
        let dy = e.client_y() as f64 - cy;

        let mut ang = dx.atan2(-dy).to_degrees();
        if ang < 0.0 {
            ang += 360.0;
        }

        match self.mode {
            ClockMode::Hours => {
                let dist = (dx * dx + dy * dy).sqrt();
                let radius = rect.width().min(rect.height()) / 2.0;
                let inner = dist < radius * 0.62;
                let p = ((ang / 30.0).round() as i64).rem_euclid(12) as u8;
                self.set_hour(if inner {
                    if p == 0 { 0 } else { 12 + p }
                } else if p == 0 {
                    12
                } else {
                    p
                });
            }
            ClockMode::Minutes => {
                let m = ((ang / 6.0).round() as i64).rem_euclid(60) as u8;
                self.minute = m;
                self.preset = Preset::Custom;
            }
        }

        Some(())
    }

    fn set_hour(&mut self, hour: u8) {
        self.hour = hour;
        self.preset = Preset::Custom;
    }
}

impl Component for MarkTimeMenu {
    type Message = Msg;
    type Properties = Props;

    fn create(ctx: &Context<Self>) -> Self {
        let (time, _time_handle) = ctx
            .link()
            .context::<TimeInfo>(ctx.link().callback(Msg::SetTime))
            .expect("Expected a configured time zone");

        let mut this = Self {
            context_open: false,
            time,
            _time_handle,
            view: api::Date::today(),
            date: api::Date::today(),
            hour: 0,
            minute: 0,
            mode: ClockMode::Hours,
            preset: Preset::Now,
            dragging: false,
            dial: NodeRef::default(),
            anchor: NodeRef::default(),
        };

        this.load_from(api::Timestamp::now());
        this
    }

    fn update(&mut self, ctx: &Context<Self>, msg: Self::Message) -> bool {
        match msg {
            Msg::SetTime(time) => {
                self.time = time;
                false
            }
            Msg::Open => {
                self.load_from(api::Timestamp::now());
                self.preset = Preset::Now;
                self.mode = ClockMode::Hours;
                self.context_open = true;
                true
            }
            Msg::Close => {
                self.context_open = false;
                true
            }
            Msg::Confirm => {
                let mark = match self.preset {
                    Preset::Now => api::MarkTime::Now,
                    Preset::Aired => api::MarkTime::WhenAired,
                    Preset::Custom => match self.date.to_timestamp_at_zoned(
                        self.hour,
                        self.minute,
                        self.time.tz().clone(),
                    ) {
                        Ok(ts) => api::MarkTime::At(ts),
                        Err(_) => return true,
                    },
                };

                self.context_open = false;
                ctx.props().on_confirm.emit(mark);
                true
            }
            Msg::SelectPreset(preset) => {
                let ts = match preset {
                    Preset::Now => Some(api::Timestamp::now()),
                    Preset::Aired => ctx.props().default_at,
                    Preset::Custom => None,
                };

                // Pre-fill the clock/calendar when we have a concrete instant; a
                // bare "Aired" (bulk, server-resolved) just marks the preset.
                if let Some(ts) = ts {
                    self.load_from(ts);
                }

                self.preset = preset;
                true
            }
            Msg::PrevMonth => {
                if let Some(view) = self.view.checked_add_months(-1) {
                    self.view = view.first_of_month();
                }
                true
            }
            Msg::NextMonth => {
                if let Some(view) = self.view.checked_add_months(1) {
                    self.view = view.first_of_month();
                }
                true
            }
            Msg::PickDay(date) => {
                self.date = date;
                self.preset = Preset::Custom;
                true
            }
            Msg::SetMode(mode) => {
                self.mode = mode;
                true
            }
            Msg::DialDown(e) => {
                if e.button() != 0 {
                    return false;
                }

                if let Some(el) = self.dial.cast::<Element>() {
                    e.prevent_default();
                    let _ = el.set_pointer_capture(e.pointer_id());
                    self.dragging = true;
                    self.dial_value(&e);
                }

                true
            }
            Msg::DialMove(e) => {
                if self.dragging {
                    self.dial_value(&e);
                    return true;
                }
                false
            }
            Msg::DialUp(e) => {
                if !self.dragging {
                    return false;
                }

                if let Some(el) = self.dial.cast::<Element>() {
                    let _ = el.release_pointer_capture(e.pointer_id());
                }

                self.dragging = false;

                // Toggle the ring after a selection: hours → minutes, and back
                // again after picking minutes.
                self.mode = match self.mode {
                    ClockMode::Hours => ClockMode::Minutes,
                    ClockMode::Minutes => ClockMode::Hours,
                };

                true
            }
        }
    }

    fn view(&self, ctx: &Context<Self>) -> Html {
        let link = ctx.link();
        let props = ctx.props();

        let context_content = if self.context_open {
            html! {
                <>
                    {self.view_interaction(ctx)}

                    if self.preset == Preset::Custom {
                        <div class="mark-time-body">
                            {self.view_clock(ctx)}

                            {self.view_calendar(ctx)}
                        </div>
                    }
                </>
            }
        } else {
            html!()
        };

        html! {
            <>
                <button ref={self.anchor.clone()} class={props.trigger_class.clone()} title={props.title.clone()} onclick={link.callback(|_| Msg::Open)}>
                    { for props.children.iter() }
                </button>

                if self.context_open {
                    <ContextMenu prompt={props.prompt.clone()} anchor={self.anchor.clone()} on_close={link.callback(|_| Msg::Close)} onerror={props.onerror.clone()}>
                        {context_content}
                    </ContextMenu>
                }
            </>
        }
    }
}

impl MarkTimeMenu {
    fn view_clock(&self, ctx: &Context<Self>) -> Html {
        let link = ctx.link();

        let hour_class = classes!(
            "clickable",
            (self.mode == ClockMode::Hours).then_some("selected")
        );

        let minute_class = classes!(
            "clickable",
            (self.mode == ClockMode::Minutes).then_some("selected")
        );

        let (hand_angle, hand_radius) = self.hand();
        let current = match self.mode {
            ClockMode::Hours => self.hour,
            ClockMode::Minutes => self.minute,
        };

        LABELS.with(|labels| {
            html! {
                <div class="mark-time-clock">
                    <div class="row text-gap">
                        <span class={hour_class} onclick={link.callback(|_| Msg::SetMode(ClockMode::Hours))}>
                            { labels.two_digit[self.hour as usize].clone() }
                        </span>
                        <span>{":"}</span>
                        <span class={minute_class} onclick={link.callback(|_| Msg::SetMode(ClockMode::Minutes))}>
                            { labels.two_digit[self.minute as usize].clone() }
                        </span>
                    </div>

                    <div class="mark-time-dial" ref={self.dial.clone()} onpointerdown={link.callback(Msg::DialDown)} onpointermove={link.callback(Msg::DialMove)} onpointerup={link.callback(Msg::DialUp)}>
                        <div class="mark-time-hand" style={format!("height: {hand_radius}%; transform: rotate({hand_angle}deg);")} />
                        { for labels.nodes(self.mode).iter().map(|node| {
                            html! {
                                <span
                                    class={classes!("mark-time-number", (node.value == current).then_some("selected"))}
                                    style={node.style.clone()}>
                                    { node.label.clone() }
                                </span>
                            }
                        }) }
                    </div>
                </div>
            }
        })
    }

    fn view_calendar(&self, ctx: &Context<Self>) -> Html {
        let link = ctx.link();

        let weekdays = ["Mo", "Tu", "We", "Th", "Fr", "Sa", "Su"];
        let lead = self.view.weekday_index() as u32;
        let days = self.view.days_in_month();
        let (year, month) = (self.view.year(), self.view.month());

        html! {
            <div class="mark-time-calendar column">
                <div class="row">
                    <button class="btn" onclick={link.callback(|_| Msg::PrevMonth)} title="Previous month">
                        <span class="icon chevron-left" />
                    </button>

                    <span class="fill center">{format!("{} {year}", self.view.month_name())}</span>

                    <button class="btn end" onclick={link.callback(|_| Msg::NextMonth)} title="Next month">
                        <span class="icon chevron-right" />
                    </button>
                </div>

                <div class="mark-time-grid">
                    { for weekdays.iter().map(|d| html! {
                        <span class="mark-time-weekday text-muted">{d}</span>
                    }) }

                    { for (0..lead).map(|_| html! { <span /> }) }

                    { for (1..=days).filter_map(|day| {
                        let date = api::Date::new(year, month as i8, day as i8)?;
                        let selected = date == self.date;
                        let on_pick = link.callback(move |_| Msg::PickDay(date));
                        let label = LABELS.with(|labels| labels.days[(day - 1) as usize].clone());
                        Some(html! {
                            <span
                                class={classes!("mark-time-day", "clickable", selected.then_some("selected"))}
                                onclick={on_pick}>
                                { label }
                            </span>
                        })
                    }) }
                </div>
            </div>
        }
    }

    fn view_interaction(&self, ctx: &Context<Self>) -> Html {
        let link = ctx.link();
        let props = ctx.props();

        let now_class = classes!(
            "btn-primary",
            (self.preset == Preset::Now).then_some("selected")
        );

        let aired_class = classes!(
            "btn-primary",
            (self.preset == Preset::Aired).then_some("selected")
        );

        let custom_class = classes!("btn", (self.preset == Preset::Custom).then_some("selected"));

        html! {
            <div class="row-split">
                <div class="input-group">
                    <button class={now_class} onclick={link.callback(|_| Msg::SelectPreset(Preset::Now))}>
                        <span class="item-inline">
                            <span class="icon clock" />
                        </span>
                        <span>{"Now"}</span>
                    </button>

                    if props.default_at.is_some() || props.show_aired {
                        <button class={aired_class} onclick={link.callback(|_| Msg::SelectPreset(Preset::Aired))}>
                            <span class="item-inline">
                                <span class="icon calendar" />
                            </span>

                            <span>{&props.aired_label}</span>
                        </button>
                    }

                    <button class={custom_class} onclick={link.callback(|_| Msg::SelectPreset(Preset::Custom))}>
                        <span class="item-inline">
                            <span class="icon pencil-square" />
                        </span>

                        <span>{"Custom"}</span>
                    </button>
                </div>

                <div class="input-group end">
                    <button class="btn" onclick={link.callback(|_| Msg::Close)} title="Cancel">
                        <span class="icon x-mark" />
                    </button>

                    <button class="btn-success" onclick={link.callback(|_| Msg::Confirm)} title="Confirm">
                        <span class="icon check" />
                    </button>
                </div>
            </div>
        }
    }
}
