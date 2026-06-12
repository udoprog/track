use web_sys::{Event, MouseEvent};
use yew::prelude::*;

use iso639::{LanguageToCountry, Languages};
use musli_web::web03::prelude::*;

use crate::error::RcError;
use crate::{Modal, SetupChannel};

#[function_component]
pub(super) fn LoadingPage() -> Html {
    html! {
        <div class="page">
            <div class="box info">
                <span class="item-inline"><span class="icon arrow-path spin" /></span>
                <span>{"Loading…"}</span>
            </div>
        </div>
    }
}

#[derive(Properties, PartialEq)]
pub(super) struct ErrorBoxProps {
    pub(super) error: RcError,
    pub(super) onclearerror: Callback<()>,
}

#[function_component]
pub(super) fn ErrorBox(props: &ErrorBoxProps) -> Html {
    html! {
        <div class="box error">
            <div class="column fill">
                { for props.error.sources().map(|e| html! { <p>{e.to_string()}</p> }) }

                <button class="btn-danger" onclick={props.onclearerror.reform(|_| ())}>
                    <span class="item-inline"><span class="icon x-mark" /></span>
                    <span>{"Dismiss"}</span>
                </button>
            </div>
        </div>
    }
}

#[derive(Properties, PartialEq)]
pub(super) struct TrackedProps {
    pub(super) tracked: bool,
    pub(super) ontoggle: Callback<bool>,
}

#[function_component]
pub(super) fn Tracked(props: &TrackedProps) -> Html {
    let tracked = !props.tracked;

    html! {
        <button class="btn" onclick={props.ontoggle.reform(move |_| tracked)} title="Track movie">
            <span class="item-inline"><span class={classes!("icon", if tracked { "eye" } else { "eye-slash" })} /></span>
            <span class="hide-desktop">{if  tracked { "Tracking" } else { "Not tracking" }}</span>
        </button>
    }
}

#[derive(Properties, PartialEq)]
pub(super) struct PaginationButtonsProps {
    pub(super) page: usize,
    pub(super) total_pages: usize,
    pub(super) on_page: Callback<usize>,
}

#[function_component]
pub(super) fn PaginationButtons(props: &PaginationButtonsProps) -> Html {
    let page = props.page.min(props.total_pages.saturating_sub(1));
    let prev = page.checked_sub(1);
    let next = page.checked_add(1).filter(|&v| v < props.total_pages);

    let on_page = props.on_page.clone();
    let on_page2 = props.on_page.clone();

    if props.total_pages <= 1 {
        return html! {};
    }

    html! {
        <div class="input-group">
            <button class="btn" disabled={prev.is_none()}
                onclick={Callback::from(move |_| { if let Some(p) = prev { on_page.emit(p); } })}>
                <span class="icon arrow-left" />
            </button>

            <span class="input-text">{format!("{} / {}", page + 1, props.total_pages)}</span>

            <button class="btn" disabled={next.is_none()}
                onclick={Callback::from(move |_| { if let Some(p) = next { on_page2.emit(p); } })}>
                <span class="icon arrow-right" />
            </button>
        </div>
    }
}

#[derive(Properties, PartialEq)]
pub(super) struct ConfirmDangerProps {
    pub(super) prompt: AttrValue,
    #[prop_or_default]
    pub(super) label: Option<AttrValue>,
    pub(super) on_confirm: Callback<()>,
    pub(super) on_cancel: Callback<()>,
    #[prop_or_default]
    pub(super) btn_class: Classes,
}

#[function_component]
pub(super) fn ConfirmDanger(props: &ConfirmDangerProps) -> Html {
    let on_confirm = {
        let cb = props.on_confirm.clone();

        Callback::from(move |e: MouseEvent| {
            e.stop_propagation();
            cb.emit(());
        })
    };

    let on_cancel = {
        let cb = props.on_cancel.clone();
        Callback::from(move |e: MouseEvent| {
            e.stop_propagation();
            cb.emit(());
        })
    };

    html! {
        <div class="row-fill fill">
            if let Some(ref label) = props.label {
                <span class="fill">{&props.prompt}{" "}{label}{"?"}</span>
            } else {
                <span class="fill">{&props.prompt}{"?"}</span>
            }

            <div class="input-group end">
                <button onclick={on_cancel} class={classes!("btn", &props.btn_class)} title="No">
                    <span class="icon x-mark" />
                </button>

                <button onclick={on_confirm} class={classes!("btn-danger", &props.btn_class)} title="Yes">
                    <span class="icon check" />
                </button>
            </div>
        </div>
    }
}

// ── MarkWatchedPicker ─────────────────────────────────────────────────────────

/// Two-button step shown after clicking "Mark watched": choose now or when aired.
/// Renders as a `row-fill fill` that can replace the watch button's action area.
#[derive(Properties, PartialEq)]
pub(super) struct MarkWatchedPickerProps {
    #[prop_or_default]
    pub(super) class: Classes,
    pub(super) on_confirm: Callback<api::MarkTime>,
    pub(super) on_cancel: Callback<()>,
}

#[function_component]
pub(super) fn MarkWatchedPicker(props: &MarkWatchedPickerProps) -> Html {
    let on_now = props.on_confirm.reform(|e: MouseEvent| {
        e.stop_propagation();
        api::MarkTime::Now
    });

    let on_aired = props.on_confirm.reform(move |e: MouseEvent| {
        e.stop_propagation();
        api::MarkTime::WhenAired
    });

    let on_cancel = props.on_cancel.reform(move |e: MouseEvent| {
        e.stop_propagation();
    });

    html! {
        <div class={classes!("row-fill", "fill", &props.class)}>
            <span>{"Watched when?"}</span>

            <div class="end">
                <div class="input-group">
                    <button class="btn" onclick={on_cancel} title="Cancel">
                        <span class="icon x-mark" />
                    </button>

                    <button class="btn-success" onclick={on_now} title="Watched now">
                        <span class="item-inline"><span class="icon check" /></span>
                        {"Now"}
                    </button>

                    <button class="btn" onclick={on_aired} title="Watched when aired">
                        <span class="item-inline"><span class="icon clock" /></span>
                        {"Aired"}
                    </button>
                </div>
            </div>
        </div>
    }
}

// ── RemoteSourceSelect ───────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum RemoteSourceKind {
    Series,
    Movie,
}

#[derive(Properties, PartialEq)]
pub(super) struct RemoteSourceSelectProps {
    pub(super) remotes: Vec<api::RemoteId>,
    pub(super) current_source: Option<api::RemoteSource>,
    pub(super) kind: RemoteSourceKind,
    pub(super) on_change: Callback<api::RemoteSource>,
}

#[function_component]
pub(super) fn RemoteSourceSelect(props: &RemoteSourceSelectProps) -> Html {
    if props.remotes.is_empty() {
        return Html::default();
    }

    let selected = props.current_source.filter(|source| {
        props
            .remotes
            .iter()
            .any(|remote| *remote.source() == *source)
    });

    let on_change = {
        let cb = props.on_change.clone();

        Callback::from(move |e: Event| {
            let input: web_sys::HtmlSelectElement = e.target_unchecked_into();
            let source = api::RemoteSource::from_raw(&input.value());

            if !source.is_unknown() {
                cb.emit(source);
            }
        })
    };

    html! {
        <select class="input-select" onchange={on_change} title="Select remote source">
            {
                for props.remotes.iter().map(|remote| {
                    let label = remote.source().as_str().to_uppercase();

                    html! {
                        <option value={remote.source().as_str().to_owned()} selected={Some(remote.source()) == selected.as_ref()}>{label}</option>
                    }
                })
            }
        </select>
    }
}

// ── RemoteEditor ──────────────────────────────────────────────────────────────

/// Sources offered when adding a remote identifier, as `(value, label)`.
const REMOTE_SOURCES: &[(api::RemoteSource, &str)] = &[
    (api::RemoteSource::Tmdb, "TMDB"),
    (api::RemoteSource::Tvdb, "TVDB"),
    (api::RemoteSource::Imdb, "IMDb"),
];

/// Validate a source/value pair and build the `RemoteId`, or return a
/// user-facing error explaining why the identifier is invalid.
fn parse_remote(source: &api::RemoteSource, value: &str) -> Result<api::RemoteId, String> {
    let value = value.trim();

    if value.is_empty() {
        return Err("Identifier must not be empty".to_string());
    }

    let value = match *source {
        api::RemoteSource::Tvdb | api::RemoteSource::Tmdb => {
            let Ok(value) = value.parse::<u32>() else {
                return Err(format!("{} identifier must be a number", source.as_str()));
            };

            api::RemoteValue::Int(value)
        }
        api::RemoteSource::Imdb => {
            let valid = value.strip_prefix("tt").is_some_and(|digits| {
                !digits.is_empty() && digits.bytes().all(|b| b.is_ascii_digit())
            });

            if !valid {
                return Err("IMDb identifier must look like tt1234567".to_string());
            }

            api::RemoteValue::Str(value.to_string())
        }
        _ => {
            return Err("Unknown remote source".to_string());
        }
    };

    Ok(api::RemoteId::new(*source, value))
}

/// Modal for adding, editing and removing remote identifiers (e.g. `tvdb:123`,
/// `imdb:tt0001234`) of a series or movie. The component is presentation-only:
/// it emits `on_add`/`on_edit`/`on_remove` and the caller performs the request,
/// which makes it reusable wherever a remote needs to be repaired.
#[derive(Properties, PartialEq)]
pub(super) struct RemoteEditorProps {
    pub(super) title: String,
    pub(super) remotes: Vec<api::RemoteId>,
    pub(super) on_add: Callback<api::RemoteId>,
    /// `(old, new)` — replace an existing identifier with an edited one.
    pub(super) on_edit: Callback<(api::RemoteId, api::RemoteId)>,
    pub(super) on_remove: Callback<api::RemoteId>,
    pub(super) on_close: Callback<()>,
}

pub(super) enum RemoteEditorMsg {
    SetSource(api::RemoteSource),
    SetValue(String),
    Submit,
    Edit(api::RemoteId),
    CancelEdit,
    AskRemove(api::RemoteId),
    CancelRemove,
    ConfirmRemove(api::RemoteId),
    Close,
}

pub(super) struct RemoteEditor {
    source: api::RemoteSource,
    value: String,
    /// When set, the form edits this existing identifier instead of adding.
    editing: Option<api::RemoteId>,
    /// When set, awaiting confirmation to remove this identifier.
    confirming_remove: Option<api::RemoteId>,
    error: Option<String>,
    /// The source `<select>`; its displayed selection is a DOM property that
    /// must be set imperatively when `source` changes programmatically.
    source_ref: NodeRef,
}

impl RemoteEditor {
    fn reset_form(&mut self) {
        self.source = REMOTE_SOURCES[0].0;
        self.value.clear();
        self.editing = None;
        self.error = None;
    }
}

impl Component for RemoteEditor {
    type Message = RemoteEditorMsg;
    type Properties = RemoteEditorProps;

    fn create(_ctx: &Context<Self>) -> Self {
        Self {
            source: REMOTE_SOURCES[0].0,
            value: String::new(),
            editing: None,
            confirming_remove: None,
            error: None,
            source_ref: NodeRef::default(),
        }
    }

    fn rendered(&mut self, _ctx: &Context<Self>, _first_render: bool) {
        // The displayed option is a DOM property, not an attribute, so it must
        // be assigned imperatively to track `source` (e.g. after Edit).
        if let Some(select) = self.source_ref.cast::<web_sys::HtmlSelectElement>() {
            select.set_value(self.source.as_str());
        }
    }

    fn update(&mut self, ctx: &Context<Self>, msg: Self::Message) -> bool {
        match msg {
            RemoteEditorMsg::SetSource(source) => {
                self.source = source;
                self.error = None;
                true
            }
            RemoteEditorMsg::SetValue(value) => {
                self.value = value;
                self.error = None;
                true
            }
            RemoteEditorMsg::Submit => {
                match parse_remote(&self.source, &self.value) {
                    Ok(remote_id) => {
                        match self.editing.take() {
                            Some(old) if old != remote_id => {
                                ctx.props().on_edit.emit((old, remote_id));
                            }
                            Some(..) => {}
                            None => {
                                ctx.props().on_add.emit(remote_id);
                            }
                        }

                        self.reset_form();
                    }
                    Err(error) => {
                        self.error = Some(error);
                    }
                }

                true
            }
            RemoteEditorMsg::Edit(remote_id) => {
                self.source = *remote_id.source();
                self.value = remote_id.value().to_string();
                self.editing = Some(remote_id);
                self.confirming_remove = None;
                self.error = None;
                true
            }
            RemoteEditorMsg::CancelEdit => {
                self.reset_form();
                true
            }
            RemoteEditorMsg::AskRemove(remote_id) => {
                self.confirming_remove = Some(remote_id);
                true
            }
            RemoteEditorMsg::CancelRemove => {
                self.confirming_remove = None;
                true
            }
            RemoteEditorMsg::ConfirmRemove(remote_id) => {
                if self.editing.as_ref() == Some(&remote_id) {
                    self.reset_form();
                }

                self.confirming_remove = None;
                ctx.props().on_remove.emit(remote_id);
                true
            }
            RemoteEditorMsg::Close => {
                ctx.props().on_close.emit(());
                false
            }
        }
    }

    fn view(&self, ctx: &Context<Self>) -> Html {
        let link = ctx.link();
        let props = ctx.props();

        let on_source = link.callback(|e: Event| {
            let select: web_sys::HtmlSelectElement = e.target_unchecked_into();
            let value = select.value();
            let source = REMOTE_SOURCES
                .iter()
                .find(|(source, _)| source.as_str() == value)
                .map(|(source, _)| source)
                .unwrap_or(&REMOTE_SOURCES[0].0);
            RemoteEditorMsg::SetSource(*source)
        });

        let on_value = link.callback(|e: InputEvent| {
            let input: web_sys::HtmlInputElement = e.target_unchecked_into();
            RemoteEditorMsg::SetValue(input.value())
        });

        let editing = self.editing.is_some();

        let title = html! {
            <>
                <span class="item-inline"><span class="icon identification" /></span>
                <span>{format!("Identifiers — {}", props.title)}</span>
            </>
        };

        html! {
            <Modal {title} on_close={link.callback(|_| RemoteEditorMsg::Close)}>
                if props.remotes.is_empty() {
                    <div class="empty text-muted">{"No remote identifiers"}</div>
                } else {
                    { for props.remotes.iter().map(|r| {
                        if self.confirming_remove.as_ref() == Some(r) {
                            let remote_id = r.clone();

                            return html! {
                                <ConfirmDanger
                                    key={r.to_string()}
                                    prompt="Remove"
                                    label={r.to_string()}
                                    on_confirm={link.callback(move |_| RemoteEditorMsg::ConfirmRemove(remote_id.clone()))}
                                    on_cancel={link.callback(|_| RemoteEditorMsg::CancelRemove)}
                                />
                            };
                        }

                        let editing_this = self.editing.as_ref() == Some(r);
                        let edit_id = r.clone();
                        let remove_id = r.clone();

                        html! {
                            <div key={r.to_string()} class={classes!("row-fill", editing_this.then_some("active"))}>
                                <div class="row clickable">
                                    <span class="item-inline-lg">
                                        <span class={classes!("logo", r.source().as_str().to_owned())} />
                                    </span>

                                    <span>{r.value().to_string()}</span>
                                </div>

                                <div class="input-group end">
                                    <button class="btn" onclick={link.callback(move |_| RemoteEditorMsg::Edit(edit_id.clone()))} title="Edit identifier">
                                        <span class="icon pencil-square" />
                                    </button>

                                    <button class="btn-danger" onclick={link.callback(move |_| RemoteEditorMsg::AskRemove(remove_id.clone()))} title="Remove identifier">
                                        <span class="icon trash" />
                                    </button>
                                </div>
                            </div>
                        }
                    }) }
                }

                <div class="form">
                    <div class={classes!("field", self.error.is_some().then_some("error"))}>
                        <div class="input-group fill">
                            <select ref={self.source_ref.clone()} class="input-select" onchange={on_source} title="Source">
                                { for REMOTE_SOURCES.iter().map(|(value, label)| html! {
                                    <option value={value.as_str()} selected={self.source == *value}>{label}</option>
                                }) }
                            </select>

                            <input type="text" class="input-text fill" placeholder="Identifier" value={self.value.clone()} oninput={on_value} />

                            <button class="btn-success" onclick={link.callback(|_| RemoteEditorMsg::Submit)} disabled={self.value.trim().is_empty()} title={if editing { "Save identifier" } else { "Add identifier" }}>
                                <span class="item-inline"><span class={classes!("icon", if editing { "check" } else { "plus" })} /></span>
                                <span>{if editing { "Save" } else { "Add" }}</span>
                            </button>

                            if editing {
                                <button class="btn" onclick={link.callback(|_| RemoteEditorMsg::CancelEdit)} title="Cancel edit">
                                    <span class="icon x-mark" />
                                </button>
                            }
                        </div>

                        if let Some(ref error) = self.error {
                            <label>{error}</label>
                        }
                    </div>
                </div>
            </Modal>
        }
    }
}

// ── LanguagePicker ────────────────────────────────────────────────────────────

const LANGUAGE_PAGE_SIZE: usize = 5;

#[derive(Properties, PartialEq)]
pub(super) struct LanguagePickerProps {
    pub(super) current: Option<String>,
    pub(super) on_change: Callback<Option<String>>,
    pub(super) placeholder: &'static str,
}

pub(super) enum Msg {
    Open,
    Close,
    Filter(String),
    Page(usize),
    Pick(Option<String>),
}

pub(super) struct LanguagePicker {
    languages: Languages,
    language_to_country: LanguageToCountry,
    open: bool,
    filter: String,
    page: usize,
}

impl Component for LanguagePicker {
    type Message = Msg;
    type Properties = LanguagePickerProps;

    fn create(_ctx: &Context<Self>) -> Self {
        Self {
            languages: Languages::new(),
            language_to_country: LanguageToCountry::new(),
            open: false,
            filter: String::new(),
            page: 0,
        }
    }

    fn update(&mut self, ctx: &Context<Self>, msg: Self::Message) -> bool {
        match msg {
            Msg::Open => {
                self.open = true;
                self.filter.clear();
                self.page = 0;
            }
            Msg::Close => {
                self.open = false;
            }
            Msg::Filter(s) => {
                self.filter = s;
                self.page = 0;
            }
            Msg::Page(p) => {
                self.page = p;
            }
            Msg::Pick(value) => {
                self.open = false;
                ctx.props().on_change.emit(value);
            }
        }

        true
    }

    fn view(&self, ctx: &Context<Self>) -> Html {
        let link = ctx.link();
        let props = ctx.props();

        let value = ctx.props().current.as_ref().and_then(|code| {
            self.languages
                .get_by_part1(code)
                .and_then(|entry| Some((entry.ref_name, entry.part1?)))
        });

        let trigger = match value {
            Some((label, code)) => html! {
                <button class="btn" onclick={link.callback(|_| Msg::Open)} title="Select language">
                    <span class="item-inline"><span class="icon language" /></span>
                    <span>{label}</span>

                    if let Some(code) = self.language_to_country.get_by_part1(code) {
                        <span class={classes!("flag", code)}></span>
                    }
                </button>
            },
            None => html! {
                <button class="btn" onclick={link.callback(|_| Msg::Open)} title="Select language">
                    <span class="item-inline"><span class="icon language" /></span>
                    <span>{props.placeholder}</span>
                </button>
            },
        };

        if !self.open {
            return trigger;
        }

        let needle = self.filter.to_lowercase();
        let filtered: Vec<(&'static str, &'static iso639::Entry)> = self
            .languages
            .iter()
            .filter(|(_, entry)| {
                needle.is_empty() || entry.ref_name.to_lowercase().contains(&needle)
            })
            .collect();

        let total_pages = filtered.len().div_ceil(LANGUAGE_PAGE_SIZE).max(1);
        let page = self.page.min(total_pages.saturating_sub(1));

        let on_filter = link.callback(|e: InputEvent| {
            let input: web_sys::HtmlInputElement = e.target_unchecked_into();
            Msg::Filter(input.value())
        });

        let current = props.current.clone();

        html! {
            <>
                {trigger}

                <Modal title="Select Language" on_close={link.callback(|_| Msg::Close)}>
                    <div class="row">
                        <input autofocus={true} type="text" class="input-text fill" placeholder="Filter" value={self.filter.clone()} oninput={on_filter} />
                    </div>

                    <div class="table">
                        <div class="table-entry row clickable" onclick={link.callback(|_| Msg::Pick(None))}>
                            <span class="fill">{props.placeholder}</span>

                            if current.is_none() {
                                <span class="icon check" />
                            }
                        </div>

                        {
                            for filtered.iter()
                                .skip(page.saturating_mul(LANGUAGE_PAGE_SIZE))
                                .take(LANGUAGE_PAGE_SIZE)
                                .map(|&(part1, entry)| {
                                    let selected = current.as_deref() == Some(part1);

                                    html! {
                                        <div key={part1} class={classes!("table-entry", "row", "clickable", selected.then_some("active"))} onclick={link.callback(move |_| Msg::Pick(Some(part1.to_string())))}>
                                            <span class="fill">{entry.ref_name}</span>

                                            if let Some(code) = self.language_to_country.get_by_part1(part1) {
                                                <span class={classes!("item-inline", "flag", code)} />
                                            }

                                            <span class="text-muted">{part1}</span>
                                        </div>
                                    }
                                })
                        }
                    </div>

                    <div class="row center">
                        <PaginationButtons
                            page={page}
                            total_pages={total_pages}
                            on_page={link.callback(Msg::Page)}
                        />
                    </div>
                </Modal>
            </>
        }
    }
}

// ── EpisodePicker ─────────────────────────────────────────────────────────────

/// Inline season + episode picker used for moving or fixing watched entries.
/// Renders two `<select>` elements and confirm/cancel buttons, fitting inside
/// a `row` or `table-entry` without taking up extra vertical space.
pub(super) struct EpisodePicker {
    channel: ws::Channel,
    selected_season: Option<api::SeasonNumber>,
    episodes: Vec<api::Episode>,
    selected_episode: Option<u32>,
    _setup: SetupChannel,
    _req: ws::Request,
}

pub(super) enum EpisodePickerMsg {
    Channel(Result<ws::Channel, ws::Error>),
    SelectSeason(api::SeasonNumber),
    EpisodesLoaded(Result<ws::Packet<api::ListEpisodes>, ws::Error>),
    SelectEpisode(u32),
    Confirm,
    Cancel,
}

#[derive(Properties, PartialEq)]
pub(super) struct EpisodePickerProps {
    pub(super) prompt: AttrValue,
    pub(super) label: Option<AttrValue>,
    pub(super) series_id: api::SeriesId,
    pub(super) seasons: Vec<api::Season>,
    #[prop_or_default]
    pub(super) selected_season: Option<api::SeasonNumber>,
    #[prop_or_default]
    pub(super) selected_episode: Option<u32>,
    pub(super) on_confirm: Callback<(api::SeasonNumber, u32)>,
    pub(super) on_cancel: Callback<()>,
}

impl Component for EpisodePicker {
    type Message = EpisodePickerMsg;
    type Properties = EpisodePickerProps;

    fn create(ctx: &Context<Self>) -> Self {
        let (ws, _) = ctx
            .link()
            .context::<ws::Handle>(Callback::noop())
            .expect("ws::Handle context not found");

        let selected_season = match ctx.props().selected_season {
            Some(selected_season) => Some(selected_season),
            None => ctx
                .props()
                .seasons
                .iter()
                .find(|s| !s.season.is_special())
                .or_else(|| ctx.props().seasons.first())
                .map(|s| s.season),
        };

        let _setup = SetupChannel::new(ws, ctx.link().callback(EpisodePickerMsg::Channel));

        tracing::warn!(selected_episode = ?ctx.props().selected_episode);

        Self {
            channel: ws::Channel::default(),
            selected_season,
            episodes: Vec::new(),
            selected_episode: ctx.props().selected_episode,
            _setup,
            _req: ws::Request::default(),
        }
    }

    fn update(&mut self, ctx: &Context<Self>, msg: Self::Message) -> bool {
        match msg {
            EpisodePickerMsg::Channel(result) => {
                self.channel = result.unwrap_or_default();
                if self.channel.id() != ws::ChannelId::NONE
                    && let Some(season) = self.selected_season
                {
                    self.load_episodes(ctx, season);
                }
                false
            }
            EpisodePickerMsg::SelectSeason(season) => {
                self.selected_season = Some(season);
                self.episodes.clear();
                self.selected_episode = None;
                self.load_episodes(ctx, season);
                true
            }
            EpisodePickerMsg::EpisodesLoaded(result) => {
                if let Ok(packet) = result
                    && let Ok(resp) = packet.decode()
                {
                    self.episodes = resp.episodes;

                    if let Some(selected_episode) = self.selected_episode
                        && !self
                            .episodes
                            .iter()
                            .any(|ep| ep.episode == selected_episode)
                    {
                        self.selected_episode = None;
                    }
                }

                true
            }
            EpisodePickerMsg::SelectEpisode(episode) => {
                self.selected_episode = Some(episode);
                false
            }
            EpisodePickerMsg::Confirm => {
                if let (Some(season), Some(episode)) = (self.selected_season, self.selected_episode)
                {
                    ctx.props().on_confirm.emit((season, episode));
                }

                false
            }
            EpisodePickerMsg::Cancel => {
                ctx.props().on_cancel.emit(());
                false
            }
        }
    }

    fn view(&self, ctx: &Context<Self>) -> Html {
        let link = ctx.link();

        let on_season_change = link.callback(|e: Event| {
            let select: web_sys::HtmlSelectElement = e.target_unchecked_into();
            let n: u32 = select.value().parse().unwrap_or(0);
            EpisodePickerMsg::SelectSeason(api::SeasonNumber::from_ordinal(n))
        });

        let on_episode_change = link.callback(|e: Event| {
            let select: web_sys::HtmlSelectElement = e.target_unchecked_into();
            let n: u32 = select.value().parse().unwrap_or(1);
            EpisodePickerMsg::SelectEpisode(n)
        });

        let can_confirm = self.selected_season.is_some() && self.selected_episode.is_some();

        html! {
            <div class="row-fill fill">
                <div class="row">
                    if let Some(ref label) = ctx.props().label {
                        <span class="fill">{&ctx.props().prompt}{" "}{label}{"?"}</span>
                    } else {
                        <span class="fill">{&ctx.props().prompt}{"?"}</span>
                    }
                </div>

                <div class="row end">
                    <select class="input-select" onchange={on_season_change}>
                        { for ctx.props().seasons.iter().map(|s| {
                            let value = s.season.ordinal().to_string();
                            let selected = self.selected_season == Some(s.season);
                            html! { <option {value} {selected}>{s.season.long().to_string()}</option> }
                        }) }
                    </select>

                    <select class="input-select" onchange={on_episode_change} disabled={self.episodes.is_empty()}>
                        { for self.episodes.iter().map(|ep| {
                            let value = ep.episode.to_string();
                            let label = format!("E{:02}", ep.episode);
                            let selected = self.selected_episode == Some(ep.episode);
                            html! { <option {value} {selected}>{label}</option> }
                        }) }
                    </select>

                    <div class="input-group">
                        <button class="btn" onclick={link.callback(|_| EpisodePickerMsg::Cancel)}
                            title="Cancel">
                            <span class="icon x-mark" />
                        </button>
                        <button class="btn-success" onclick={link.callback(|_| EpisodePickerMsg::Confirm)}
                            title="Confirm" disabled={!can_confirm}>
                            <span class="icon check" />
                        </button>
                    </div>
                </div>
            </div>
        }
    }
}

impl EpisodePicker {
    fn load_episodes(&mut self, ctx: &Context<Self>, season: api::SeasonNumber) {
        self._req = self
            .channel
            .request()
            .body(api::ListEpisodesRequest {
                series_id: ctx.props().series_id,
                season,
            })
            .on_packet(ctx.link().callback(EpisodePickerMsg::EpisodesLoaded))
            .send();
    }
}
