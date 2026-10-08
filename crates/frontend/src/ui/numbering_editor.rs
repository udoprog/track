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

/// A column of the map: the show's own (TMDB) episodes, or an XEM system's.
#[derive(Clone, PartialEq)]
pub(crate) enum Side {
    Tmdb,
    System(String),
}

impl Side {
    fn label(&self) -> &str {
        match self {
            Side::Tmdb => "TMDB",
            Side::System(system) => api::xem_system_label(system),
        }
    }

    fn logo(&self) -> &str {
        match self {
            Side::Tmdb => "tmdb",
            Side::System(system) => system,
        }
    }
}

pub(crate) enum Msg {
    Channel(Result<ws::Channel, ws::Error>),
    Loaded(Result<ws::Packet<api::GetShowNumbering>, ws::Error>),
    SetManual(bool),
    Set(usize, Field, String),
    SetSystem(usize, String),
    Select(Option<usize>),
    /// A click on an episode in a column of the map.
    Pick(Side, u32, u32),
    ToggleSpecials,
    Add,
    Remove(usize),
    /// Suggest from episode order, asking for the basis when the numberings
    /// disagree.
    Suggest,
    /// Suggest from the episode order of a system.
    SuggestFrom(String),
    CancelSuggest,
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

    /// Point the range at `system`'s `season`/`first`, keeping its length.
    fn set_target(&mut self, system: &str, season: u32, first: u32) {
        self.system = system.to_owned();
        self.target_season = season.to_string();
        self.target_first = first.to_string();
    }

    /// Set the range's span in its target to `first..=last`, which moves the
    /// end of its TMDB span to match.
    fn set_target_span(&mut self, system: &str, season: u32, first: u32, last: u32) {
        self.set_target(system, season, first);

        if let Ok(start) = self.first.trim().parse::<u32>() {
            self.last = (start + (last - first)).to_string();
        }
    }
}

/// Whether `r` can be drawn: its numbers are in order and start at 1.
fn drawable(r: &api::NumberingRange) -> bool {
    r.first >= 1 && r.first <= r.last && r.target_first >= 1
}

fn covers(r: &api::NumberingRange, season: u32, episode: u32) -> bool {
    r.season == season && (r.first..=r.last).contains(&episode)
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

/// A span of episodes as `S1 E4–E5`, or `S1 E4` for one.
fn span(season: u32, first: u32, last: u32) -> String {
    if first == last {
        format!("S{season} E{first}")
    } else {
        format!("S{season} E{first}–E{last}")
    }
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

/// One column of the map: the seasons of a numbering, laid out top to bottom.
struct Column {
    side: Side,
    seasons: Vec<ColumnSeason>,
    known: BTreeSet<(u32, u32)>,
}

impl Column {
    /// The `known` episodes, widened to the `used` spans of ranges as
    /// `(season, first, last)`. Specials are left out unless asked for or
    /// used.
    fn new(side: Side, known: &[(u32, u32)], used: &[(u32, u32, u32)], specials: bool) -> Self {
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

        Self {
            side,
            seasons,
            known,
        }
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

/// The ranges reaching each episode of a column, as `(row, TMDB code)`.
type Reach = BTreeMap<(u32, u32), Vec<(usize, (u32, u32))>>;

/// The editor for a show's episode numbering: the show's episodes beside
/// those of every numbering XEM has for it, with a band for each range
/// through all of them.
pub(crate) struct NumberingEditor {
    channel: ws::Channel,
    _setup: SetupChannel,
    _load_req: ws::Request,
    _save_req: ws::Request,
    data: Option<api::ShowNumbering>,
    links: api::XemLinks,
    manual: bool,
    rows: Vec<Row>,
    /// The system the rows were suggested from, while they are unedited.
    suggested: Option<String>,
    /// Asking which numbering to suggest from.
    asking: bool,
    selected: Option<usize>,
    /// The first episode clicked of a span the next click in the same column
    /// completes.
    anchor: Option<(Side, u32, u32)>,
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

        Self {
            channel: ws::Channel::default(),
            _setup: SetupChannel::new(ws, ctx.link().callback(Msg::Channel)),
            _load_req: ws::Request::default(),
            _save_req: ws::Request::default(),
            data: None,
            links: api::XemLinks::default(),
            manual: props.manual || props.numbering.is_some(),
            rows,
            suggested: None,
            asking: false,
            selected: None,
            anchor: None,
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

        let bases = self.bases();

        let suggest_title = match bases.as_slice() {
            [group] => format!(
                "Suggest from episode order ({})",
                api::xem_system_label(self.basis(group))
            ),
            _ => "Suggest from episode order".to_owned(),
        };

        html! {
            <Modal icon="adjustments-horizontal" title={html!("Episode numbering")} class="numbering-modal" on_close={on_close.clone()}>
                <div class="numbering-mode" role="group" aria-label="Numbering mode">
                    <Button class={classes!("chip", (!self.manual).then_some("selected"))} label="Automatic" title="Automatic: same as TheTVDB" pressed={Some(!self.manual)} onclick={link.callback(|_: MouseEvent| Msg::SetManual(false))} />
                    <Button class={classes!("chip", self.manual.then_some("selected"))} label="Manual ranges" title="Manual ranges" pressed={Some(self.manual)} onclick={link.callback(|_: MouseEvent| Msg::SetManual(true))} />
                </div>

                <div class="numbering-layout">
                    { self.view_status(ctx, &ranges, &bases, &suggest_title) }
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
                                    title={suggest_title}
                                    disabled={bases.is_empty()}
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
                self.links = api::XemLinks::new(&data.systems);
                self.data = Some(data);

                if self.manual && self.rows.is_empty() {
                    self.start_suggestion();
                }

                Ok(true)
            }
            Msg::SetManual(manual) => {
                self.manual = manual;
                self.selected = None;
                self.anchor = None;
                self.asking = false;

                if manual && self.rows.is_empty() {
                    self.start_suggestion();
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
                    row.system = system;
                }

                self.edited();
                Ok(true)
            }
            Msg::Select(index) => {
                self.selected = index.filter(|&i| i < self.rows.len());
                self.anchor = None;
                Ok(true)
            }
            Msg::Pick(side, season, episode) => {
                if !self.manual {
                    return Ok(false);
                }

                self.pick(side, season, episode);
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
                        system: self.default_system(),
                        target_season: 1,
                        target_first: 1,
                    }),
                };

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
                if !self.start_suggestion() {
                    self.manual = true;
                }

                self.rejected.clear();
                Ok(true)
            }
            Msg::SuggestFrom(system) => {
                self.manual = true;
                self.suggest(&system);
                self.rejected.clear();
                Ok(true)
            }
            Msg::CancelSuggest => {
                self.asking = false;
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
        self.suggested = None;
        self.rejected.clear();
    }

    /// The systems drawn beside TMDB, in the order of [`api::XEM_SYSTEMS`]:
    /// TheTVDB, which automatic numbering uses, every system XEM numbers the
    /// show in, and any a range targets.
    fn systems(&self) -> Vec<&'static str> {
        api::XEM_SYSTEMS
            .iter()
            .map(|&(name, _)| name)
            .filter(|&name| {
                name == "tvdb"
                    || self
                        .data
                        .as_ref()
                        .is_some_and(|d| !d.system(name).is_empty())
                    || self.rows.iter().any(|r| r.system == name)
            })
            .collect()
    }

    /// The systems a suggestion can follow, grouped by the numbering they
    /// give: one group when they all agree.
    fn bases(&self) -> Vec<Vec<String>> {
        let Some(data) = &self.data else {
            return Vec::new();
        };

        let systems = self
            .systems()
            .into_iter()
            .filter_map(|name| data.systems.iter().find(|s| s.system == name).cloned())
            .collect::<Vec<_>>();

        self.links.suggestion_bases(&data.episodes, &systems)
    }

    /// The system to suggest from in a group that agrees: the one the ranges
    /// already use, else the first.
    fn basis<'a>(&self, group: &'a [String]) -> &'a str {
        self.rows
            .first()
            .and_then(|r| group.iter().find(|s| **s == r.system))
            .or(group.first())
            .map_or("tvdb", String::as_str)
    }

    /// The system a new range targets: the one the ranges use, else the first
    /// with episodes.
    fn default_system(&self) -> String {
        if let Some(row) = self.rows.first() {
            return row.system.clone();
        }

        let data = self.data.as_ref();

        self.systems()
            .into_iter()
            .find(|name| data.is_some_and(|d| !d.system(name).is_empty()))
            .unwrap_or("tvdb")
            .to_owned()
    }

    /// Suggest ranges when the numberings agree on episode order, otherwise
    /// ask which to follow. Returns whether it asked.
    fn start_suggestion(&mut self) -> bool {
        match self.bases().as_slice() {
            [] => false,
            [group] => {
                let system = self.basis(group).to_owned();
                self.suggest(&system);
                false
            }
            _ => {
                self.asking = true;
                true
            }
        }
    }

    fn suggest(&mut self, system: &str) {
        let Some(data) = &self.data else {
            return;
        };

        let n = api::suggest_numbering(&data.episodes, system, data.system(system));
        self.rows = n.ranges.iter().map(Row::new).collect();
        self.selected = None;
        self.anchor = None;
        self.asking = false;
        self.suggested = Some(system.to_owned());
    }

    /// Where `r` puts its TMDB `episode` in a column: the TMDB code itself,
    /// its target in the range's system, or that target's code in another
    /// system that XEM links it to.
    fn address(&self, r: &api::NumberingRange, episode: u32, side: &Side) -> Option<(u32, u32)> {
        let target = r.target_first + (episode - r.first);

        match side {
            Side::Tmdb => Some((r.season, episode)),
            Side::System(system) => {
                self.links
                    .translate(&r.system, r.target_season, target, system)
            }
        }
    }

    /// Every episode of `side` the `drawn` ranges reach.
    fn reach(&self, drawn: &[(usize, api::NumberingRange)], side: &Side) -> Reach {
        let mut out = Reach::new();

        for (i, r) in drawn {
            for e in r.first..=r.last {
                if let Some(code) = self.address(r, e, side) {
                    out.entry(code).or_default().push((*i, (r.season, e)));
                }
            }
        }

        out
    }

    /// A click on episode `season`/`episode` of `side`. With a range
    /// selected, two clicks in a column set its span there: in TMDB its
    /// episodes, elsewhere the episodes they map to. Otherwise a click
    /// selects the range reaching the episode, or starts a new range there.
    fn pick(&mut self, side: Side, season: u32, episode: u32) {
        let parsed = self.rows.iter().map(Row::parse).collect::<Vec<_>>();
        let drawn = parsed
            .iter()
            .enumerate()
            .filter_map(|(i, r)| Some((i, r.clone().filter(drawable)?)))
            .collect::<Vec<_>>();

        let owners = self
            .reach(&drawn, &side)
            .remove(&(season, episode))
            .unwrap_or_default();

        if let Some(i) = self.selected {
            if let Some((s, e)) = self
                .anchor
                .as_ref()
                .filter(|(a, s, _)| *a == side && *s == season)
                .map(|&(_, s, e)| (s, e))
            {
                let (first, last) = (e.min(episode), e.max(episode));

                match &side {
                    Side::Tmdb => self.rows[i].set_span(s, first, last),
                    Side::System(system) => self.rows[i].set_target_span(system, s, first, last),
                }

                self.anchor = None;
                self.edited();
                return;
            }

            if owners.iter().all(|(o, _)| *o == i) {
                match &side {
                    Side::Tmdb => self.rows[i].set_span(season, episode, episode),
                    Side::System(system) => self.rows[i].set_target(system, season, episode),
                }

                self.anchor = Some((side, season, episode));
                self.edited();
                return;
            }
        }

        if let Some(&(o, _)) = owners.first() {
            self.selected = Some(o);
            self.anchor = None;
            return;
        }

        let range = match &side {
            Side::Tmdb => self.new_from_tmdb(&parsed, season, episode),
            Side::System(system) => self.new_from_target(&drawn, system, season, episode),
        };

        self.rows.push(Row::new(&range));
        self.selected = Some(self.rows.len() - 1);
        self.anchor = Some((side, season, episode));
        self.edited();
    }

    /// A new range from TMDB's `season`/`episode`: it continues the range
    /// holding the previous episode, or else starts at the same code in the
    /// ranges' system.
    fn new_from_tmdb(
        &self,
        parsed: &[Option<api::NumberingRange>],
        season: u32,
        episode: u32,
    ) -> api::NumberingRange {
        let previous = parsed
            .iter()
            .flatten()
            .find(|r| episode > 1 && covers(r, season, episode - 1));

        match previous {
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
                system: self.default_system(),
                target_season: season,
                target_first: episode,
            },
        }
    }

    /// A new range onto `system`'s `season`/`episode`: from the TMDB episode
    /// after the one mapped to the previous episode, else the first TMDB
    /// episode no range covers, else the same code.
    fn new_from_target(
        &self,
        drawn: &[(usize, api::NumberingRange)],
        system: &str,
        season: u32,
        episode: u32,
    ) -> api::NumberingRange {
        let side = Side::System(system.to_owned());
        let reach = self.reach(drawn, &side);

        let previous = (episode > 1)
            .then(|| reach.get(&(season, episode - 1)))
            .flatten()
            .and_then(|o| o.first())
            .map(|&(_, (s, e))| (s, e + 1));

        let unmapped = || {
            let mut episodes = self
                .data
                .iter()
                .flat_map(|d| &d.episodes)
                .copied()
                .filter(|&(s, e)| s > 0 && !drawn.iter().any(|(_, r)| covers(r, s, e)))
                .collect::<Vec<_>>();
            episodes.sort();
            episodes.first().copied()
        };

        let (s, e) = previous.or_else(unmapped).unwrap_or((season, episode));

        api::NumberingRange {
            season: s,
            first: e,
            last: e,
            system: system.to_owned(),
            target_season: season,
            target_first: episode,
        }
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

    /// XEM's regular episodes no range reaches, each told once in the first
    /// system that numbers it, as `(system, season, episodes)`.
    fn unreached(
        &self,
        drawn: &[(usize, api::NumberingRange)],
    ) -> Vec<(&'static str, u32, Vec<u32>)> {
        let Some(data) = &self.data else {
            return Vec::new();
        };

        let mut told = BTreeSet::new();

        for (_, r) in drawn {
            for e in r.target_first..=r.target_last() {
                told.extend(self.links.entry(&r.system, r.target_season, e));
            }
        }

        let mut out = Vec::new();

        for system in self.systems() {
            let mut seasons = BTreeMap::<u32, Vec<u32>>::new();

            for &(s, e) in data.system(system) {
                if s > 0
                    && let Some(entry) = self.links.entry(system, s, e)
                    && told.insert(entry)
                {
                    seasons.entry(s).or_default().push(e);
                }
            }

            for (s, episodes) in seasons {
                out.push((system, s, episodes));
            }
        }

        out
    }

    fn view_status(
        &self,
        ctx: &Context<Self>,
        ranges: &[Option<api::NumberingRange>],
        bases: &[Vec<String>],
        suggest_title: &str,
    ) -> Html {
        let link = ctx.link();

        let Some(data) = &self.data else {
            return Html::default();
        };

        let drawn = self.drawn(ranges);

        let unreached = self
            .unreached(&drawn)
            .into_iter()
            .map(|(system, s, episodes)| {
                html! {
                    <p class="hint numbering-unreached">
                        {format!("No TMDB episode maps to {} S{s} {}.", api::xem_system_label(system), runs(&episodes))}
                    </p>
                }
            })
            .collect::<Html>();

        let asking = self.view_bases(ctx, bases);

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
                            {format!("XEM has no {} S{} {}, so TMDB S{} {} get no other numbers.", api::xem_system_label(&r.system), r.target_season, runs(&missing), r.season, runs(&episodes))}
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

                        if self.asking {
                            {asking}
                        } else {
                            <div class="numbering-tools">
                                <Button icon="sparkles" label="Suggest from episode order" title={suggest_title.to_owned()} disabled={bases.is_empty()} onclick={link.callback(|_: MouseEvent| Msg::Suggest)} />
                            </div>
                        }
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

        let hint = match (self.selected, &self.anchor) {
            (Some(_), Some((side, s, e))) => format!(
                "Click the last episode of the range in {} Season {s}, or E{e} again for one episode.",
                side.label()
            ),
            (Some(_), None) => "Click two TMDB episodes to set the range's first and last, or two episodes of another numbering to map it there.".to_owned(),
            (None, _) => {
                "Click an unmapped episode in any column to start a range there, or a mapped one to edit its range."
                    .to_owned()
            }
        };

        html! {
            <div class="numbering-status">
                {asking}

                if let Some(system) = &self.suggested {
                    <p class="numbering-note">
                        <span class="icon sparkles" aria-hidden="true" />
                        {format!("Suggested from {}'s episode order. Nothing is saved until you save.", api::xem_system_label(system))}
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

    /// The numberings to suggest from when they disagree, each with what it
    /// would map.
    fn view_bases(&self, ctx: &Context<Self>, bases: &[Vec<String>]) -> Html {
        let link = ctx.link();

        let Some(data) = self.data.as_ref().filter(|_| self.asking) else {
            return Html::default();
        };

        let options = bases.iter().map(|group| {
            let system = self.basis(group).to_owned();
            let n = api::suggest_numbering(&data.episodes, &system, data.system(&system));
            let episodes = n.ranges.iter().map(|r| r.last - r.first + 1).sum::<u32>();
            let names = group
                .iter()
                .map(|s| api::xem_system_label(s))
                .collect::<Vec<_>>()
                .join(" · ");

            let ranges = match n.ranges.len() {
                1 => "1 range".to_owned(),
                n => format!("{n} ranges"),
            };

            html! {
                <Button class="numbering-basis-option" title={format!("Suggest from {}", api::xem_system_label(&system))} onclick={link.callback(move |_: MouseEvent| Msg::SuggestFrom(system.clone()))}>
                    for s in group {
                        <span class={classes!("logo", s.clone())} aria-hidden="true" />
                    }
                    <span class="numbering-basis-name">{names}</span>
                    <span class="text-muted">{format!("{episodes} episodes in {ranges}")}</span>
                </Button>
            }
        });

        html! {
            <div class="numbering-basis" role="group" aria-label="Suggest from">
                <p class="hint">{"These numberings put the episodes in different orders. Which should the suggestion follow?"}</p>

                for option in options {
                    {option}
                }

                <Button class="numbering-basis-cancel" icon="x-mark" label="Cancel" title="Cancel suggestion" onclick={link.callback(|_: MouseEvent| Msg::CancelSuggest)} />
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

        let drawn = self.drawn(ranges);

        let mut columns = vec![Column::new(
            Side::Tmdb,
            &data.episodes,
            &drawn
                .iter()
                .map(|(_, r)| (r.season, r.first, r.last))
                .collect::<Vec<_>>(),
            self.specials,
        )];

        for system in self.systems() {
            columns.push(Column::new(
                Side::System(system.to_owned()),
                data.system(system),
                &drawn
                    .iter()
                    .filter(|(_, r)| r.system == system)
                    .map(|(_, r)| (r.target_season, r.target_first, r.target_last()))
                    .collect::<Vec<_>>(),
                self.specials,
            ));
        }

        let height = columns.iter().map(Column::height).max().unwrap_or(0);
        let template = format!(
            "grid-template-columns: repeat({}, var(--numbering-col) var(--numbering-gap)) var(--numbering-col)",
            columns.len() - 1
        );

        let mut map = Vec::new();

        for (index, column) in columns.iter().enumerate() {
            let reach = self.reach(&drawn, &column.side);
            map.push(self.view_column(ctx, column, &drawn, &reach, errors));

            if let Some(next) = columns.get(index + 1) {
                map.push(self.view_bands(ctx, column, next, &drawn, errors, height));
            }
        }

        let heads = columns.iter().map(|c| {
            html! {
                <span class="numbering-map-source" title={c.side.label().to_owned()}>
                    <span class={classes!("logo", c.side.logo().to_owned())} aria-hidden="true" />
                    <span class="numbering-map-label visually-hidden">{c.side.label().to_owned()}</span>
                </span>
            }
        });

        let has_specials = columns.iter().any(|c| match &c.side {
            Side::Tmdb => data.episodes.iter().any(|&(s, _)| s == 0),
            Side::System(system) => data.system(system).iter().any(|&(s, _)| s == 0),
        });

        html! {
            <div class={classes!("numbering-map-pane", (!self.manual).then_some("read-only"))}>
                <div class="numbering-map-head" style={template.clone()}>
                    for head in heads {
                        {head}
                    }
                </div>

                <div class="numbering-map-scroll">
                    <div class="numbering-map" style={format!("{template}; height: {height}px")}>
                        for part in map {
                            {part}
                        }
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

    /// The bands between two neighbouring columns: for each range, a band per
    /// run of its episodes that runs on in both.
    fn view_bands(
        &self,
        ctx: &Context<Self>,
        left: &Column,
        right: &Column,
        drawn: &[(usize, api::NumberingRange)],
        errors: &[(usize, String)],
        height: u32,
    ) -> Html {
        let link = ctx.link();
        let mut bands = Vec::new();

        for (i, r) in drawn {
            let i = *i;
            let own = Side::System(r.system.clone());
            let touches_own = left.side == own || right.side == own;
            let missing = self.missing_targets(r);

            // Runs as (first left, last left, first right, last right, whether
            // they reach target episodes XEM doesn't know).
            let mut runs = Vec::<((u32, u32), (u32, u32), (u32, u32), (u32, u32), bool)>::new();

            for e in r.first..=r.last {
                let (Some(a), Some(b)) = (
                    self.address(r, e, &left.side),
                    self.address(r, e, &right.side),
                ) else {
                    continue;
                };

                let m = touches_own && missing.contains(&(r.target_first + (e - r.first)));

                if let Some(run) = runs.last_mut()
                    && run.1.0 == a.0
                    && run.1.1 + 1 == a.1
                    && run.3.0 == b.0
                    && run.3.1 + 1 == b.1
                    && run.4 == m
                {
                    run.1 = a;
                    run.3 = b;
                    continue;
                }

                runs.push((a, a, b, b, m));
            }

            for (a1, a2, b1, b2, missing) in runs {
                let (Some(y1), Some(y2), Some(y3), Some(y4)) = (
                    left.y(a1.0, a1.1),
                    left.y(a2.0, a2.1),
                    right.y(b1.0, b1.1),
                    right.y(b2.0, b2.1),
                ) else {
                    continue;
                };

                let (y1, y2, y3, y4) = (y1 + 1, y2 + ROW - 1, y3 + 1, y4 + ROW - 1);
                let d = format!(
                    "M0 {y1} C50 {y1} 50 {y3} 100 {y3} L100 {y4} C50 {y4} 50 {y2} 0 {y2} Z"
                );

                let class = classes!(
                    "numbering-band",
                    format!("tone-{}", i % 4),
                    (self.manual && self.selected == Some(i)).then_some("selected"),
                    (self.manual && errors.iter().any(|(e, _)| *e == i)).then_some("invalid"),
                    missing.then_some("missing"),
                );

                let onclick = self
                    .manual
                    .then(|| link.callback(move |_: MouseEvent| Msg::Select(Some(i))));

                let title = format!(
                    "{} {} ↔ {} {}",
                    left.side.label(),
                    span(a1.0, a1.1, a2.1),
                    right.side.label(),
                    span(b1.0, b1.1, b2.1)
                );

                bands.push(html! {
                    <path {class} {d} vector-effect="non-scaling-stroke" {onclick}>
                        <title>{title}</title>
                    </path>
                });
            }
        }

        html! {
            <svg class="numbering-bands" viewBox={format!("0 0 100 {}", height.max(1))} preserveAspectRatio="none" style={format!("height: {height}px")} aria-hidden="true">
                for band in bands {
                    {band}
                }
            </svg>
        }
    }

    /// A column's episodes, each marked by the ranges reaching it.
    fn view_column(
        &self,
        ctx: &Context<Self>,
        column: &Column,
        drawn: &[(usize, api::NumberingRange)],
        reach: &Reach,
        errors: &[(usize, String)],
    ) -> Html {
        let link = ctx.link();
        let side = &column.side;
        let label = side.label();
        let tmdb = *side == Side::Tmdb;

        let seasons = column.seasons.iter().map(|s| {
            let (heading, name) = if s.season == 0 {
                ("Sp".to_owned(), "Specials".to_owned())
            } else {
                (format!("S{}", s.season), format!("Season {}", s.season))
            };

            let cells = (s.lo..=s.hi).map(|e| {
                let season = s.season;
                let owners = reach.get(&(season, e)).map_or(&[][..], Vec::as_slice);
                let known = column.has(season, e);

                let state = match owners {
                    [] if tmdb && known && season > 0 => "unmapped".to_owned(),
                    [] => String::new(),
                    [(i, _)] => format!("tone-{}", i % 4),
                    _ => "overlap".to_owned(),
                };

                let code = format!("{label} S{season} E{e}");

                let title = match owners {
                    _ if !tmdb && !known && !owners.is_empty() => format!("{code}: not in XEM"),
                    [] if tmdb && known => format!("{code}: not mapped"),
                    [] if tmdb => format!("{code}: not on TMDB"),
                    [] if known => format!("{code}: no TMDB episode"),
                    [] => format!("{code}: not in XEM"),
                    [(i, (ts, te))] if tmdb => {
                        let target = drawn.iter().find(|(j, _)| j == i).and_then(|(_, r)| {
                            let system = Side::System(r.system.clone());
                            Some((system.label().to_owned(), self.address(r, *te, &system)?))
                        });

                        match target {
                            Some((system, (s, e))) => format!("{code} ↔ {system} S{s} E{e}"),
                            None => format!("{code}: not mapped"),
                        }
                    }
                    [(_, (ts, te))] => format!("{code} ↔ TMDB S{ts} E{te}"),
                    _ if tmdb => format!("{code}: in more than one range"),
                    _ => format!("{code}: reached by more than one range"),
                };

                let class = classes!(
                    "numbering-ep",
                    state,
                    (!tmdb && known && owners.is_empty()).then_some("untargeted"),
                    (!known).then_some(if tmdb || owners.is_empty() { "future" } else { "missing" }),
                    owners
                        .iter()
                        .any(|(i, _)| self.manual && self.selected == Some(*i))
                        .then_some("selected"),
                    owners
                        .iter()
                        .any(|(i, _)| self.manual && errors.iter().any(|(x, _)| x == i))
                        .then_some("invalid"),
                    self.anchor
                        .as_ref()
                        .is_some_and(|(a, s, x)| a == side && (*s, *x) == (season, e))
                        .then_some("anchor"),
                );

                let pick = side.clone();
                let onclick = link.callback(move |_: MouseEvent| Msg::Pick(pick.clone(), season, e));

                html! {
                    <button type="button" {class} aria-label={code} {title} disabled={!self.manual} {onclick}>
                        <span>{format!("E{e}")}</span>
                    </button>
                }
            });

            html! {
                <>
                    <div class="numbering-map-season" title={format!("{label} {name}: {} episodes", s.known)}>
                        {heading}<span class="text-muted">{format!(" · {}", s.known)}</span>
                    </div>
                    for cell in cells {
                        {cell}
                    }
                </>
            }
        });

        html! {
            <div class="numbering-column" data-side={side.logo().to_owned()}>
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
                    <span class="icon arrows-right-left" aria-hidden="true" />
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
