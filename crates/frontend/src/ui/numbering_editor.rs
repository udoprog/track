use std::collections::{BTreeMap, BTreeSet};

use musli_web::web03::prelude::*;
use web_sys::{Event, HtmlInputElement, HtmlSelectElement, InputEvent, MouseEvent};
use yew::prelude::*;

use crate::SetupChannel;
use crate::background::Background;
use crate::error::{CustomContext, Error, Message};

use super::{Button, Modal, Variant};

/// The height of an episode in the map, as `$numbering-row` in
/// _numbering.scss.
const ROW: u32 = 22;
/// The height of a season heading in the map, as `$numbering-head`.
const HEAD: u32 = 26;
/// How far past a season's last known episode a range is drawn.
const OVERRUN: u32 = 100;

#[derive(Properties, PartialEq)]
pub(crate) struct Props {
    pub(crate) show_id: api::ShowId,
    /// The show's saved ranges; `None` is automatic.
    pub(crate) numbering: Option<api::Numbering>,
    /// Open on manual ranges, which for an automatic show start from a
    /// suggestion.
    pub(crate) manual: bool,
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
    SetManual(bool),
    Set(usize, Field, String),
    SetSystem(usize, String),
    Select(Option<usize>),
    /// A click on the show's episode.
    PickEpisode(u32, u32),
    /// A click on an episode of the shown numbering.
    PickTarget(u32, u32),
    View(String),
    ToggleSpecials,
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

    fn set_span(&mut self, season: u32, first: u32, last: u32) {
        self.season = season.to_string();
        self.first = first.to_string();
        self.last = last.to_string();
    }
}

/// Whether `r` can be drawn: its numbers are in order and start at 1.
fn drawable(r: &api::NumberingRange) -> bool {
    r.first >= 1 && r.first <= r.last && r.target_first >= 1
}

fn covers(r: &api::NumberingRange, season: u32, episode: u32) -> bool {
    r.season == season && (r.first..=r.last).contains(&episode)
}

fn targets(r: &api::NumberingRange, system: &str, season: u32, episode: u32) -> bool {
    r.system == system
        && r.target_season == season
        && (r.target_first..=r.target_last()).contains(&episode)
}

/// The warning for a season automatic numbering gets wrong.
pub(crate) fn mismatch_message(m: &api::SeasonMismatch) -> String {
    let from = if m.episodes > m.tvdb {
        format!(
            ", so from S{:02}E{:02} on, other numberings would be wrong",
            m.season,
            m.tvdb + 1
        )
    } else {
        ", so other numberings may be wrong".to_owned()
    };

    format!(
        "TMDB Season {} has {} episodes but TheTVDB Season {} has {}{from}.",
        m.season, m.episodes, m.season, m.tvdb
    )
}

/// Sorted episode numbers as runs, such as `E1–E3, E7`.
fn runs(episodes: &[u32]) -> String {
    let mut out = Vec::new();
    let mut iter = episodes.iter().copied().peekable();

    while let Some(first) = iter.next() {
        let mut last = first;

        while iter.peek() == Some(&(last + 1)) {
            last += 1;
            iter.next();
        }

        out.push(if first == last {
            format!("E{first}")
        } else {
            format!("E{first}–E{last}")
        });
    }

    out.join(", ")
}

/// A season drawn in a column of the map: episodes `lo..=hi` under a heading
/// at `top`.
struct ColumnSeason {
    season: u32,
    lo: u32,
    hi: u32,
    /// How many of its episodes the source knows.
    known: usize,
    top: u32,
}

/// One side of the map: the seasons of a numbering, laid out top to bottom.
struct Column {
    seasons: Vec<ColumnSeason>,
    known: BTreeSet<(u32, u32)>,
}

impl Column {
    /// The `known` episodes, widened to the `used` spans of ranges as
    /// `(season, first, last)`. Specials are left out unless asked for or
    /// used.
    fn new(known: &[(u32, u32)], used: &[(u32, u32, u32)], specials: bool) -> Self {
        let known = known
            .iter()
            .copied()
            .filter(|&(s, e)| (s > 0 || specials) && e > 0)
            .collect::<BTreeSet<_>>();

        let mut spans = BTreeMap::<u32, (u32, u32, usize)>::new();

        for &(s, e) in &known {
            let v = spans.entry(s).or_insert((e, e, 0));
            v.0 = v.0.min(e);
            v.1 = v.1.max(e);
            v.2 += 1;
        }

        for &(s, first, last) in used {
            let v = spans.entry(s).or_insert((first, first, 0));
            let cap = v.1.max(first) + OVERRUN;
            v.0 = v.0.min(first);
            v.1 = v.1.max(last.min(cap));
        }

        let mut order = spans.into_iter().collect::<Vec<_>>();
        // Regular seasons in order, then specials.
        order.sort_by_key(|&(s, _)| (s == 0, s));

        let mut top = 0;
        let mut seasons = Vec::new();

        for (season, (lo, hi, known)) in order {
            seasons.push(ColumnSeason {
                season,
                lo,
                hi,
                known,
                top,
            });

            top += HEAD + (hi - lo + 1) * ROW;
        }

        Self { seasons, known }
    }

    fn height(&self) -> u32 {
        self.seasons
            .last()
            .map_or(0, |s| s.top + HEAD + (s.hi - s.lo + 1) * ROW)
    }

    /// The top of `episode` in `season`, held to the drawn episodes.
    fn y(&self, season: u32, episode: u32) -> Option<u32> {
        let s = self.seasons.iter().find(|s| s.season == season)?;
        Some(s.top + HEAD + (episode.clamp(s.lo, s.hi) - s.lo) * ROW)
    }

    fn has(&self, season: u32, episode: u32) -> bool {
        self.known.contains(&(season, episode))
    }
}

/// The editor for a show's episode numbering: the show's episodes beside
/// those of an XEM numbering, with a band for each range that links them.
pub(crate) struct NumberingEditor {
    channel: ws::Channel,
    _setup: SetupChannel,
    _load_req: ws::Request,
    _save_req: ws::Request,
    data: Option<api::ShowNumbering>,
    manual: bool,
    rows: Vec<Row>,
    /// The rows are an unedited suggestion.
    suggested: bool,
    selected: Option<usize>,
    /// The first episode clicked of a span the next click completes.
    anchor: Option<(u32, u32)>,
    /// The numbering shown beside the show's episodes in manual mode.
    view: String,
    specials: bool,
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

        let props = ctx.props();

        let rows = props
            .numbering
            .iter()
            .flat_map(|n| &n.ranges)
            .map(Row::new)
            .collect::<Vec<_>>();

        let view = rows
            .first()
            .map_or("tvdb", |r| r.system.as_str())
            .to_owned();

        Self {
            channel: ws::Channel::default(),
            _setup: SetupChannel::new(ws, ctx.link().callback(Msg::Channel)),
            _load_req: ws::Request::default(),
            _save_req: ws::Request::default(),
            data: None,
            manual: props.manual || props.numbering.is_some(),
            rows,
            suggested: false,
            selected: None,
            anchor: None,
            view,
            specials: false,
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
        let on_close = ctx.props().on_close.clone();

        let disabled = self.saving
            || if self.manual {
                !errors.is_empty()
            } else {
                ctx.props().numbering.is_none()
            };

        html! {
            <Modal icon="adjustments-horizontal" title={html!("Episode numbering")} class="numbering-modal" on_close={on_close.clone()}>
                <div class="numbering-mode" role="group" aria-label="Numbering mode">
                    <Button class={classes!("chip", (!self.manual).then_some("selected"))} label="Automatic" title="Automatic: same as TheTVDB" pressed={Some(!self.manual)} onclick={link.callback(|_: MouseEvent| Msg::SetManual(false))} />
                    <Button class={classes!("chip", self.manual.then_some("selected"))} label="Manual ranges" title="Manual ranges" pressed={Some(self.manual)} onclick={link.callback(|_: MouseEvent| Msg::SetManual(true))} />
                </div>

                <div class="numbering-layout">
                    { self.view_status(ctx, &ranges) }
                    { self.view_map(ctx, &ranges, &errors) }

                    if self.manual {
                        <div class="numbering-side">
                            <div class="numbering-ranges">
                                for (index, row) in self.rows.iter().enumerate() {
                                    { self.view_row(ctx, index, row, ranges[index].as_ref(), &errors) }
                                }
                            </div>

                            <div class="numbering-tools">
                                <Button icon="plus" label="Add range" title="Add range" onclick={link.callback(|_: MouseEvent| Msg::Add)} />
                                <Button
                                    icon="sparkles"
                                    label="Suggest from episode order"
                                    title={format!("Suggest from episode order ({})", api::xem_system_label(self.suggest_system()))}
                                    disabled={!self.can_suggest()}
                                    onclick={link.callback(|_: MouseEvent| Msg::Suggest)}
                                />
                            </div>
                        </div>
                    }
                </div>

                <div class="numbering-actions">
                    <Button icon="x-mark" label="Cancel" title="Cancel" onclick={on_close.reform(|_: MouseEvent| ())} />
                    <Button icon="check" label="Save" title="Save" variant={Variant::Primary} spin={self.saving} {disabled} onclick={link.callback(|_: MouseEvent| Msg::Save)} />
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

                if self.manual && self.rows.is_empty() {
                    self.suggest();
                }

                Ok(true)
            }
            Msg::SetManual(manual) => {
                self.manual = manual;
                self.selected = None;
                self.anchor = None;

                if manual && self.rows.is_empty() {
                    self.suggest();
                }

                Ok(true)
            }
            Msg::Set(index, field, value) => {
                if let Some(row) = self.rows.get_mut(index) {
                    *row.field(field) = value;
                }

                self.edited();
                Ok(true)
            }
            Msg::SetSystem(index, system) => {
                if let Some(row) = self.rows.get_mut(index) {
                    row.system = system.clone();
                    self.view = system;
                }

                self.edited();
                Ok(true)
            }
            Msg::Select(index) => {
                self.selected = index.filter(|&i| i < self.rows.len());
                self.anchor = None;

                if let Some(i) = self.selected {
                    self.view = self.rows[i].system.clone();
                }

                Ok(true)
            }
            Msg::PickEpisode(season, episode) => {
                if !self.manual {
                    return Ok(false);
                }

                self.pick_episode(season, episode);
                Ok(true)
            }
            Msg::PickTarget(season, episode) => {
                if !self.manual {
                    return Ok(false);
                }

                match self.selected {
                    Some(i) => {
                        let row = &mut self.rows[i];
                        row.system = self.view.clone();
                        row.target_season = season.to_string();
                        row.target_first = episode.to_string();
                        self.anchor = None;
                        self.edited();
                    }
                    None => {
                        self.selected = self.rows.iter().position(|row| {
                            row.parse()
                                .is_some_and(|r| targets(&r, &self.view, season, episode))
                        });
                    }
                }

                Ok(true)
            }
            Msg::View(system) => {
                self.view = system;
                Ok(true)
            }
            Msg::ToggleSpecials => {
                self.specials = !self.specials;
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
                        system: self.view.clone(),
                        target_season: 1,
                        target_first: 1,
                    }),
                };

                self.view = row.system.clone();
                self.rows.push(row);
                self.selected = Some(self.rows.len() - 1);
                self.anchor = None;
                self.edited();
                Ok(true)
            }
            Msg::Remove(index) => {
                if index < self.rows.len() {
                    self.rows.remove(index);
                }

                self.selected = match self.selected {
                    Some(i) if i == index => None,
                    Some(i) if i > index => Some(i - 1),
                    other => other,
                };

                self.anchor = None;
                self.edited();
                Ok(true)
            }
            Msg::Suggest => {
                self.manual = true;
                self.suggest();
                self.rejected.clear();
                Ok(true)
            }
            Msg::Save => {
                let (ranges, errors) = self.check();

                if self.manual && !errors.is_empty() || self.channel.id() == ws::ChannelId::NONE {
                    return Ok(false);
                }

                let numbering = self.manual.then(|| api::Numbering {
                    ranges: ranges.into_iter().flatten().collect(),
                });

                self.saving = true;
                self._save_req = self
                    .channel
                    .request()
                    .body(api::SetShowNumberingRequest {
                        id: ctx.props().show_id,
                        numbering,
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

    fn edited(&mut self) {
        self.suggested = false;
        self.rejected.clear();
    }

    /// A click on the show's episode `season`/`episode`. With a range
    /// selected, two clicks set its span; otherwise a click selects the range
    /// the episode is in, or starts a new range there.
    fn pick_episode(&mut self, season: u32, episode: u32) {
        let parsed = self.rows.iter().map(Row::parse).collect::<Vec<_>>();
        let owner = parsed
            .iter()
            .position(|r| r.as_ref().is_some_and(|r| covers(r, season, episode)));

        if let Some(i) = self.selected {
            if let Some((s, e)) = self.anchor
                && s == season
            {
                self.rows[i].set_span(season, e.min(episode), e.max(episode));
                self.anchor = None;
                self.edited();
                return;
            }

            if owner.is_none_or(|o| o == i) {
                self.rows[i].set_span(season, episode, episode);
                self.anchor = Some((season, episode));
                self.edited();
                return;
            }
        }

        if let Some(o) = owner {
            self.selected = Some(o);
            self.anchor = None;
            self.view = self.rows[o].system.clone();
            return;
        }

        // A new range continues the one holding the previous episode, or else
        // starts at the same code in the shown numbering.
        let previous = parsed
            .iter()
            .flatten()
            .find(|r| episode > 1 && covers(r, season, episode - 1));

        let range = match previous {
            Some(r) => api::NumberingRange {
                season,
                first: episode,
                last: episode,
                system: r.system.clone(),
                target_season: r.target_season,
                target_first: r.target_first + (episode - r.first),
            },
            None => api::NumberingRange {
                season,
                first: episode,
                last: episode,
                system: self.view.clone(),
                target_season: season,
                target_first: episode,
            },
        };

        self.view = range.system.clone();
        self.rows.push(Row::new(&range));
        self.selected = Some(self.rows.len() - 1);
        self.anchor = Some((season, episode));
        self.edited();
    }

    /// The system suggestions pair episodes with: the first range's, else
    /// TheTVDB.
    fn suggest_system(&self) -> &str {
        self.rows.first().map_or("tvdb", |r| r.system.as_str())
    }

    fn can_suggest(&self) -> bool {
        self.data
            .as_ref()
            .is_some_and(|d| !d.system(self.suggest_system()).is_empty())
    }

    fn suggest(&mut self) {
        let Some(data) = &self.data else {
            return;
        };

        let system = self.suggest_system().to_owned();
        let n = api::suggest_numbering(&data.episodes, &system, data.system(&system));
        self.rows = n.ranges.iter().map(Row::new).collect();
        self.view = system;
        self.selected = None;
        self.anchor = None;
        self.suggested = true;
    }

    /// The numbering shown beside the show's episodes.
    fn view_system(&self) -> &str {
        if self.manual { &self.view } else { "tvdb" }
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

    /// The ranges drawn, by row: the manual ones, or the season to season
    /// link automatic numbering assumes.
    fn drawn(&self, ranges: &[Option<api::NumberingRange>]) -> Vec<(usize, api::NumberingRange)> {
        if self.manual {
            return ranges
                .iter()
                .enumerate()
                .filter_map(|(i, r)| Some((i, r.clone().filter(drawable)?)))
                .collect();
        }

        let mut seasons = BTreeMap::<u32, (u32, u32)>::new();

        for &(s, e) in self.data.iter().flat_map(|d| &d.episodes) {
            if s > 0 && e > 0 {
                let v = seasons.entry(s).or_insert((e, e));
                v.0 = v.0.min(e);
                v.1 = v.1.max(e);
            }
        }

        seasons
            .into_iter()
            .enumerate()
            .map(|(i, (season, (first, last)))| {
                (
                    i,
                    api::NumberingRange {
                        season,
                        first,
                        last,
                        system: "tvdb".to_owned(),
                        target_season: season,
                        target_first: first,
                    },
                )
            })
            .collect()
    }

    /// The target episodes of `r` XEM doesn't know, when it knows the
    /// system at all.
    fn missing_targets(&self, r: &api::NumberingRange) -> Vec<u32> {
        let Some(data) = &self.data else {
            return Vec::new();
        };

        let known = data.system(&r.system);

        if known.is_empty() || !drawable(r) {
            return Vec::new();
        }

        (r.target_first..=r.target_last())
            .filter(|&e| !known.contains(&(r.target_season, e)))
            .collect()
    }

    fn view_status(&self, ctx: &Context<Self>, ranges: &[Option<api::NumberingRange>]) -> Html {
        let link = ctx.link();

        let Some(data) = &self.data else {
            return Html::default();
        };

        let drawn = self.drawn(ranges);
        let system = self.view_system();
        let label = api::xem_system_label(system);

        // The shown numbering's regular episodes no range reaches.
        let mut unreached = BTreeMap::<u32, Vec<u32>>::new();

        for &(s, e) in data.system(system) {
            if s > 0 && !drawn.iter().any(|(_, r)| targets(r, system, s, e)) {
                unreached.entry(s).or_default().push(e);
            }
        }

        let unreached = unreached
            .iter()
            .map(|(s, episodes)| {
                html! {
                    <p class="hint numbering-unreached">
                        {format!("No TMDB episode maps to {label} S{s} {}.", runs(episodes))}
                    </p>
                }
            })
            .collect::<Html>();

        if !self.manual {
            let mismatches = api::numbering_mismatches(&data.episodes, data.system("tvdb"));

            let missing = drawn
                .iter()
                .filter_map(|(_, r)| {
                    let missing = self.missing_targets(r);

                    let episodes = missing
                        .iter()
                        .map(|e| r.first + (e - r.target_first))
                        .collect::<Vec<_>>();

                    (!missing.is_empty()).then(|| html! {
                        <p class="numbering-missing">
                            {format!("XEM has no {label} S{} {}, so TMDB S{} {} get no other numbers.", r.target_season, runs(&missing), r.season, runs(&episodes))}
                        </p>
                    })
                })
                .collect::<Html>();

            return html! {
                <div class="numbering-status">
                    if mismatches.is_empty() {
                        <p class="hint">{"Automatic numbering reads the show's episode codes as TheTVDB's, and its seasons line up."}</p>
                    } else {
                        <p class="hint">{"Automatic numbering reads the show's episode codes as TheTVDB's."}</p>

                        for m in &mismatches {
                            <div class="numbering-warning" role="alert">
                                <span class="icon exclamation-triangle" aria-hidden="true" />
                                <span>{mismatch_message(m)}</span>
                            </div>
                        }

                        {missing}
                        {unreached}

                        <div class="numbering-tools">
                            <Button icon="sparkles" label="Suggest from episode order" title="Suggest from episode order (TheTVDB)" disabled={data.system("tvdb").is_empty()} onclick={link.callback(|_: MouseEvent| Msg::Suggest)} />
                        </div>
                    }
                </div>
            };
        }

        let mut unmapped = BTreeMap::<u32, Vec<u32>>::new();
        let mut regular = 0;

        for &(s, e) in &data.episodes {
            if s == 0 {
                continue;
            }

            regular += 1;

            if !ranges.iter().flatten().any(|r| covers(r, s, e)) {
                unmapped.entry(s).or_default().push(e);
            }
        }

        let mapped = regular - unmapped.values().map(Vec::len).sum::<usize>();

        let hint = match (self.selected, self.anchor) {
            (Some(_), Some((s, e))) => format!(
                "Click the last episode of the range in TMDB Season {s}, or E{e} again for one episode."
            ),
            (Some(_), None) => format!(
                "Click two TMDB episodes to set the range's first and last, and a {} episode to set where it starts.",
                api::xem_system_label(&self.view)
            ),
            (None, _) => {
                "Click an unmapped TMDB episode to start a range, or a mapped one to edit its range."
                    .to_owned()
            }
        };

        html! {
            <div class="numbering-status">
                if self.suggested {
                    <p class="numbering-note">
                        <span class="icon sparkles" aria-hidden="true" />
                        {format!("Suggested from {}'s episode order. Nothing is saved until you save.", api::xem_system_label(self.suggest_system()))}
                    </p>
                }

                <p class="hint numbering-coverage">
                    {format!("{mapped} of {regular} regular TMDB episodes are mapped.")}

                    for (s, episodes) in &unmapped {
                        {format!(" Not mapped: S{s} {}.", runs(episodes))}
                    }
                </p>

                {unreached}

                <p class="hint numbering-hint">{hint}</p>
            </div>
        }
    }

    fn view_map(
        &self,
        ctx: &Context<Self>,
        ranges: &[Option<api::NumberingRange>],
        errors: &[(usize, String)],
    ) -> Html {
        let link = ctx.link();

        let Some(data) = &self.data else {
            return html! { <div class="numbering-map-pane" /> };
        };

        let system = self.view_system();
        let label = api::xem_system_label(system);
        let drawn = self.drawn(ranges);
        let left = Column::new(
            &data.episodes,
            &drawn
                .iter()
                .map(|(_, r)| (r.season, r.first, r.last))
                .collect::<Vec<_>>(),
            self.specials,
        );

        let right = Column::new(
            data.system(system),
            &drawn
                .iter()
                .filter(|(_, r)| r.system == system)
                .map(|(_, r)| (r.target_season, r.target_first, r.target_last()))
                .collect::<Vec<_>>(),
            self.specials,
        );

        let height = left.height().max(right.height());
        let invalid = |i: usize| self.manual && errors.iter().any(|(e, _)| *e == i);

        let bands = drawn
            .iter()
            .filter(|(_, r)| r.system == system)
            .filter_map(|(i, r)| {
                let y1 = left.y(r.season, r.first)? + 1;
                let y2 = left.y(r.season, r.last)? + ROW - 1;
                let y3 = right.y(r.target_season, r.target_first)? + 1;
                let y4 = right.y(r.target_season, r.target_last())? + ROW - 1;
                let d = format!(
                    "M0 {y1} C50 {y1} 50 {y3} 100 {y3} L100 {y4} C50 {y4} 50 {y2} 0 {y2} Z"
                );

                let i = *i;
                let class = classes!(
                    "numbering-band",
                    format!("tone-{}", i % 4),
                    (self.selected == Some(i)).then_some("selected"),
                    invalid(i).then_some("invalid"),
                    (!self.missing_targets(r).is_empty()).then_some("missing"),
                );

                let onclick = self
                    .manual
                    .then(|| link.callback(move |_: MouseEvent| Msg::Select(Some(i))));

                Some(html! {
                    <path {class} {d} vector-effect="non-scaling-stroke" {onclick}>
                        <title>{format!("S{} E{}–E{} → {label} S{} E{}–E{}", r.season, r.first, r.last, r.target_season, r.target_first, r.target_last())}</title>
                    </path>
                })
            })
            .collect::<Html>();

        let systems = api::XEM_SYSTEMS
            .iter()
            .copied()
            .filter(|(name, _)| {
                *name == system
                    || !data.system(name).is_empty()
                    || self.rows.iter().any(|r| r.system == *name)
            })
            .collect::<Vec<_>>();

        let on_view = link.callback(|e: Event| {
            let select: HtmlSelectElement = e.target_unchecked_into();
            Msg::View(select.value())
        });

        let has_specials = data.episodes.iter().any(|&(s, _)| s == 0)
            || data.system(system).iter().any(|&(s, _)| s == 0);

        html! {
            <div class={classes!("numbering-map-pane", (!self.manual).then_some("read-only"))}>
                <div class="numbering-map-head">
                    <span class="numbering-map-source"><span class="logo tmdb" aria-hidden="true" />{"TMDB"}</span>
                    if self.manual {
                        <select class="input-select" title="Shown numbering" onchange={on_view}>
                            for (name, label) in systems {
                                <option value={name} selected={name == system}>{label}</option>
                            }
                        </select>
                    } else {
                        <span class="numbering-map-source"><span class="logo tvdb" aria-hidden="true" />{label}</span>
                    }
                </div>

                <div class="numbering-map-scroll">
                    <div class="numbering-map" style={format!("height: {height}px")}>
                        { self.view_left(ctx, &left, &drawn, errors) }
                        <svg class="numbering-bands" viewBox={format!("0 0 100 {}", height.max(1))} preserveAspectRatio="none" style={format!("height: {height}px")} aria-hidden="true">
                            {bands}
                        </svg>
                        { self.view_right(ctx, &right, &drawn, errors) }
                    </div>
                </div>

                if has_specials {
                    <button type="button" class="link-button numbering-specials" onclick={link.callback(|_: MouseEvent| Msg::ToggleSpecials)}>
                        { if self.specials { "Hide specials" } else { "Show specials" } }
                    </button>
                }
            </div>
        }
    }

    /// The show's episodes.
    fn view_left(
        &self,
        ctx: &Context<Self>,
        column: &Column,
        drawn: &[(usize, api::NumberingRange)],
        errors: &[(usize, String)],
    ) -> Html {
        let link = ctx.link();

        let seasons = column.seasons.iter().map(|s| {
            let label = if s.season == 0 {
                "Specials".to_owned()
            } else {
                format!("Season {}", s.season)
            };

            let cells = (s.lo..=s.hi).map(|e| {
                let season = s.season;
                let owners = drawn
                    .iter()
                    .filter(|(_, r)| covers(r, season, e))
                    .collect::<Vec<_>>();
                let known = column.has(season, e);

                let state = match owners.as_slice() {
                    [] if known && season > 0 => "unmapped".to_owned(),
                    [] => String::new(),
                    [(i, _)] => format!("tone-{}", i % 4),
                    _ => "overlap".to_owned(),
                };

                let code = format!("TMDB S{season} E{e}");

                let title = match owners.as_slice() {
                    [] if known => format!("{code}: not mapped"),
                    [] => format!("{code}: not on TMDB"),
                    [(_, r)] => format!(
                        "{code} → {} S{} E{}",
                        api::xem_system_label(&r.system),
                        r.target_season,
                        r.target_first + (e - r.first)
                    ),
                    _ => format!("{code}: in more than one range"),
                };

                let class = classes!(
                    "numbering-ep",
                    state,
                    (!known).then_some("future"),
                    owners
                        .iter()
                        .any(|(i, _)| self.manual && self.selected == Some(*i))
                        .then_some("selected"),
                    owners
                        .iter()
                        .any(|(i, _)| self.manual && errors.iter().any(|(x, _)| x == i))
                        .then_some("invalid"),
                    (self.anchor == Some((season, e))).then_some("anchor"),
                );

                html! {
                    <button type="button" {class} aria-label={code} {title} disabled={!self.manual} onclick={link.callback(move |_: MouseEvent| Msg::PickEpisode(season, e))}>
                        <span>{format!("E{e}")}</span>
                    </button>
                }
            });

            html! {
                <>
                    <div class="numbering-map-season">
                        {label}<span class="text-muted">{format!(" · {}", s.known)}</span>
                    </div>
                    for cell in cells {
                        {cell}
                    }
                </>
            }
        });

        html! {
            <div class="numbering-column">
                for season in seasons {
                    {season}
                }
            </div>
        }
    }

    /// The shown numbering's episodes.
    fn view_right(
        &self,
        ctx: &Context<Self>,
        column: &Column,
        drawn: &[(usize, api::NumberingRange)],
        errors: &[(usize, String)],
    ) -> Html {
        let link = ctx.link();
        let system = self.view_system();
        let label = api::xem_system_label(system);

        let seasons = column.seasons.iter().map(|s| {
            let heading = if s.season == 0 {
                "Specials".to_owned()
            } else {
                format!("Season {}", s.season)
            };

            let cells = (s.lo..=s.hi).map(|e| {
                let season = s.season;
                let owners = drawn
                    .iter()
                    .filter(|(_, r)| targets(r, system, season, e))
                    .collect::<Vec<_>>();
                let known = column.has(season, e);

                let state = match owners.as_slice() {
                    [] => String::new(),
                    [(i, _)] => format!("tone-{}", i % 4),
                    _ => "overlap".to_owned(),
                };

                let code = format!("{label} S{season} E{e}");

                let title = match owners.as_slice() {
                    _ if !known && !owners.is_empty() => format!("{code}: not in XEM"),
                    [] if known => format!("{code}: no TMDB episode"),
                    [] => format!("{code}: not in XEM"),
                    [(_, r)] => format!(
                        "{code} ← TMDB S{} E{}",
                        r.season,
                        r.first + (e - r.target_first)
                    ),
                    _ => format!("{code}: the target of more than one range"),
                };

                let class = classes!(
                    "numbering-ep",
                    state,
                    (known && owners.is_empty()).then_some("untargeted"),
                    (!known).then_some(if owners.is_empty() { "future" } else { "missing" }),
                    owners
                        .iter()
                        .any(|(i, _)| self.manual && self.selected == Some(*i))
                        .then_some("selected"),
                    owners
                        .iter()
                        .any(|(i, _)| self.manual && errors.iter().any(|(x, _)| x == i))
                        .then_some("invalid"),
                );

                html! {
                    <button type="button" {class} aria-label={code} {title} disabled={!self.manual} onclick={link.callback(move |_: MouseEvent| Msg::PickTarget(season, e))}>
                        <span>{format!("E{e}")}</span>
                    </button>
                }
            });

            html! {
                <>
                    <div class="numbering-map-season">
                        {heading}<span class="text-muted">{format!(" · {}", s.known)}</span>
                    </div>
                    for cell in cells {
                        {cell}
                    }
                </>
            }
        });

        html! {
            <div class="numbering-column">
                for season in seasons {
                    {season}
                }
            </div>
        }
    }

    fn view_row(
        &self,
        ctx: &Context<Self>,
        index: usize,
        row: &Row,
        range: Option<&api::NumberingRange>,
        errors: &[(usize, String)],
    ) -> Html {
        let link = ctx.link();
        let messages = errors
            .iter()
            .filter(|(i, _)| *i == index)
            .map(|(_, m)| m.clone())
            .collect::<Vec<_>>();

        let n = index + 1;
        let selected = self.selected == Some(index);

        let summary = match range.filter(|r| drawable(r)) {
            Some(r) => html! {
                <>
                    <span class="numbering-span">{format!("S{} E{}–E{}", r.season, r.first, r.last)}</span>
                    <span class="icon arrow-long-right" aria-hidden="true" />
                    <span class={classes!("logo", r.system.clone())} title={api::xem_system_label(&r.system).to_owned()} />
                    <span class="numbering-span">{format!("S{} E{}–E{}", r.target_season, r.target_first, r.target_last())}</span>
                </>
            },
            None => html! { <span class="numbering-span">{"Incomplete range"}</span> },
        };

        let missing = range.map(|r| self.missing_targets(r)).unwrap_or_default();

        let on_select =
            link.callback(move |_: MouseEvent| Msg::Select((!selected).then_some(index)));

        html! {
            <div class={classes!("numbering-range", selected.then_some("selected"), (!messages.is_empty()).then_some("invalid"))} role="group" aria-label={format!("Range {n}")}>
                <div class="numbering-range-head">
                    <button type="button" class="numbering-range-summary" title={format!("Edit range {n}")} aria-expanded={selected.to_string()} onclick={on_select}>
                        <span class={classes!("numbering-swatch", format!("tone-{}", index % 4))} aria-hidden="true" />
                        {summary}
                    </button>
                    <Button class="numbering-remove" icon="trash" title={format!("Remove range {n}")} onclick={link.callback(move |_: MouseEvent| Msg::Remove(index))} />
                </div>

                if selected {
                    { self.view_fields(ctx, index, row) }
                }

                if !messages.is_empty() {
                    <div class="numbering-errors" role="alert">
                        for m in messages {
                            <span>{m}</span>
                        }
                    </div>
                }

                if let Some(r) = range.filter(|_| !missing.is_empty()) {
                    <div class="numbering-missing">
                        {format!("XEM has no {} S{} {}.", api::xem_system_label(&r.system), r.target_season, runs(&missing))}
                    </div>
                }
            </div>
        }
    }

    /// The selected range's numbers, to type instead of clicking.
    fn view_fields(&self, ctx: &Context<Self>, index: usize, row: &Row) -> Html {
        let link = ctx.link();
        let n = index + 1;

        let input = |field: Field, value: &str, title: String| {
            let oninput = link.callback(move |e: InputEvent| {
                let input: HtmlInputElement = e.target_unchecked_into();
                Msg::Set(index, field, input.value())
            });

            html! {
                <input class="input-number" type="text" inputmode="numeric" value={value.to_owned()} aria-label={title.clone()} {title} {oninput} />
            }
        };

        let on_system = link.callback(move |e: Event| {
            let select: HtmlSelectElement = e.target_unchecked_into();
            Msg::SetSystem(index, select.value())
        });

        let known = api::XEM_SYSTEMS.iter().any(|(name, _)| *name == row.system);

        html! {
            <div class="numbering-fields">
                <div class="numbering-source">
                    <span class="logo tmdb" title="TMDB" />
                    <span class="numbering-letter">{"S"}</span>
                    { input(Field::Season, &row.season, format!("Range {n} season")) }
                    <span class="numbering-letter">{"E"}</span>
                    { input(Field::First, &row.first, format!("Range {n} first episode")) }
                    <span class="numbering-letter">{"–"}</span>
                    { input(Field::Last, &row.last, format!("Range {n} last episode")) }
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
                </div>

                <Button class="numbering-done" icon="check" label="Done" title={format!("Done editing range {n}")} onclick={link.callback(|_: MouseEvent| Msg::Select(None))} />
            </div>
        }
    }
}
