use std::collections::{BTreeMap, BTreeSet};
use std::ops::Range;

use musli_web::web03::prelude::*;
use musli_web::yew023::ContextExt;
use wasm_bindgen::JsCast as _;
use web_sys::{
    Element, Event, HtmlInputElement, HtmlSelectElement, InputEvent, MouseEvent,
    ScrollIntoViewOptions, ScrollLogicalPosition,
};
use yew::prelude::*;

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
/// How many episodes in a row of a column must look alike for the map to
/// fold them, less the first and last, which stay to be clicked.
const FOLD_RUN: u32 = 10;
/// How many range colours the map and list cycle through.
const TONES: usize = 6;

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
    Channel(ws::ChannelEvent),
    Loaded(Result<ws::Packet<api::GetShowNumbering>, ws::Error>),
    SetManual(bool),
    Set(usize, Field, String),
    /// Whether a range runs on through every later episode of its season.
    SetOpen(usize, bool),
    SetSystem(usize, String),
    /// Select a range in the list, or none.
    Select(Option<usize>),
    /// A click on a range's band in the map.
    SelectBand(usize),
    /// A click on an episode in a column of the map.
    Pick(Side, u32, u32),
    /// The map scrolled to this offset.
    MapScrolled(u32),
    /// Scroll the map to this offset, the top of a season.
    JumpTo(u32),
    ToggleSpecials,
    ToggleDetails,
    /// Unfold or fold the run of rows that map whole seasons alike.
    SetFold(Range<usize>, bool),
    /// Unfold a run of episodes `first..=last` of a season in a column of
    /// the map, and where the same ranges reach in the other columns.
    Unfold(Side, u32, u32, u32),
    /// Fold every long run of the map, or none.
    FoldAll(bool),
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

/// The side to scroll to the selected range once drawn, so it follows a
/// selection made in the other.
enum Reveal {
    Map,
    List,
}

/// A row of the map to hold in place while the map changes around it: the
/// row of episode `season`/`episode` in the column of `side`, at `top` in
/// the window.
struct Pin {
    side: String,
    season: u32,
    episode: u32,
    top: f64,
}

/// A range as typed: numbers stay text until they parse.
#[derive(Clone)]
struct Row {
    season: String,
    first: String,
    last: String,
    /// The range has no last episode; `last` is kept for closing it again.
    open: bool,
    system: String,
    target_season: String,
    target_first: String,
}

impl Row {
    fn new(r: &api::NumberingRange) -> Self {
        Self {
            season: r.season.to_string(),
            first: r.first.to_string(),
            last: r.last.map(|l| l.to_string()).unwrap_or_default(),
            open: r.last.is_none(),
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
            last: if self.open {
                None
            } else {
                Some(n(&self.last)?)
            },
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
        self.open = false;
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
            self.open = false;
        }
    }
}

/// Whether `r` can be drawn: its numbers are in order and start at 1.
fn drawable(r: &api::NumberingRange) -> bool {
    r.first >= 1 && r.last.is_none_or(|l| r.first <= l) && r.target_first >= 1
}

fn covers(r: &api::NumberingRange, season: u32, episode: u32) -> bool {
    r.season == season && episode >= r.first && r.last.is_none_or(|l| episode <= l)
}

/// The last episode of a drawn range, whose open end
/// [`NumberingEditor::drawn`] has filled in.
fn end(r: &api::NumberingRange) -> u32 {
    r.last.unwrap_or(r.first)
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

/// `r` with an open end drawn through the last episode the show or XEM
/// knows on either side, and at least through its first.
fn materialised(data: Option<&api::ShowNumbering>, r: &api::NumberingRange) -> api::NumberingRange {
    if r.last.is_some() {
        return r.clone();
    }

    let last_in = |codes: &[(u32, u32)], season: u32| {
        codes
            .iter()
            .filter(|&&(s, _)| s == season)
            .map(|&(_, e)| e)
            .max()
    };

    let own = data.and_then(|d| last_in(&d.episodes, r.season));
    let target = data
        .and_then(|d| last_in(d.system(&r.system), r.target_season))
        .filter(|&e| e >= r.target_first)
        .map(|e| r.first + (e - r.target_first));

    api::NumberingRange {
        last: Some(own.into_iter().chain(target).fold(r.first, u32::max)),
        ..r.clone()
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
    /// Runs of episodes drawn as one row each, as `(first, last)` in order.
    folds: Vec<(u32, u32)>,
}

impl ColumnSeason {
    fn rows(&self) -> u32 {
        self.hi - self.lo + 1 - self.folds.iter().map(|(a, b)| b - a).sum::<u32>()
    }

    fn bottom(&self) -> u32 {
        self.top + HEAD + self.rows() * ROW
    }

    /// The top and bottom of `episode`, held to the drawn episodes. An
    /// episode in a fold gets its share of the fold's row, so bands keep
    /// their order through it.
    fn y(&self, episode: u32) -> (f64, f64) {
        let e = episode.clamp(self.lo, self.hi);
        let mut row = e - self.lo;
        let mut share = (0, 1);

        for &(a, b) in &self.folds {
            if e > b {
                row -= b - a;
            } else if e >= a {
                row -= e - a;
                share = (e - a, b - a + 1);
                break;
            }
        }

        let top = f64::from(self.top + HEAD + row * ROW);
        let part = f64::from(ROW) / f64::from(share.1);
        (
            top + f64::from(share.0) * part,
            top + f64::from(share.0 + 1) * part,
        )
    }
}

/// One column of the map: the seasons of a numbering, laid out top to bottom.
struct Column {
    side: Side,
    seasons: Vec<ColumnSeason>,
    known: BTreeSet<(u32, u32)>,
    /// The episode after each open range's last drawn one, by row.
    tails: BTreeMap<(u32, u32), usize>,
}

impl Column {
    /// The `known` episodes, widened to the `used` spans of ranges as
    /// `(season, first, last)` and to the `tails` of open ranges. Specials
    /// are left out unless asked for or used.
    fn new(
        side: Side,
        known: &[(u32, u32)],
        used: &[(u32, u32, u32)],
        tails: BTreeMap<(u32, u32), usize>,
        specials: bool,
    ) -> Self {
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

        let tail_spans = tails.keys().map(|&(s, e)| (s, e, e));

        for (s, first, last) in used.iter().copied().chain(tail_spans) {
            let v = spans.entry(s).or_insert((first, first, 0));
            let cap = v.1.max(first) + OVERRUN;
            v.0 = v.0.min(first);
            v.1 = v.1.max(last.min(cap));
        }

        let mut order = spans.into_iter().collect::<Vec<_>>();
        // Regular seasons in order, then specials.
        order.sort_by_key(|&(s, _)| (s == 0, s));

        let seasons = order
            .into_iter()
            .map(|(season, (lo, hi, known))| ColumnSeason {
                season,
                lo,
                hi,
                known,
                top: 0,
                folds: Vec::new(),
            })
            .collect();

        let mut column = Self {
            side,
            seasons,
            known,
            tails,
        };

        column.fold(|_| Vec::new());
        column
    }

    /// Fold each season's runs as `folds` gives them, laying the seasons out
    /// again.
    fn fold(&mut self, mut folds: impl FnMut(&ColumnSeason) -> Vec<(u32, u32)>) {
        let mut top = 0;

        for s in &mut self.seasons {
            s.folds = folds(s);
            s.top = top;
            top = s.bottom();
        }
    }

    fn height(&self) -> u32 {
        self.seasons.last().map_or(0, ColumnSeason::bottom)
    }

    /// The top and bottom of `episode` in `season`, held to the drawn
    /// episodes.
    fn y(&self, season: u32, episode: u32) -> Option<(f64, f64)> {
        let s = self.seasons.iter().find(|s| s.season == season)?;
        Some(s.y(episode))
    }

    fn has(&self, season: u32, episode: u32) -> bool {
        self.known.contains(&(season, episode))
    }
}

/// `top..bottom` drawn a pixel in at both ends to part it from its
/// neighbours, or less when it is a thin share of a fold.
fn inset(top: f64, bottom: f64) -> (f64, f64) {
    let by = ((bottom - top) / 4.0).min(1.0);
    (top + by, bottom - by)
}

/// A band from `y1..y2` on the left to `y3..y4` on the right.
fn band(y1: f64, y2: f64, y3: f64, y4: f64) -> String {
    format!(
        "M0 {y1:.2} C50 {y1:.2} 50 {y3:.2} 100 {y3:.2} L100 {y4:.2} C50 {y4:.2} 50 {y2:.2} 0 {y2:.2} Z"
    )
}

/// The season among `seasons`, as `(season, top, bottom)`, whose heading or
/// episodes are at `y` in the map.
fn season_at(seasons: &[(u32, u32, u32)], y: u32) -> Option<u32> {
    seasons
        .iter()
        .find(|&&(_, top, bottom)| top <= y && y < bottom)
        .map(|&(season, ..)| season)
}

/// The ranges reaching each episode of a column, as `(row, TMDB code)`.
type Reach = BTreeMap<(u32, u32), Vec<(usize, (u32, u32))>>;

/// How an episode is drawn in a column of the map.
struct Cell {
    classes: Vec<String>,
    /// The ranges reaching it or running on over it.
    owners: Vec<usize>,
    title: String,
    /// It is where an open range runs on.
    tail: bool,
}

impl Cell {
    fn alike(&self, other: &Cell) -> bool {
        self.classes == other.classes && self.owners == other.owners
    }
}

/// The runs of season `s` to fold: those of at least [`FOLD_RUN`] episodes
/// that look alike, less their first and last and any episode `shown`.
fn map_folds(
    s: &ColumnSeason,
    cells: &BTreeMap<(u32, u32), Cell>,
    shown: impl Fn(u32) -> bool,
) -> Vec<(u32, u32)> {
    let alike = |a: u32, b: u32| match (cells.get(&(s.season, a)), cells.get(&(s.season, b))) {
        (Some(a), Some(b)) => a.alike(b),
        _ => false,
    };

    let mut out = Vec::new();
    let mut start = s.lo;

    while start <= s.hi {
        let mut end = start;

        while end < s.hi && alike(start, end + 1) {
            end += 1;
        }

        if end - start + 1 >= FOLD_RUN {
            let mut e = start + 1;

            while e < end {
                if shown(e) {
                    e += 1;
                    continue;
                }

                let first = e;

                while e < end && !shown(e) {
                    e += 1;
                }

                if e - first >= FOLD_RUN - 2 {
                    out.push((first, e - 1));
                }
            }
        }

        start = end + 1;
    }

    out
}

/// The editor for a show's episode numbering: the show's episodes beside
/// those of every numbering XEM has for it, with a band for each range
/// through all of them.
pub(crate) struct NumberingEditor {
    channel: ws::Channel,
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
    reveal: Option<Reveal>,
    map_scroll: NodeRef,
    map_head: NodeRef,
    /// How far the map is scrolled, which names the season in view in each
    /// column.
    map_top: u32,
    ranges: NodeRef,
    /// The status's warnings and unmapped episodes are unfolded.
    details: bool,
    /// The runs unfolded in the list, by their system and first season.
    unfolded: BTreeSet<(String, u32)>,
    /// Long runs of alike episodes fold in the map.
    fold: bool,
    /// Episodes of the map unfolded by hand, by column, season and episode.
    shown: BTreeSet<(String, u32, u32)>,
    pin: Option<Pin>,
    /// Problems the server reported on the last save.
    rejected: Vec<api::RangeError>,
    saving: bool,
    background: Background,
}

impl Component for NumberingEditor {
    type Message = Msg;
    type Properties = Props;

    fn create(ctx: &Context<Self>) -> Self {
        let ws = ctx.handle();

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
            channel: ws
                .channel()
                .on_change(ctx.link().callback(Msg::Channel))
                .open(),
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
            reveal: None,
            map_scroll: NodeRef::default(),
            map_head: NodeRef::default(),
            map_top: 0,
            ranges: NodeRef::default(),
            details: false,
            unfolded: BTreeSet::new(),
            fold: true,
            shown: BTreeSet::new(),
            pin: None,
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

    fn rendered(&mut self, _: &Context<Self>, _: bool) {
        self.show_in_view();

        if let Some(pin) = self.pin.take() {
            self.restore(&pin);
        }

        match self.reveal.take() {
            Some(Reveal::Map) => self.reveal_in_map(),
            Some(Reveal::List) => self.reveal_in_list(),
            None => {}
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
                            <div class="numbering-ranges" ref={self.ranges.clone()}>
                                for group in self.folds(&ranges, &errors) {
                                    if group.len() > 1 {
                                        { self.view_fold(ctx, group, &ranges, &errors) }
                                    } else {
                                        { self.view_row(ctx, group.start, &self.rows[group.start], ranges[group.start].as_ref(), &errors) }
                                    }
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
            Msg::Channel(event) => {
                if let ws::ChannelEvent::Failed(error) = event {
                    return Err(error.into());
                }

                if self.channel.is_open() {
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
                self.details = false;
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
            Msg::SetOpen(index, open) => {
                let data = self.data.as_ref();

                if let Some(row) = self.rows.get_mut(index) {
                    // Close at the last episode drawn while it was open.
                    if !open
                        && row.last.trim().is_empty()
                        && let Some(r) = row.parse()
                    {
                        row.last = end(&materialised(data, &r)).to_string();
                    }

                    row.open = open;
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
                self.reveal = self.selected.map(|_| Reveal::Map);
                Ok(true)
            }
            Msg::SelectBand(index) => {
                self.selected = Some(index).filter(|&i| i < self.rows.len());
                self.anchor = None;
                self.reveal = Some(Reveal::List);
                Ok(true)
            }
            Msg::Pick(side, season, episode) => {
                if !self.manual {
                    return Ok(false);
                }

                self.pin = self.pin_at(side.logo(), season, episode);
                self.pick(side, season, episode);
                self.reveal = self.selected.map(|_| Reveal::List);
                Ok(true)
            }
            Msg::MapScrolled(top) => {
                self.map_top = top;
                Ok(true)
            }
            Msg::JumpTo(top) => {
                let Some(scroll) = self.map_scroll.cast::<Element>() else {
                    return Ok(false);
                };

                scroll.set_scroll_top(top as i32);
                self.map_top = scroll.scroll_top().max(0) as u32;
                Ok(true)
            }
            Msg::ToggleSpecials => {
                self.specials = !self.specials;
                Ok(true)
            }
            Msg::ToggleDetails => {
                self.details = !self.details;
                Ok(true)
            }
            Msg::SetFold(group, unfold) => {
                let Some(first) = self.rows.get(group.start).and_then(Row::parse) else {
                    return Ok(false);
                };

                let key = (first.system, first.season);

                if unfold {
                    self.unfolded.insert(key);
                } else {
                    self.unfolded.remove(&key);

                    if self.selected.is_some_and(|i| group.contains(&i)) {
                        self.selected = None;
                        self.anchor = None;
                    }
                }

                Ok(true)
            }
            Msg::Unfold(side, season, first, last) => {
                self.pin = self.pin_at(side.logo(), season, first);

                let parsed = self.rows.iter().map(Row::parse).collect::<Vec<_>>();
                let drawn = self.drawn(&parsed);
                let reach = self.reach(&drawn, &side);

                let sides = [Side::Tmdb]
                    .into_iter()
                    .chain(
                        self.systems()
                            .into_iter()
                            .map(|s| Side::System(s.to_owned())),
                    )
                    .collect::<Vec<_>>();

                for e in first..=last {
                    self.shown.insert((side.logo().to_owned(), season, e));

                    for (i, (_, te)) in reach.get(&(season, e)).into_iter().flatten() {
                        let Some((_, r)) = drawn.iter().find(|(j, _)| j == i) else {
                            continue;
                        };

                        for other in &sides {
                            if let Some((s, x)) = self.address(r, *te, other) {
                                self.shown.insert((other.logo().to_owned(), s, x));
                            }
                        }
                    }
                }

                Ok(true)
            }
            Msg::FoldAll(fold) => {
                self.pin = self.pin_top();
                self.fold = fold;
                self.shown.clear();
                Ok(true)
            }
            Msg::Add => {
                // Continue where the last range ends.
                let row = match self.rows.last().and_then(Row::parse) {
                    // An open range holds the rest of its season, so go on
                    // with the next.
                    Some(last) if last.last.is_none() => Row::new(&api::NumberingRange {
                        season: last.season + 1,
                        first: 1,
                        last: Some(1),
                        system: last.system.clone(),
                        target_season: last.target_season + 1,
                        target_first: 1,
                    }),
                    Some(last) => {
                        let next = |n: u32| n + 1;

                        Row::new(&api::NumberingRange {
                            season: last.season,
                            first: next(end(&last)),
                            last: Some(next(end(&last))),
                            system: last.system.clone(),
                            target_season: last.target_season,
                            target_first: next(last.target_of(end(&last))),
                        })
                    }
                    None => Row::new(&api::NumberingRange {
                        season: 1,
                        first: 1,
                        last: Some(1),
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

                if self.manual && !errors.is_empty() || !self.channel.is_open() {
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

    /// Scroll the map to the selected range's first TMDB episode, and
    /// sideways to its first episode in the column of its system.
    fn reveal_in_map(&self) {
        let Some(scroll) = self.map_scroll.cast::<Element>() else {
            return;
        };

        let Some(row) = self.selected.and_then(|i| self.rows.get(i)) else {
            return;
        };

        let first = |side: &str| {
            scroll
                .query_selector(&format!(
                    ".numbering-column[data-side='{side}'] .numbering-ep.selected"
                ))
                .ok()
                .flatten()
                .map(|e| e.get_bounding_client_rect())
        };

        let view = scroll.get_bounding_client_rect();

        if let Some(ep) = first("tmdb")
            && (ep.top() < view.top() || ep.bottom() > view.bottom())
        {
            // Leave room for the season heading above it.
            let by = ep.top() - view.top() - f64::from(HEAD);
            scroll.set_scroll_top(scroll.scroll_top() + by as i32);
        }

        if let Some(ep) = first(&row.system) {
            let by = if ep.right() > view.right() {
                ep.right() - view.right()
            } else {
                (ep.left() - view.left()).min(0.0)
            };

            scroll.set_scroll_left(scroll.scroll_left() + by as i32);
        }
    }

    /// The row of `side`'s column in the map showing `season`/`episode`.
    fn map_row(&self, side: &str, season: u32, episode: u32) -> Option<Element> {
        let rows = self
            .map_scroll
            .cast::<Element>()?
            .query_selector_all(&format!(
                ".numbering-column[data-side='{side}'] [data-season='{season}']"
            ))
            .ok()?;

        let attr = |e: &Element, name: &str| e.get_attribute(name)?.parse::<u32>().ok();

        (0..rows.length())
            .filter_map(|i| rows.get(i)?.dyn_into::<Element>().ok())
            .find(|e| {
                attr(e, "data-first").is_some_and(|a| a <= episode)
                    && attr(e, "data-last").is_some_and(|b| episode <= b)
            })
    }

    fn pin_at(&self, side: &str, season: u32, episode: u32) -> Option<Pin> {
        let row = self.map_row(side, season, episode)?;

        Some(Pin {
            side: side.to_owned(),
            season,
            episode,
            top: row.get_bounding_client_rect().top(),
        })
    }

    /// Pin the first TMDB row wholly in view.
    fn pin_top(&self) -> Option<Pin> {
        let scroll = self.map_scroll.cast::<Element>()?;
        let top = scroll.get_bounding_client_rect().top();
        let rows = scroll
            .query_selector_all(".numbering-column[data-side='tmdb'] [data-season]")
            .ok()?;

        let row = (0..rows.length())
            .filter_map(|i| rows.get(i)?.dyn_into::<Element>().ok())
            .find(|e| e.get_bounding_client_rect().top() >= top - 0.5)?;

        let attr = |name: &str| row.get_attribute(name)?.parse::<u32>().ok();

        Some(Pin {
            side: "tmdb".to_owned(),
            season: attr("data-season")?,
            episode: attr("data-first")?,
            top: row.get_bounding_client_rect().top(),
        })
    }

    /// Scroll the map so the pinned row is back where it was.
    fn restore(&self, pin: &Pin) {
        let (Some(scroll), Some(row)) = (
            self.map_scroll.cast::<Element>(),
            self.map_row(&pin.side, pin.season, pin.episode),
        ) else {
            return;
        };

        let by = row.get_bounding_client_rect().top() - pin.top;

        if by.abs() >= 0.5 {
            scroll.set_scroll_top(scroll.scroll_top() + by.round() as i32);
        }
    }

    /// Set each season picker of the map's head to its `selected` option,
    /// which a picker stops following once it has been picked from.
    fn show_in_view(&self) {
        let Some(selects) = self
            .map_head
            .cast::<Element>()
            .and_then(|e| e.query_selector_all("select").ok())
        else {
            return;
        };

        for i in 0..selects.length() {
            let Some(select) = selects
                .get(i)
                .and_then(|n| n.dyn_into::<HtmlSelectElement>().ok())
            else {
                continue;
            };

            if let Ok(Some(option)) = select.query_selector("option[selected]") {
                select.set_value(&option.get_attribute("value").unwrap_or_default());
            }
        }
    }

    /// Scroll the selected range's row into view in the list.
    fn reveal_in_list(&self) {
        let Some(row) = self
            .ranges
            .cast::<Element>()
            .and_then(|e| e.query_selector(".numbering-range.selected").ok().flatten())
        else {
            return;
        };

        let options = ScrollIntoViewOptions::new();
        options.set_block(ScrollLogicalPosition::Nearest);
        options.set_inline(ScrollLogicalPosition::Nearest);
        row.scroll_into_view_with_scroll_into_view_options(&options);
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
        let target = r.target_of(episode);

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
            for e in r.first..=end(r) {
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
        let drawn = self.drawn(&parsed);

        let mut owners = self
            .reach(&drawn, &side)
            .remove(&(season, episode))
            .unwrap_or_default();

        // The tail past an open range's last drawn episode belongs to it.
        if owners.is_empty()
            && let Some((i, r)) = drawn.iter().find(|(i, r)| {
                self.is_open(*i)
                    && self.address(r, end(r), &side).map(|(s, e)| (s, e + 1))
                        == Some((season, episode))
            })
        {
            owners.push((*i, (r.season, end(r) + 1)));
        }

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
                last: Some(episode),
                system: r.system.clone(),
                target_season: r.target_season,
                target_first: r.target_of(episode),
            },
            None => api::NumberingRange {
                season,
                first: episode,
                last: Some(episode),
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
            last: Some(e),
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

    /// Whether the range of row `index` is open.
    fn is_open(&self, index: usize) -> bool {
        self.manual && self.rows.get(index).is_some_and(|r| r.open)
    }

    /// The ranges drawn, by row, with open ends drawn as far as either side
    /// knows episodes: the manual ones, or the season to season link
    /// automatic numbering assumes.
    fn drawn(&self, ranges: &[Option<api::NumberingRange>]) -> Vec<(usize, api::NumberingRange)> {
        if self.manual {
            return ranges
                .iter()
                .enumerate()
                .filter_map(|(i, r)| {
                    let r = r.as_ref().filter(|r| drawable(r))?;
                    Some((i, materialised(self.data.as_ref(), r)))
                })
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
                        last: Some(last),
                        system: "tvdb".to_owned(),
                        target_season: season,
                        target_first: first,
                    },
                )
            })
            .collect()
    }

    /// Whether `r` maps all of its season onto the same season of its system,
    /// episode for episode, with each side ending where the other does.
    fn season_for_season(&self, r: &api::NumberingRange) -> bool {
        let Some(data) = &self.data else {
            return false;
        };

        let last = |codes: &[(u32, u32)]| {
            codes
                .iter()
                .filter(|&&(s, _)| s == r.season)
                .map(|&(_, e)| e)
                .max()
        };

        drawable(r)
            && r.season == r.target_season
            && r.first == 1
            && r.target_first == 1
            && last(&data.episodes).is_some_and(|own| {
                own == end(&materialised(Some(data), r))
                    && last(data.system(&r.system)) == Some(own)
            })
            && self.missing_targets(r).is_empty()
    }

    /// The rows to list in order, as groups: consecutive rows that map
    /// consecutive seasons [season for season](Self::season_for_season) in
    /// one system share a group, every other row is a group of its own.
    fn folds(
        &self,
        ranges: &[Option<api::NumberingRange>],
        errors: &[(usize, String)],
    ) -> Vec<Range<usize>> {
        let alike = |i: usize| {
            ranges[i]
                .as_ref()
                .filter(|r| !errors.iter().any(|(e, _)| *e == i) && self.season_for_season(r))
        };

        let mut groups = Vec::<Range<usize>>::new();

        for i in 0..ranges.len() {
            if let Some(group) = groups.last_mut()
                && let (Some(prev), Some(r)) = (alike(group.end - 1), alike(i))
                && prev.system == r.system
                && prev.season + 1 == r.season
            {
                group.end = i + 1;
            } else {
                groups.push(i..i + 1);
            }
        }

        groups
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

        let r = materialised(Some(data), r);

        (r.target_first..=r.target_of(end(&r)))
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
            for e in r.target_first..=r.target_of(end(r)) {
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

        let unreached = self.unreached(&drawn);
        let unreached_count = unreached.len();

        let unreached = unreached
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

        let toggle = html! {
            <button type="button" class="link-button numbering-details-toggle" aria-expanded={self.details.to_string()} onclick={link.callback(|_: MouseEvent| Msg::ToggleDetails)}>
                { if self.details { "Hide details" } else { "Show details" } }
            </button>
        };

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

                        <div class="numbering-warning numbering-summary" role="alert">
                            <span class="icon exclamation-triangle" aria-hidden="true" />
                            <span>
                                { match mismatches.len() {
                                    1 => "1 season differs from TheTVDB.".to_owned(),
                                    n => format!("{n} seasons differ from TheTVDB."),
                                } }
                                {" "}
                                {toggle}
                            </span>
                        </div>

                        if self.asking {
                            {asking}
                        } else {
                            <div class="numbering-tools">
                                <Button icon="sparkles" label="Suggest from episode order" title={suggest_title.to_owned()} disabled={bases.is_empty()} onclick={link.callback(|_: MouseEvent| Msg::Suggest)} />
                            </div>
                        }

                        if self.details {
                            <div class="numbering-details">
                                for m in &mismatches {
                                    <div class="numbering-warning">
                                        <span class="icon exclamation-triangle" aria-hidden="true" />
                                        <span>{mismatch_message(m)}</span>
                                    </div>
                                }

                                {missing}
                                {unreached}
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
        let has_details = !unmapped.is_empty() || unreached_count > 0;

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

                <p class="hint">
                    <span class="numbering-coverage">{format!("{mapped} of {regular} regular TMDB episodes are mapped.")}</span>

                    if has_details {
                        {" "}
                        {toggle}
                    }
                </p>

                if self.details && has_details {
                    <div class="numbering-details">
                        for (s, episodes) in &unmapped {
                            <p class="hint numbering-unmapped">{format!("Not mapped: S{s} {}.", runs(episodes))}</p>
                        }

                        {unreached}
                    </div>
                }

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
            let episodes = data
                .episodes
                .iter()
                .filter(|&&(s, e)| s > 0 && n.target(s, e).is_some())
                .count();
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

        // Each open range runs on past its last drawn episode wherever that
        // episode is.
        let tails = |side: &Side| {
            drawn
                .iter()
                .filter(|(i, _)| self.is_open(*i))
                .filter_map(|(i, r)| {
                    let (s, e) = self.address(r, end(r), side)?;
                    Some(((s, e + 1), *i))
                })
                .collect::<BTreeMap<_, _>>()
        };

        let mut columns = vec![Column::new(
            Side::Tmdb,
            &data.episodes,
            &drawn
                .iter()
                .map(|(_, r)| (r.season, r.first, end(r)))
                .collect::<Vec<_>>(),
            tails(&Side::Tmdb),
            self.specials,
        )];

        for system in self.systems() {
            let side = Side::System(system.to_owned());
            let tails = tails(&side);

            columns.push(Column::new(
                side,
                data.system(system),
                &drawn
                    .iter()
                    .filter(|(_, r)| r.system == system)
                    .map(|(_, r)| (r.target_season, r.target_first, r.target_of(end(r))))
                    .collect::<Vec<_>>(),
                tails,
                self.specials,
            ));
        }

        // The episodes folded, and those that would be with every run folded.
        let (mut folded, mut foldable) = (0, 0);
        let count = |folds: &[(u32, u32)]| folds.iter().map(|(a, b)| b - a + 1).sum::<u32>();
        let mut parts = Vec::new();

        for column in &mut columns {
            let reach = self.reach(&drawn, &column.side);
            let cells = self.cells(column, &drawn, &reach, errors);
            let key = column.side.logo().to_owned();

            column.fold(|s| {
                foldable += count(&map_folds(s, &cells, |_| false));

                if !self.fold {
                    return Vec::new();
                }

                let folds = map_folds(s, &cells, |e| {
                    self.shown.contains(&(key.clone(), s.season, e))
                });
                folded += count(&folds);
                folds
            });

            parts.push(cells);
        }

        let height = columns.iter().map(Column::height).max().unwrap_or(0);
        let template = format!(
            "grid-template-columns: repeat({}, var(--numbering-col) var(--numbering-gap)) var(--numbering-col)",
            columns.len() - 1
        );

        let mut map = Vec::new();

        for (index, (column, cells)) in columns.iter().zip(&parts).enumerate() {
            map.push(self.view_column(ctx, column, cells));

            if let Some(next) = columns.get(index + 1) {
                map.push(self.view_bands(ctx, column, next, &drawn, errors, height));
            }
        }

        let spans = columns
            .iter()
            .map(|c| {
                c.seasons
                    .iter()
                    .map(|s| (s.season, s.top, s.bottom()))
                    .collect::<Vec<_>>()
            })
            .collect::<Vec<_>>();

        let current = spans
            .iter()
            .map(|s| season_at(s, self.map_top))
            .collect::<Vec<_>>();

        // Render only when a column's season in view changes.
        let onscroll = {
            let scroll = self.map_scroll.clone();
            let spans = spans.clone();
            let current = current.clone();

            link.batch_callback(move |_: Event| {
                let top = scroll.cast::<Element>()?.scroll_top().max(0) as u32;
                let next = spans.iter().map(|s| season_at(s, top)).collect::<Vec<_>>();
                (next != current).then_some(Msg::MapScrolled(top))
            })
        };

        let heads = columns.iter().zip(&spans).zip(&current).map(|((c, spans), current)| {
            let label = c.side.label().to_owned();
            let options = spans.clone();

            let onchange = link.batch_callback(move |e: Event| {
                let season = e.target_dyn_into::<HtmlSelectElement>()?.value().parse::<u32>().ok()?;
                let &(_, top, _) = options.iter().find(|(s, ..)| *s == season)?;
                Some(Msg::JumpTo(top))
            });

            html! {
                <span class="numbering-map-source" title={label.clone()}>
                    <span class={classes!("logo", c.side.logo().to_owned())} aria-hidden="true" />
                    <span class="numbering-map-label visually-hidden">{label.clone()}</span>
                    <select class="input-select numbering-map-in-view" aria-label={format!("{label} season in view")} title={format!("{label} season in view: pick one to jump to it")} {onchange}>
                        if current.is_none() {
                            <option value="" selected=true hidden=true>{"–"}</option>
                        }
                        for &(season, ..) in spans {
                            <option value={season.to_string()} selected={*current == Some(season)}>
                                { if season == 0 { "Sp".to_owned() } else { format!("S{season}") } }
                            </option>
                        }
                    </select>
                </span>
            }
        });

        let has_specials = columns.iter().any(|c| match &c.side {
            Side::Tmdb => data.episodes.iter().any(|&(s, _)| s == 0),
            Side::System(system) => data.system(system).iter().any(|&(s, _)| s == 0),
        });

        html! {
            <div class={classes!("numbering-map-pane", (!self.manual).then_some("read-only"))}>
                <div class="numbering-map-head" style={template.clone()} ref={self.map_head.clone()}>
                    for head in heads {
                        {head}
                    }
                </div>

                <svg class="numbering-defs" aria-hidden="true">
                    <linearGradient id="numbering-fade-gradient" x1="0" y1="0" x2="0" y2="1">
                        <stop offset="0" stop-color="white" />
                        <stop offset="1" stop-color="white" stop-opacity="0" />
                    </linearGradient>
                    <mask id="numbering-fade" maskContentUnits="objectBoundingBox">
                        <rect width="1" height="1" fill="url(#numbering-fade-gradient)" />
                    </mask>
                </svg>

                <div class="numbering-map-scroll" ref={self.map_scroll.clone()} {onscroll}>
                    <div class="numbering-map" style={format!("{template}; height: {height}px")}>
                        for part in map {
                            {part}
                        }
                    </div>
                </div>

                if has_specials || folded > 0 || foldable > folded {
                    <div class="numbering-map-foot">
                        if has_specials {
                            <button type="button" class="link-button numbering-specials" onclick={link.callback(|_: MouseEvent| Msg::ToggleSpecials)}>
                                { if self.specials { "Hide specials" } else { "Show specials" } }
                            </button>
                        }

                        if folded > 0 {
                            <button type="button" class="link-button numbering-unfold-all" title="Show every episode of the map" onclick={link.callback(|_: MouseEvent| Msg::FoldAll(false))}>
                                {"Unfold all"}
                            </button>
                        }

                        if foldable > folded {
                            <button type="button" class="link-button numbering-fold-all" title="Fold long runs of episodes that look alike" onclick={link.callback(|_: MouseEvent| Msg::FoldAll(true))}>
                                {"Fold long runs"}
                            </button>
                        }
                    </div>
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

            for e in r.first..=end(r) {
                let (Some(a), Some(b)) = (
                    self.address(r, e, &left.side),
                    self.address(r, e, &right.side),
                ) else {
                    continue;
                };

                let m = touches_own && missing.contains(&r.target_of(e));

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

                let ((y1, y2), (y3, y4)) = (inset(y1.0, y2.1), inset(y3.0, y4.1));
                let d = band(y1, y2, y3, y4);

                let class = classes!(
                    "numbering-band",
                    format!("tone-{}", i % TONES),
                    (self.manual && self.selected == Some(i)).then_some("selected"),
                    (self.manual && errors.iter().any(|(e, _)| *e == i)).then_some("invalid"),
                    missing.then_some("missing"),
                );

                let onclick = self
                    .manual
                    .then(|| link.callback(move |_: MouseEvent| Msg::SelectBand(i)));

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

            // An open range fades out over the episode after its last on
            // both sides.
            let tail = |c: &Column| c.tails.iter().find(|(_, j)| **j == i).map(|(&k, _)| k);

            if let (Some(a), Some(b)) = (tail(left), tail(right))
                && let (Some(ya), Some(yb)) = (left.y(a.0, a.1), right.y(b.0, b.1))
            {
                let d = band(ya.0 - 1.0, ya.1 - 1.0, yb.0 - 1.0, yb.1 - 1.0);

                let class = classes!(
                    "numbering-band",
                    "open-end",
                    format!("tone-{}", i % TONES),
                    (self.manual && self.selected == Some(i)).then_some("selected"),
                    (self.manual && errors.iter().any(|(e, _)| *e == i)).then_some("invalid"),
                );

                let onclick = link.callback(move |_: MouseEvent| Msg::SelectBand(i));

                let title = format!(
                    "{} S{} E{} on ↔ {} S{} E{} on",
                    left.side.label(),
                    a.0,
                    a.1,
                    right.side.label(),
                    b.0,
                    b.1
                );

                bands.push(html! {
                    <path {class} {d} mask="url(#numbering-fade)" {onclick}>
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

    /// How each episode of `column` is drawn, by its code.
    fn cells(
        &self,
        column: &Column,
        drawn: &[(usize, api::NumberingRange)],
        reach: &Reach,
        errors: &[(usize, String)],
    ) -> BTreeMap<(u32, u32), Cell> {
        let side = &column.side;
        let label = side.label();
        let tmdb = *side == Side::Tmdb;
        let mut out = BTreeMap::new();

        for s in &column.seasons {
            let season = s.season;

            for e in s.lo..=s.hi {
                let owners = reach.get(&(season, e)).map_or(&[][..], Vec::as_slice);
                let known = column.has(season, e);
                let tail = column
                    .tails
                    .get(&(season, e))
                    .copied()
                    .filter(|_| owners.is_empty());

                let state = match owners {
                    [] if let Some(i) = tail => format!("tone-{} open-end", i % TONES),
                    [] if tmdb && known && season > 0 => "unmapped".to_owned(),
                    [] => String::new(),
                    [(i, _)] => format!("tone-{}", i % TONES),
                    _ => "overlap".to_owned(),
                };

                let code = format!("{label} S{season} E{e}");

                let title = match owners {
                    [] if tail.is_some() => {
                        format!("{code} on: mapped by the open range as they are added")
                    }
                    _ if !tmdb && !known && !owners.is_empty() => format!("{code}: not in XEM"),
                    [] if tmdb && known => format!("{code}: not mapped"),
                    [] if tmdb => format!("{code}: not on TMDB"),
                    [] if known => format!("{code}: no TMDB episode"),
                    [] => format!("{code}: not in XEM"),
                    [(i, (_, te))] if tmdb => {
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

                let mine = |i: &usize| tail == Some(*i) || owners.iter().any(|(o, _)| o == i);

                let flags = [
                    (!tmdb && known && owners.is_empty() && tail.is_none()).then_some("untargeted"),
                    (!known && tail.is_none()).then_some(if tmdb || owners.is_empty() {
                        "future"
                    } else {
                        "missing"
                    }),
                    self.selected
                        .filter(|i| self.manual && mine(i))
                        .map(|_| "selected"),
                    errors
                        .iter()
                        .any(|(x, _)| self.manual && mine(x))
                        .then_some("invalid"),
                    self.anchor
                        .as_ref()
                        .is_some_and(|(a, s, x)| a == side && (*s, *x) == (season, e))
                        .then_some("anchor"),
                ];

                let classes = (!state.is_empty())
                    .then_some(state)
                    .into_iter()
                    .chain(flags.into_iter().flatten().map(str::to_owned))
                    .collect();

                out.insert(
                    (season, e),
                    Cell {
                        classes,
                        owners: owners.iter().map(|&(o, _)| o).chain(tail).collect(),
                        title,
                        tail: tail.is_some(),
                    },
                );
            }
        }

        out
    }

    /// A column's episodes, each marked by the ranges reaching it, with long
    /// runs that look alike folded into a row that unfolds on a click.
    fn view_column(
        &self,
        ctx: &Context<Self>,
        column: &Column,
        cells: &BTreeMap<(u32, u32), Cell>,
    ) -> Html {
        let link = ctx.link();
        let side = &column.side;
        let label = side.label();

        let seasons = column.seasons.iter().map(|s| {
            let (heading, name) = if s.season == 0 {
                ("Sp".to_owned(), "Specials".to_owned())
            } else {
                (format!("S{}", s.season), format!("Season {}", s.season))
            };

            let season = s.season;
            let mut rows = Vec::new();
            let mut folds = s.folds.iter().copied().peekable();
            let mut e = s.lo;

            while e <= s.hi {
                let Some(cell) = cells.get(&(season, e)) else {
                    e += 1;
                    continue;
                };

                if let Some((first, last)) = folds.next_if(|&(a, _)| a == e) {
                    let n = last - first + 1;
                    let code = format!("{label} S{season} E{first}–E{last}");
                    let title = format!("{code}: {n} episodes folded, click to show them");
                    let pick = side.clone();
                    let onclick = link.callback(move |_: MouseEvent| Msg::Unfold(pick.clone(), season, first, last));

                    rows.push(html! {
                        <button type="button" class={classes!("numbering-ep", "numbering-fold-row", cell.classes.clone())} data-season={season.to_string()} data-first={first.to_string()} data-last={last.to_string()} aria-label={code} aria-expanded="false" {title} {onclick}>
                            <span>
                                <span>{format!("E{first}–E{last}")}</span>
                                <span class="numbering-fold-count">{format!("· {n}")}</span>
                            </span>
                        </button>
                    });

                    e = last + 1;
                    continue;
                }

                let pick = side.clone();
                let onclick = link.callback(move |_: MouseEvent| Msg::Pick(pick.clone(), season, e));
                let text = if cell.tail { format!("E{e}…") } else { format!("E{e}") };

                rows.push(html! {
                    <button type="button" class={classes!("numbering-ep", cell.classes.clone())} data-season={season.to_string()} data-first={e.to_string()} data-last={e.to_string()} aria-label={format!("{label} S{season} E{e}")} title={cell.title.clone()} disabled={!self.manual} {onclick}>
                        <span>{text}</span>
                    </button>
                });

                e += 1;
            }

            html! {
                <>
                    <div class="numbering-map-season" title={format!("{label} {name}: {} episodes", s.known)}>
                        {heading}<span class="text-muted">{format!(" · {}", s.known)}</span>
                    </div>
                    for row in rows {
                        {row}
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

    /// A run of rows that map whole seasons alike, as one row that unfolds
    /// into them, also while one of them is selected.
    fn view_fold(
        &self,
        ctx: &Context<Self>,
        group: Range<usize>,
        ranges: &[Option<api::NumberingRange>],
        errors: &[(usize, String)],
    ) -> Html {
        let (Some(first), Some(last)) = (&ranges[group.start], &ranges[group.end - 1]) else {
            return Html::default();
        };

        let open = self
            .unfolded
            .contains(&(first.system.clone(), first.season))
            || self.selected.is_some_and(|i| group.contains(&i));

        let label = format!(
            "S{}–S{} match {} season for season · {} ranges",
            first.season,
            last.season,
            api::xem_system_label(&first.system),
            group.len()
        );

        let title = if open {
            "Fold these ranges"
        } else {
            "Show these ranges"
        };
        let toggle = group.clone();

        html! {
            <div class={classes!("numbering-fold", open.then_some("open"))} role="group" aria-label={label.clone()}>
                <button type="button" class="numbering-fold-summary" {title} aria-expanded={open.to_string()} onclick={ctx.link().callback(move |_: MouseEvent| Msg::SetFold(toggle.clone(), !open))}>
                    <span class={classes!("icon", if open { "chevron-down" } else { "chevron-right" })} aria-hidden="true" />
                    <span class="numbering-fold-label">{label}</span>
                </button>

                if open {
                    for index in group {
                        { self.view_row(ctx, index, &self.rows[index], ranges[index].as_ref(), errors) }
                    }
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

        // A cell each for the season, first and last episode, so they line up
        // in columns from row to row.
        let cells = |season: u32, first: u32, last: Option<u32>| {
            html! {
                <span class="numbering-span">
                    <span class="numbering-season">{format!("S{season}")}</span>{" "}
                    <span class="numbering-first">{format!("E{first}")}</span>
                    <span class="numbering-last">{last.map(|l| format!("–E{l}")).unwrap_or_else(|| "–".to_owned())}</span>
                </span>
            }
        };

        let summary = match range.filter(|r| drawable(r)) {
            Some(r) => html! {
                <>
                    { cells(r.season, r.first, r.last) }
                    <span class="icon arrows-right-left" aria-hidden="true" />
                    <span class={classes!("logo", r.system.clone())} title={api::xem_system_label(&r.system).to_owned()} />
                    { cells(r.target_season, r.target_first, r.target_last()) }
                </>
            },
            None => html! { <span class="numbering-incomplete">{"Incomplete range"}</span> },
        };

        let missing = range.map(|r| self.missing_targets(r)).unwrap_or_default();

        let on_select =
            link.callback(move |_: MouseEvent| Msg::Select((!selected).then_some(index)));

        html! {
            <div class={classes!("numbering-range", selected.then_some("selected"), (!messages.is_empty()).then_some("invalid"))} role="group" aria-label={format!("Range {n}")}>
                <div class="numbering-range-head">
                    <button type="button" class="numbering-range-summary" title={format!("Edit range {n}")} aria-expanded={selected.to_string()} onclick={on_select}>
                        <span class={classes!("numbering-swatch", format!("tone-{}", index % TONES))} aria-hidden="true" />
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
        let open = row.open;

        html! {
            <div class="numbering-fields">
                <div class="numbering-source">
                    <span class="logo tmdb" title="TMDB" />
                    <span class="numbering-letter">{"S"}</span>
                    { input(Field::Season, &row.season, format!("Range {n} season")) }
                    <span class="numbering-letter">{"E"}</span>
                    { input(Field::First, &row.first, format!("Range {n} first episode")) }
                    <span class="numbering-letter">{"–"}</span>
                    if row.open {
                        <input class="input-number" type="text" value="" placeholder="…" disabled=true aria-label={format!("Range {n} last episode")} title="Open: runs on through the season" />
                    } else {
                        { input(Field::Last, &row.last, format!("Range {n} last episode")) }
                    }
                    <Button
                        class={classes!("chip", "numbering-open", row.open.then_some("selected"))}
                        icon="arrow-long-down"
                        label="Open end"
                        title={format!("Range {n} runs on through later episodes of the season")}
                        pressed={Some(row.open)}
                        onclick={link.callback(move |_: MouseEvent| Msg::SetOpen(index, !open))}
                    />
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
