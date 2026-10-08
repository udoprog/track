use musli_web::web03::prelude::*;
use web_sys::{Event, HtmlInputElement, HtmlSelectElement, InputEvent, MouseEvent};
use yew::prelude::*;

use crate::SetupChannel;
use crate::background::Background;
use crate::error::{CustomContext, Error, Message};

use super::{Button, Modal, Variant};

#[derive(Properties, PartialEq)]
pub(crate) struct Props {
    pub(crate) show_id: api::ShowId,
    /// The show's saved ranges; `None` (automatic) starts from a suggestion.
    pub(crate) numbering: Option<api::Numbering>,
    pub(crate) on_close: Callback<()>,
}

#[derive(Clone, Copy)]
pub(crate) enum Field {
    Season,
    First,
    Last,
    TargetSeason,
    TargetFirst,
}

pub(crate) enum Msg {
    Channel(Result<ws::Channel, ws::Error>),
    Loaded(Result<ws::Packet<api::GetShowNumbering>, ws::Error>),
    Set(usize, Field, String),
    SetSystem(usize, String),
    Add,
    Remove(usize),
    Suggest,
    Save,
    Saved(Result<ws::Packet<api::SetShowNumbering>, ws::Error>),
}

/// A range as typed: numbers stay text until they parse.
#[derive(Clone)]
struct Row {
    season: String,
    first: String,
    last: String,
    system: String,
    target_season: String,
    target_first: String,
}

impl Row {
    fn new(r: &api::NumberingRange) -> Self {
        Self {
            season: r.season.to_string(),
            first: r.first.to_string(),
            last: r.last.to_string(),
            system: r.system.clone(),
            target_season: r.target_season.to_string(),
            target_first: r.target_first.to_string(),
        }
    }

    fn parse(&self) -> Option<api::NumberingRange> {
        let n = |s: &str| s.trim().parse::<u32>().ok();

        Some(api::NumberingRange {
            season: n(&self.season)?,
            first: n(&self.first)?,
            last: n(&self.last)?,
            system: self.system.clone(),
            target_season: n(&self.target_season)?,
            target_first: n(&self.target_first)?,
        })
    }

    fn field(&mut self, field: Field) -> &mut String {
        match field {
            Field::Season => &mut self.season,
            Field::First => &mut self.first,
            Field::Last => &mut self.last,
            Field::TargetSeason => &mut self.target_season,
            Field::TargetFirst => &mut self.target_first,
        }
    }
}

/// The editor for a show's manual numbering: ranges of the show's episodes,
/// each mapped one to one onto the start of a season in an XEM numbering.
pub(crate) struct NumberingEditor {
    channel: ws::Channel,
    _setup: SetupChannel,
    _load_req: ws::Request,
    _save_req: ws::Request,
    data: Option<api::ShowNumbering>,
    rows: Vec<Row>,
    /// Problems the server reported on the last save.
    rejected: Vec<api::RangeError>,
    saving: bool,
    background: Background,
}

impl Component for NumberingEditor {
    type Message = Msg;
    type Properties = Props;

    fn create(ctx: &Context<Self>) -> Self {
        let (ws, _) = ctx
            .link()
            .context::<ws::Handle>(Callback::noop())
            .expect("Expected ws::Handle in context");

        let (background, _) = ctx
            .link()
            .context::<Background>(Callback::noop())
            .expect("Expected Background in context");

        let rows = ctx
            .props()
            .numbering
            .iter()
            .flat_map(|n| &n.ranges)
            .map(Row::new)
            .collect();

        Self {
            channel: ws::Channel::default(),
            _setup: SetupChannel::new(ws, ctx.link().callback(Msg::Channel)),
            _load_req: ws::Request::default(),
            _save_req: ws::Request::default(),
            data: None,
            rows,
            rejected: Vec::new(),
            saving: false,
            background,
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
        let link = ctx.link();
        let (ranges, errors) = self.check();
        let suggest_system = self.suggest_system();
        let can_suggest = self
            .data
            .as_ref()
            .is_some_and(|d| !d.system(suggest_system).is_empty());

        let on_close = ctx.props().on_close.clone();

        html! {
            <Modal icon="adjustments-horizontal" title={html!("Episode numbering")} class="numbering-modal" on_close={on_close.clone()}>
                <p class="hint numbering-intro">
                    {"Map each range of TMDB episodes onto the start of a season in another numbering. The range continues one to one, so its end follows from its length."}
                </p>

                <div class="numbering-ranges">
                    for (index, row) in self.rows.iter().enumerate() {
                        { self.view_row(ctx, index, row, &errors) }
                    }
                </div>

                <div class="numbering-tools">
                    <Button icon="plus" label="Add range" title="Add range" onclick={link.callback(|_: MouseEvent| Msg::Add)} />
                    <Button
                        icon="sparkles"
                        label="Suggest from episode order"
                        title={format!("Suggest from episode order ({})", api::xem_system_label(suggest_system))}
                        disabled={!can_suggest}
                        onclick={link.callback(|_: MouseEvent| Msg::Suggest)}
                    />
                </div>

                { self.view_coverage(&ranges) }

                <div class="numbering-actions">
                    <Button icon="x-mark" label="Cancel" title="Cancel" onclick={on_close.reform(|_: MouseEvent| ())} />
                    <Button icon="check" label="Save" title="Save" variant={Variant::Primary} spin={self.saving} disabled={!errors.is_empty() || self.saving} onclick={link.callback(|_: MouseEvent| Msg::Save)} />
                </div>
            </Modal>
        }
    }
}

impl NumberingEditor {
    fn try_update(&mut self, ctx: &Context<Self>, msg: Msg) -> Result<bool, Error> {
        match msg {
            Msg::Channel(result) => {
                self.channel = result?;

                if self.channel.id() != ws::ChannelId::NONE {
                    self._load_req = self
                        .channel
                        .request()
                        .body(api::GetShowNumberingRequest {
                            id: ctx.props().show_id,
                        })
                        .on_packet(ctx.link().callback(Msg::Loaded))
                        .send();
                }

                Ok(false)
            }
            Msg::Loaded(result) => {
                let data = result
                    .context(Message::LoadingNumbering)?
                    .decode()
                    .context(Message::LoadingNumbering)?;
                self.data = Some(data);

                if ctx.props().numbering.is_none() && self.rows.is_empty() {
                    self.suggest();
                }

                Ok(true)
            }
            Msg::Set(index, field, value) => {
                if let Some(row) = self.rows.get_mut(index) {
                    *row.field(field) = value;
                }

                self.rejected.clear();
                Ok(true)
            }
            Msg::SetSystem(index, system) => {
                if let Some(row) = self.rows.get_mut(index) {
                    row.system = system;
                }

                self.rejected.clear();
                Ok(true)
            }
            Msg::Add => {
                // Continue where the last range ends.
                let row = match self.rows.last().and_then(Row::parse) {
                    Some(last) => {
                        let next = |n: u32| (n + 1).to_string();

                        Row {
                            season: last.season.to_string(),
                            first: next(last.last),
                            last: next(last.last),
                            system: last.system.clone(),
                            target_season: last.target_season.to_string(),
                            target_first: next(last.target_last()),
                        }
                    }
                    None => Row::new(&api::NumberingRange {
                        season: 1,
                        first: 1,
                        last: 1,
                        system: "tvdb".to_owned(),
                        target_season: 1,
                        target_first: 1,
                    }),
                };

                self.rows.push(row);
                self.rejected.clear();
                Ok(true)
            }
            Msg::Remove(index) => {
                if index < self.rows.len() {
                    self.rows.remove(index);
                }

                self.rejected.clear();
                Ok(true)
            }
            Msg::Suggest => {
                self.suggest();
                self.rejected.clear();
                Ok(true)
            }
            Msg::Save => {
                let (ranges, errors) = self.check();

                if !errors.is_empty() || self.channel.id() == ws::ChannelId::NONE {
                    return Ok(false);
                }

                self.saving = true;
                self._save_req = self
                    .channel
                    .request()
                    .body(api::SetShowNumberingRequest {
                        id: ctx.props().show_id,
                        numbering: Some(api::Numbering {
                            ranges: ranges.into_iter().flatten().collect(),
                        }),
                    })
                    .on_packet(ctx.link().callback(Msg::Saved))
                    .send();
                Ok(true)
            }
            Msg::Saved(result) => {
                self.saving = false;
                let response = result
                    .context(Message::SettingNumbering)?
                    .decode()
                    .context(Message::SettingNumbering)?;

                if response.errors.is_empty() {
                    ctx.props().on_close.emit(());
                    return Ok(false);
                }

                self.rejected = response.errors;
                Ok(true)
            }
        }
    }

    /// The system suggestions pair episodes with: the first range's, else
    /// TheTVDB.
    fn suggest_system(&self) -> &str {
        self.rows.first().map_or("tvdb", |r| r.system.as_str())
    }

    fn suggest(&mut self) {
        let Some(data) = &self.data else {
            return;
        };

        let system = self.suggest_system().to_owned();
        let n = api::suggest_numbering(&data.episodes, &system, data.system(&system));
        self.rows = n.ranges.iter().map(Row::new).collect();
    }

    /// Each row's parsed range, and every problem as (row, message).
    fn check(&self) -> (Vec<Option<api::NumberingRange>>, Vec<(usize, String)>) {
        let parsed = self.rows.iter().map(Row::parse).collect::<Vec<_>>();
        let mut errors = Vec::new();

        for (index, r) in parsed.iter().enumerate() {
            if r.is_none() {
                errors.push((index, "Enter whole numbers.".to_owned()));
            }
        }

        // Validate the ranges that parsed, mapping their positions back to rows.
        let rows = parsed
            .iter()
            .enumerate()
            .filter_map(|(i, r)| Some((i, r.clone()?)))
            .collect::<Vec<_>>();

        let n = api::Numbering {
            ranges: rows.iter().map(|(_, r)| r.clone()).collect(),
        };

        for e in n
            .validate()
            .into_iter()
            .chain(self.rejected.iter().cloned())
        {
            if let Some((row, _)) = rows.get(e.index as usize) {
                errors.push((*row, e.message));
            }
        }

        (parsed, errors)
    }

    fn view_row(
        &self,
        ctx: &Context<Self>,
        index: usize,
        row: &Row,
        errors: &[(usize, String)],
    ) -> Html {
        let link = ctx.link();
        let messages = errors
            .iter()
            .filter(|(i, _)| *i == index)
            .map(|(_, m)| m.clone())
            .collect::<Vec<_>>();

        let input = |field: Field, value: &str, title: String| {
            let oninput = link.callback(move |e: InputEvent| {
                let input: HtmlInputElement = e.target_unchecked_into();
                Msg::Set(index, field, input.value())
            });

            html! {
                <input class="input-number" type="text" inputmode="numeric" value={value.to_owned()} aria-label={title.clone()} {title} {oninput} />
            }
        };

        let n = index + 1;

        let end = row
            .parse()
            .filter(|r| r.first <= r.last && r.target_first > 0)
            .map(|r| format!("– E{}", r.target_last()));

        let on_system = link.callback(move |e: Event| {
            let select: HtmlSelectElement = e.target_unchecked_into();
            Msg::SetSystem(index, select.value())
        });

        let known = api::XEM_SYSTEMS.iter().any(|(name, _)| *name == row.system);

        html! {
            <div class={classes!("numbering-range", (!messages.is_empty()).then_some("invalid"))} role="group" aria-label={format!("Range {n}")}>
                <div class="numbering-source">
                    <span class="logo tmdb" title="TMDB" />
                    <span class="numbering-letter">{"S"}</span>
                    { input(Field::Season, &row.season, format!("Range {n} season")) }
                    <span class="numbering-letter">{"E"}</span>
                    { input(Field::First, &row.first, format!("Range {n} first episode")) }
                    <span class="numbering-letter">{"–"}</span>
                    { input(Field::Last, &row.last, format!("Range {n} last episode")) }
                    <span class="icon arrow-long-right" aria-hidden="true" />
                </div>

                <div class="numbering-target">
                    <select class="input-select" title={format!("Range {n} numbering")} onchange={on_system}>
                        if !known {
                            <option value={row.system.clone()} selected=true>{row.system.clone()}</option>
                        }

                        for (name, label) in api::XEM_SYSTEMS.iter().copied() {
                            <option value={name} selected={row.system == name}>{label}</option>
                        }
                    </select>
                    <span class="numbering-letter">{"S"}</span>
                    { input(Field::TargetSeason, &row.target_season, format!("Range {n} target season")) }
                    <span class="numbering-letter">{"E"}</span>
                    { input(Field::TargetFirst, &row.target_first, format!("Range {n} target first episode")) }
                    <span class="numbering-end">{end}</span>
                </div>

                <Button class="numbering-remove" icon="trash" title={format!("Remove range {n}")} onclick={link.callback(move |_: MouseEvent| Msg::Remove(index))} />

                if !messages.is_empty() {
                    <div class="numbering-errors" role="alert">
                        for m in messages {
                            <span>{m}</span>
                        }
                    </div>
                }
            </div>
        }
    }

    /// A bar per season showing which of its episodes each range covers.
    fn view_coverage(&self, ranges: &[Option<api::NumberingRange>]) -> Html {
        let episodes = self
            .data
            .as_ref()
            .map(|d| d.episodes.as_slice())
            .unwrap_or_default();

        let mut seasons = episodes
            .iter()
            .map(|&(s, _)| s)
            .chain(ranges.iter().flatten().map(|r| r.season))
            .collect::<Vec<_>>();

        // Regular seasons in order, then specials.
        seasons.sort_by_key(|&s| (s == 0, s));
        seasons.dedup();

        if seasons.is_empty() {
            return Html::default();
        }

        html! {
            <div class="numbering-coverage">
                for season in seasons {
                    { view_season(season, episodes, ranges) }
                }
            </div>
        }
    }
}

fn view_season(
    season: u32,
    episodes: &[(u32, u32)],
    ranges: &[Option<api::NumberingRange>],
) -> Html {
    let mut ranges = ranges
        .iter()
        .flatten()
        .filter(|r| r.season == season && r.first >= 1 && r.first <= r.last)
        .collect::<Vec<_>>();
    ranges.sort_by_key(|r| r.first);

    let count = episodes.iter().filter(|&&(s, _)| s == season).count() as u32;
    let total = ranges.iter().map(|r| r.last).fold(count, u32::max).max(1);

    let label = if season == 0 {
        "Specials".to_owned()
    } else {
        format!("TMDB S{season}")
    };

    let mut previous: Option<&str> = None;

    let segments = ranges
        .iter()
        .enumerate()
        .map(|(i, r)| {
            let left = f64::from(r.first - 1) * 100.0 / f64::from(total);
            let width = f64::from(r.last - r.first + 1) * 100.0 / f64::from(total);
            let span = format!("S{} · {}–{}", r.target_season, r.target_first, r.target_last());

            let text = if previous == Some(r.system.as_str()) {
                span
            } else {
                format!("{} {span}", api::xem_system_label(&r.system))
            };

            previous = Some(&r.system);

            html! {
                <span class={classes!("numbering-segment", format!("tone-{}", i % 4))} style={format!("left: {left:.3}%; width: {width:.3}%")} title={format!("E{}–E{} → {text}", r.first, r.last)}>
                    {text}
                </span>
            }
        })
        .collect::<Html>();

    html! {
        <>
            <span class="numbering-season">{label}</span>
            <div class="numbering-bar">
                if ranges.is_empty() {
                    <span class="numbering-unmapped">{"not mapped: no other numbering shown"}</span>
                } else {
                    {segments}
                }
            </div>
        </>
    }
}
