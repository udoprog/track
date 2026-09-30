//! Anchored date/time picker (`MarkTimeMenu`) used by the mark-watched and
//! mark-pending flows: a "Now" preset, an optional caller-supplied
//! [`TimePreset`], and a custom date and time in native fields.

use web_sys::{Event, HtmlInputElement};
use yew::prelude::*;

use api::TimeInfo;

use crate::ui::{Button, ContextMenu, Variant};

/// A quick preset that loads an instant into the picker without submitting.
#[derive(Clone, Copy, PartialEq)]
pub(crate) enum Preset {
    /// The current time.
    Now,
    /// The caller-supplied [`TimePreset`].
    Supplied,
    /// A custom time.
    Custom,
}

/// A caller-supplied quick option shown alongside "Now". Built via [`TimePreset::at`]
/// for an exact instant or [`TimePreset::when_aired`] for an air/release date that
/// the server resolves per item (bulk flows).
#[derive(Clone, PartialEq)]
pub(crate) struct TimePreset {
    icon: AttrValue,
    label: AttrValue,
    kind: TimePresetKind,
}

#[derive(Clone, PartialEq)]
enum TimePresetKind {
    /// An exact instant: pre-fills the picker, confirms as `MarkTime::At`.
    At(api::Timestamp),
    /// Air/release date resolved per-item server-side: no pre-fill, confirms as
    /// `MarkTime::WhenAired`. Used by bulk flows. Carries a caller-supplied
    /// description of what instant each item resolves to (the menu has no access to
    /// the per-item air dates, so the wording must be passed in).
    WhenAired { description: AttrValue },
}

impl TimePreset {
    /// A preset that loads an exact instant into the picker.
    pub(crate) fn at(
        icon: impl Into<AttrValue>,
        label: impl Into<AttrValue>,
        timestamp: api::Timestamp,
    ) -> Self {
        Self {
            icon: icon.into(),
            label: label.into(),
            kind: TimePresetKind::At(timestamp),
        }
    }

    /// A preset that defers to each item's own server-resolved air/release date.
    /// `description` explains which instant that resolves to (e.g. "When each
    /// episode in Season 1 aired"), shown once selected.
    pub(crate) fn when_aired(
        icon: impl Into<AttrValue>,
        label: impl Into<AttrValue>,
        description: impl Into<AttrValue>,
    ) -> Self {
        Self {
            icon: icon.into(),
            label: label.into(),
            kind: TimePresetKind::WhenAired {
                description: description.into(),
            },
        }
    }
}

#[derive(Properties, PartialEq)]
pub(crate) struct Props {
    /// Inner content of the trigger button (icons, labels). The component wraps
    /// it in a `<button>` that opens the popover. Unused when `quick`.
    #[prop_or_default]
    pub(crate) children: Children,
    /// Classes for the trigger button (e.g. `"success"`).
    #[prop_or_default]
    pub(crate) class: Classes,
    #[prop_or_default]
    pub(crate) icon: Option<AttrValue>,
    #[prop_or_default]
    pub(crate) title: AttrValue,
    /// Heading shown at the top of the popover.
    pub(crate) prompt: AttrValue,
    /// An optional caller-supplied quick option shown alongside "Now".
    #[prop_or_default]
    pub(crate) preset: Option<TimePreset>,
    /// The trigger confirms `Now` straight away, and a narrow button beside it
    /// opens the popover for choosing another time.
    #[prop_or_default]
    pub(crate) quick: bool,
    /// With `quick`, the trigger's label on mobile.
    #[prop_or_default]
    pub(crate) text: Option<AttrValue>,
    pub(crate) on_confirm: Callback<api::MarkTime>,
}

pub(crate) enum Msg {
    Open,
    Close,
    Confirm,
    ConfirmNow,
    SetTime(TimeInfo),
    SelectPreset(Preset),
    SetDate(String),
    SetClock(String),
}

/// A trigger button that opens an anchored popover for choosing a
/// [`api::MarkTime`]: a "Now" preset, an optional caller-supplied
/// [`TimePreset`], or a custom date and time. Shared by the "mark watched" and
/// "mark pending" flows.
pub(crate) struct MarkTimeMenu {
    /// Whether the popover is open.
    context_open: bool,
    time: TimeInfo,
    _time_handle: ContextHandle<TimeInfo>,
    /// The custom day.
    date: api::Date,
    hour: u8,
    minute: u8,
    /// The chosen preset. Drives which `MarkTime` variant is emitted on
    /// confirm.
    preset: Preset,
    /// The trigger button, anchored to by the popover.
    anchor: NodeRef,
}

impl MarkTimeMenu {
    /// Load the working date/time fields from an instant in the active timezone.
    fn load_from(&mut self, ts: api::Timestamp) {
        self.date = ts.date(self.time.clone());
        let (h, m) = ts.hour_minute(self.time.clone());
        self.hour = h;
        self.minute = m;
    }
}

/// Parse a date field's `YYYY-MM-DD`.
fn parse_date(value: &str) -> Option<api::Date> {
    let mut parts = value.splitn(3, '-');
    let year = parts.next()?.parse().ok()?;
    let month = parts.next()?.parse().ok()?;
    let day = parts.next()?.parse().ok()?;
    api::Date::new(year, month, day)
}

/// Parse a time field's `HH:MM`.
fn parse_clock(value: &str) -> Option<(u8, u8)> {
    let (hour, minute) = value.split_once(':')?;
    let hour = hour.parse().ok().filter(|h| *h < 24)?;
    let minute = minute.get(..2)?.parse().ok().filter(|m| *m < 60)?;
    Some((hour, minute))
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
            date: api::Date::today(),
            hour: 0,
            minute: 0,
            preset: Preset::Now,
            anchor: NodeRef::default(),
        };

        this.load_from(this.time.now());
        this
    }

    fn update(&mut self, ctx: &Context<Self>, msg: Self::Message) -> bool {
        match msg {
            Msg::SetTime(time) => {
                self.time = time;
                false
            }
            Msg::Open => {
                self.load_from(self.time.now());
                self.preset = Preset::Now;
                self.context_open = true;
                true
            }
            Msg::Close => {
                self.context_open = false;
                true
            }
            Msg::ConfirmNow => {
                ctx.props().on_confirm.emit(api::MarkTime::Now);
                false
            }
            Msg::Confirm => {
                let mark = match self.preset {
                    Preset::Now => api::MarkTime::Now,
                    Preset::Supplied => match ctx.props().preset.as_ref().map(|p| &p.kind) {
                        Some(TimePresetKind::At(ts)) => api::MarkTime::At(*ts),
                        Some(TimePresetKind::WhenAired { .. }) => api::MarkTime::WhenAired,
                        None => return false,
                    },
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
                    Preset::Now => Some(self.time.now()),
                    Preset::Supplied => match ctx.props().preset.as_ref().map(|p| &p.kind) {
                        Some(TimePresetKind::At(ts)) => Some(*ts),
                        Some(TimePresetKind::WhenAired { .. }) | None => None,
                    },
                    Preset::Custom => None,
                };

                // Pre-fill the custom fields when we have a concrete instant; a
                // bare "when aired" (bulk, server-resolved) just marks the preset.
                if let Some(ts) = ts {
                    self.load_from(ts);
                }

                self.preset = preset;
                true
            }
            Msg::SetDate(value) => {
                if let Some(date) = parse_date(&value) {
                    self.date = date;
                }

                false
            }
            Msg::SetClock(value) => {
                if let Some((hour, minute)) = parse_clock(&value) {
                    self.hour = hour;
                    self.minute = minute;
                }

                false
            }
        }
    }

    fn view(&self, ctx: &Context<Self>) -> Html {
        let link = ctx.link();
        let props = ctx.props();

        html! {
            <>
                if props.quick {
                    <Button icon={props.icon.clone().unwrap_or(AttrValue::Static("check"))} class={props.class.clone()} title={props.title.clone()} text={props.text.clone()} onclick={link.callback(|_| Msg::ConfirmNow)} />

                    <Button node_ref={self.anchor.clone()} icon="chevron-down" class={classes!(props.class.clone(), "mark-time-more", self.context_open.then_some("selected"))} title="Choose when" expanded={Some(self.context_open)} haspopup="dialog" onclick={link.callback(|_| Msg::Open)} />
                } else {
                    <Button node_ref={self.anchor.clone()} class={props.class.clone()} title={props.title.clone()} expanded={Some(self.context_open)} haspopup="dialog" onclick={link.callback(|_| Msg::Open)}>
                        { for props.children.iter() }
                    </Button>
                }

                if self.context_open {
                    <ContextMenu icon={props.icon.clone()} prompt={props.prompt.clone()} anchor={self.anchor.clone()} on_close={link.callback(|_| Msg::Close)}>
                        <div class="mark-time">
                            {self.view_presets(ctx)}

                            if self.preset == Preset::Custom {
                                {self.view_fields(ctx)}
                            } else {
                                <div class="mark-time-resolved">
                                    {self.view_resolved(ctx)}
                                </div>
                            }

                            <div class="mark-time-actions">
                                <Button icon="x-mark" label="Cancel" title="Cancel" onclick={link.callback(|_| Msg::Close)} />

                                <Button icon="check" label="Confirm" title="Confirm" variant={Variant::Primary} onclick={link.callback(|_| Msg::Confirm)} />
                            </div>
                        </div>
                    </ContextMenu>
                }
            </>
        }
    }
}

impl MarkTimeMenu {
    /// The custom date and time, as the browser's own fields.
    fn view_fields(&self, ctx: &Context<Self>) -> Html {
        let link = ctx.link();

        let date = format!(
            "{:04}-{:02}-{:02}",
            self.date.year(),
            self.date.month(),
            self.date.day()
        );
        let clock = format!("{:02}:{:02}", self.hour, self.minute);

        let on_date = link.callback(|e: Event| {
            let input: HtmlInputElement = e.target_unchecked_into();
            Msg::SetDate(input.value())
        });

        let on_clock = link.callback(|e: Event| {
            let input: HtmlInputElement = e.target_unchecked_into();
            Msg::SetClock(input.value())
        });

        html! {
            <div class="mark-time-fields">
                <label>
                    <span>{"Date"}</span>
                    <input type="date" class="input-text" value={date} onchange={on_date} />
                </label>

                <label>
                    <span>{"Time"}</span>
                    <input type="time" class="input-text" value={clock} onchange={on_clock} />
                </label>
            </div>
        }
    }

    /// A line saying the instant the chosen preset resolves to: a formatted
    /// timestamp for the concrete presets, or the caller-supplied description
    /// for the abstract "when aired" preset.
    fn view_resolved(&self, ctx: &Context<Self>) -> Html {
        let time = self.time.clone();

        match self.preset {
            Preset::Now => self.time.now().human_date_time(time).view(),
            Preset::Supplied => match ctx.props().preset.as_ref().map(|p| &p.kind) {
                Some(TimePresetKind::At(ts)) => ts.human_date_time(time).view(),
                Some(TimePresetKind::WhenAired { description }) => {
                    html!(<span>{description}</span>)
                }
                None => html!(),
            },
            Preset::Custom => html!(),
        }
    }

    fn view_presets(&self, ctx: &Context<Self>) -> Html {
        let link = ctx.link();
        let props = ctx.props();

        let chip = |on: bool| classes!("chip", on.then_some("selected"));

        html! {
            <div class="chips" role="group" aria-label="When">
                <Button icon="clock" label="Now" title="Now" class={chip(self.preset == Preset::Now)} pressed={Some(self.preset == Preset::Now)} onclick={link.callback(|_| Msg::SelectPreset(Preset::Now))} />

                {props.preset.as_ref().map(|preset| {
                    let on = self.preset == Preset::Supplied;

                    html! {
                        <Button key="preset-button" icon={preset.icon.clone()} label={preset.label.clone()} title={preset.label.clone()} class={chip(on)} pressed={Some(on)} onclick={link.callback(move |_| Msg::SelectPreset(Preset::Supplied))} />
                    }
                })}

                <Button icon="pencil-square" label="Custom" title="Custom time" class={chip(self.preset == Preset::Custom)} pressed={Some(self.preset == Preset::Custom)} onclick={link.callback(|_| Msg::SelectPreset(Preset::Custom))} />
            </div>
        }
    }
}
