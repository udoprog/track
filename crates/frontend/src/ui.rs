use gloo::timers::callback::Timeout;
use web_sys::{Event, InputEvent, MouseEvent};
use yew::prelude::*;

use iso639::{LanguageToCountry, Languages};
use iso3166::{Countries, Country};
use musli_web::web03::prelude::*;

use crate::error::RcError;
use crate::{Modal, SetupChannel};

pub(crate) const MDASH: &str = "—";

/// Loading indicator placed inside the shared page container (rendered by
/// `App`). Use [`LoadingPage`] for standalone, full-page loading screens.
#[function_component]
pub(super) fn Loading() -> Html {
    html! {
        <div class="box info">
            <span class="icon arrow-path spin" />
            <span>{"Loading…"}</span>
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
                    <span class="icon x-mark" />
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
    let tracked = props.tracked;

    html! {
        <button class="btn" onclick={props.ontoggle.reform(move |_| !tracked)} title="Track movie">
            <span class={classes!("icon", if tracked { "eye" } else { "eye-slash" })} />
            <span class="hide-desktop">{if tracked { "Tracking" } else { "Not tracking" }}</span>
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

    let on_back = prev.map(|prev| props.on_page.reform(move |_: MouseEvent| prev));
    let on_next = next.map(|next| props.on_page.reform(move |_: MouseEvent| next));

    html! {
        <>
            <button class={classes!("btn", prev.is_none().then_some("disabled"))} onclick={on_back}>
                <span class="icon chevron-left" />
            </button>

            <span class="input-text">
                {format!("{} / {}", page.saturating_add(1), props.total_pages)}
            </span>

            <button class={classes!("btn", next.is_none().then_some("disabled"))} onclick={on_next}>
                <span class="icon chevron-right" />
            </button>
        </>
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
    let on_confirm = props.on_confirm.reform(move |e: MouseEvent| {
        e.stop_propagation();
    });

    let on_cancel = props.on_cancel.reform(move |e: MouseEvent| {
        e.stop_propagation();
    });

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

/// Two-button step shown after clicking "Mark watched": choose now or when aired.
/// Renders as a `row-fill fill` that can replace the watch button's action area.
#[derive(Properties, PartialEq)]
pub(super) struct MarkWatchedPickerProps {
    #[prop_or_default]
    pub(super) class: Classes,
    /// Heading shown above the choices. Defaults to "Watched when?".
    #[prop_or(AttrValue::Static("Watched when?"))]
    pub(super) prompt: AttrValue,
    /// Label for the "when aired" choice. Defaults to "Aired"; movies pass
    /// "Released" since "aired" reads oddly for them.
    #[prop_or(AttrValue::Static("Aired"))]
    pub(super) aired_label: AttrValue,
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
            <span>{&props.prompt}</span>

            <div class="end">
                <div class="input-group">
                    <button class="btn" onclick={on_cancel} title="Cancel">
                        <span class="icon x-mark" />
                    </button>

                    <button class="btn-success" onclick={on_now} title="Watched now">
                        <span class="icon check" />
                        {"Now"}
                    </button>

                    <button class="btn" onclick={on_aired} title="Watched when aired">
                        <span class="icon clock" />
                        {&props.aired_label}
                    </button>
                </div>
            </div>
        </div>
    }
}

#[derive(Properties, PartialEq)]
pub(super) struct MediaSettingsModalProps {
    pub(super) title: AttrValue,
    pub(super) language: Option<String>,
    pub(super) has_images: bool,
    pub(super) on_language_change: Callback<Option<String>>,
    pub(super) on_edit_graphics: Callback<()>,
    pub(super) on_edit_identifiers: Callback<()>,
    pub(super) on_close: Callback<()>,
    /// Remote identifiers available for syncing.
    pub(super) remotes: Vec<api::Remote>,
    /// Whether to scope the sync-source select to show or movie remotes.
    pub(super) kind: RemoteSourceKind,
    /// Currently selected sync source.
    pub(super) current_source: Option<api::RemoteSource>,
    /// Formatted "last synced at" timestamp, or `None` if never synced.
    pub(super) last_synced: Option<AttrValue>,
    /// Whether a sync is currently in progress (spins the sync icon).
    pub(super) syncing: bool,
    pub(super) on_sync_source_change: Callback<api::RemoteSource>,
    pub(super) on_sync: Callback<()>,
    /// Current specials override. Only meaningful when
    /// `on_include_specials_change` is set.
    #[prop_or_default]
    pub(super) include_specials: Option<bool>,
    /// When set, the "Specials when syncing" field is rendered.
    #[prop_or_default]
    pub(super) on_include_specials_change: Option<Callback<Option<bool>>>,
    /// Current per-movie release-filter override (`None` = use the global default). Only meaningful
    /// when `on_release_filters_change` is set.
    #[prop_or_default]
    pub(super) release_filters: Option<Vec<api::ReleaseFilter>>,
    /// Global default release filters, used to seed the editor when switching to a custom override.
    #[prop_or_default]
    pub(super) default_release_filters: Vec<api::ReleaseFilter>,
    /// When set, the "Release dates" override field is rendered.
    #[prop_or_default]
    pub(super) on_release_filters_change: Option<Callback<Option<Vec<api::ReleaseFilter>>>>,
}

#[function_component]
pub(super) fn MediaSettingsModal(props: &MediaSettingsModalProps) -> Html {
    let on_edit_graphics = props.on_edit_graphics.reform(|_: MouseEvent| ());
    let on_edit_identifiers = props.on_edit_identifiers.reform(|_: MouseEvent| ());
    let on_sync = props.on_sync.reform(|_: MouseEvent| ());

    let specials = props.on_include_specials_change.as_ref().map(|cb| {
        let include_specials = props.include_specials;
        let value = match include_specials {
            None => "default",
            Some(true) => "include",
            Some(false) => "skip",
        };

        let on_change = cb.reform(|e: Event| {
            let select: web_sys::HtmlSelectElement = e.target_unchecked_into();
            match select.value().as_str() {
                "include" => Some(true),
                "skip" => Some(false),
                _ => None,
            }
        });

        html! {
            <div class="field">
                <label>{"Specials when syncing"}</label>
                <select class="input-select" onchange={on_change} {value}>
                    <option value="default" selected={include_specials.is_none()}>{"Default"}</option>
                    <option value="include" selected={include_specials == Some(true)}>{"Include"}</option>
                    <option value="skip" selected={include_specials == Some(false)}>{"Skip"}</option>
                </select>
            </div>
        }
    });

    let release = props.on_release_filters_change.as_ref().map(|cb| {
        let is_custom = props.release_filters.is_some();

        let on_mode = {
            let cb = cb.clone();
            let default = props.default_release_filters.clone();
            Callback::from(move |e: Event| {
                let select: web_sys::HtmlSelectElement = e.target_unchecked_into();
                match select.value().as_str() {
                    "custom" => cb.emit(Some(default.clone())),
                    _ => cb.emit(None),
                }
            })
        };

        let editor = props.release_filters.as_ref().map(|filters| {
            let on_change = cb.reform(|f: Vec<api::ReleaseFilter>| Some(f));
            html! {
                <ReleaseFiltersEditor filters={filters.clone()} on_change={on_change} />
            }
        });

        html! {
            <div class="field">
                <label>{"Release dates"}</label>
                <select class="input-select" onchange={on_mode}>
                    <option value="default" selected={!is_custom}>{"Default"}</option>
                    <option value="custom" selected={is_custom}>{"Customize"}</option>
                </select>
                {editor}
            </div>
        }
    });

    html! {
        <Modal title={props.title.clone()} on_close={props.on_close.reform(|_| ())}>
            <div class="form">
                <div class="field">
                    <label>{"Language"}</label>
                    <LanguagePicker
                        current={props.language.clone()}
                        placeholder="Default"
                        on_change={props.on_language_change.clone()}
                    />
                </div>

                {specials}

                {release}

                <div class="field">
                    <label>{"Sync"}</label>
                    <div class="input-group">
                        <RemoteSourceSelect
                            kind={props.kind}
                            remotes={props.remotes.clone()}
                            current_source={props.current_source}
                            on_change={props.on_sync_source_change.clone()}
                        />

                        if let Some(ref ts) = props.last_synced {
                            <div class="input-text fill" title="Last synced at">{ts}</div>
                        } else {
                            <div class="input-text fill text-muted">{"Never synced"}</div>
                        }

                        if !props.remotes.is_empty() {
                            <button class="btn" onclick={on_sync} title="Sync now">
                                <span class={classes!("icon", "arrow-path", props.syncing.then_some("spin"))} />
                            </button>
                        }
                    </div>
                </div>

                <div class="field">
                    <label>{"Graphics"}</label>
                    if props.has_images {
                        <button class="btn" onclick={on_edit_graphics}>
                            <span class="icon photo" />
                            <span>{"Edit graphics"}</span>
                        </button>
                        <span class="hint">{"Choose the poster, backdrop, banner, and other artwork."}</span>
                    } else {
                        <span class="hint">{"No graphics available. Sync to fetch artwork."}</span>
                    }
                </div>

                <div class="field">
                    <label>{"Identifiers"}</label>
                    <button class="btn" onclick={on_edit_identifiers}>
                        <span class="icon identification" />
                        <span>{"Edit identifiers"}</span>
                    </button>
                    <span class="hint">{"Repair the TMDB, TVDB, and other remote ids used to sync."}</span>
                </div>
            </div>
        </Modal>
    }
}

/// How long a revealed secret stays visible before auto-hiding.
const SECRET_REVEAL_MS: u32 = 3000;

/// Reusable input for sensitive values (API keys, PINs). Renders as a password
/// field with three actions: reveal (shows the value, then auto-hides after a
/// few seconds), copy to clipboard, and clear. Controlled `value` comes from
/// the parent and edits are emitted through `on_change`.
#[derive(Properties, PartialEq)]
pub(super) struct SecretInputProps {
    pub(super) value: String,
    #[prop_or_default]
    pub(super) placeholder: AttrValue,
    pub(super) on_change: Callback<String>,
}

pub(super) enum SecretInputMsg {
    Input(String),
    Reveal,
    Hide,
    Copy,
    Clear,
}

pub(super) struct SecretInput {
    revealed: bool,
    // Held so the scheduled auto-hide fires; dropping it cancels the timer.
    _hide_timer: Option<Timeout>,
}

impl Component for SecretInput {
    type Message = SecretInputMsg;
    type Properties = SecretInputProps;

    fn create(_ctx: &Context<Self>) -> Self {
        Self {
            revealed: false,
            _hide_timer: None,
        }
    }

    fn update(&mut self, ctx: &Context<Self>, msg: Self::Message) -> bool {
        match msg {
            SecretInputMsg::Input(value) => {
                ctx.props().on_change.emit(value);
                false
            }
            SecretInputMsg::Reveal => {
                self.revealed = true;
                let link = ctx.link().clone();
                self._hide_timer = Some(Timeout::new(SECRET_REVEAL_MS, move || {
                    link.send_message(SecretInputMsg::Hide);
                }));
                true
            }
            SecretInputMsg::Hide => {
                self.revealed = false;
                self._hide_timer = None;
                true
            }
            SecretInputMsg::Copy => {
                if let Some(window) = web_sys::window() {
                    let _ = window
                        .navigator()
                        .clipboard()
                        .write_text(&ctx.props().value);
                }
                false
            }
            SecretInputMsg::Clear => {
                ctx.props().on_change.emit(String::new());
                self.revealed = false;
                self._hide_timer = None;
                true
            }
        }
    }

    fn view(&self, ctx: &Context<Self>) -> Html {
        let link = ctx.link();
        let props = ctx.props();
        let is_empty = props.value.is_empty();

        let on_input = link.callback(|e: InputEvent| {
            let input: web_sys::HtmlInputElement = e.target_unchecked_into();
            SecretInputMsg::Input(input.value())
        });

        let revealed = self.revealed;
        let on_toggle = link.callback(move |_: MouseEvent| {
            if revealed {
                SecretInputMsg::Hide
            } else {
                SecretInputMsg::Reveal
            }
        });

        let (toggle_icon, toggle_title) = if revealed {
            ("eye-slash", "Hide")
        } else {
            ("eye", "Reveal")
        };

        html! {
            <div class="input-group">
                <input
                    type={if revealed { "text" } else { "password" }}
                    class="input-text fill"
                    placeholder={props.placeholder.clone()}
                    value={props.value.clone()}
                    oninput={on_input}
                    autocomplete="off"
                    spellcheck="false"
                />

                <button type="button" class="btn" title={toggle_title} disabled={is_empty} onclick={on_toggle}>
                    <span class={classes!("icon", toggle_icon)} />
                </button>

                <button type="button" class="btn" title="Copy to clipboard" disabled={is_empty} onclick={link.callback(|_| SecretInputMsg::Copy)}>
                    <span class="icon clipboard" />
                </button>

                <button type="button" class="btn" title="Clear" disabled={is_empty} onclick={link.callback(|_| SecretInputMsg::Clear)}>
                    <span class="icon x-mark" />
                </button>
            </div>
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum RemoteSourceKind {
    Show,
    Movie,
}

#[derive(Properties, PartialEq)]
pub(super) struct RemoteSourceSelectProps {
    pub(super) remotes: Vec<api::Remote>,
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
            {for props.remotes.iter().map(|remote| {
                let label = remote.source().as_str().to_uppercase();

                html! {
                    <option value={remote.source().as_str().to_owned()} selected={Some(remote.source()) == selected.as_ref()}>{label}</option>
                }
            })}
        </select>
    }
}

/// Sources offered when adding a remote identifier, as `(value, label)`.
const REMOTE_SOURCES: &[(api::RemoteSource, &str)] = &[
    (api::RemoteSource::Tmdb, "TMDB"),
    (api::RemoteSource::Tvdb, "TVDB"),
    (api::RemoteSource::Imdb, "IMDb"),
];

/// Validate a source/value pair and build the `Remote`, or return a
/// user-facing error explaining why the identifier is invalid.
fn parse_remote(source: &api::RemoteSource, value: &str) -> Result<api::Remote, String> {
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

    Ok(api::Remote::new(*source, value))
}

/// Modal for adding, editing and removing remote identifiers (e.g. `tvdb:123`,
/// `imdb:tt0001234`) of a show or movie. The component is presentation-only:
/// it emits `on_add`/`on_edit`/`on_remove` and the caller performs the request,
/// which makes it reusable wherever a remote needs to be repaired.
#[derive(Properties, PartialEq)]
pub(super) struct RemoteEditorProps {
    pub(super) title: String,
    pub(super) kind: RemoteSourceKind,
    pub(super) remotes: Vec<api::RemoteEntry>,
    pub(super) on_add: Callback<(Option<String>, api::Remote)>,
    pub(super) on_edit: Callback<(api::RemoteId, Option<String>, api::Remote)>,
    pub(super) on_remove: Callback<api::RemoteId>,
    pub(super) on_close: Callback<()>,
}

pub(super) enum RemoteEditorMsg {
    SetSource(api::RemoteSource),
    SetValue(String),
    SetSlug(String),
    ToggleSlug,
    ClearSlug,
    Submit,
    Edit(api::RemoteEntry),
    CancelEdit,
    AskRemove(api::RemoteEntry),
    CancelRemove,
    ConfirmRemove(api::RemoteId),
    Close,
}

pub(super) struct RemoteEditor {
    source: api::RemoteSource,
    /// Raw input value for the identifier.
    value: String,
    /// Raw input value for the slug to use for this remote.
    slug: String,
    /// Whether the optional slug input is revealed for editing.
    show_slug: bool,
    /// When set, the form edits the remote with this id instead of adding.
    editing: Option<api::RemoteId>,
    /// When set, awaiting confirmation to remove this identifier.
    confirming_remove: Option<api::RemoteEntry>,
    error: Option<String>,
    /// The source `<select>`; its displayed selection is a DOM property that
    /// must be set imperatively when `source` changes programmatically.
    source_ref: NodeRef,
}

impl RemoteEditor {
    fn reset_form(&mut self) {
        self.source = REMOTE_SOURCES[0].0;
        self.value.clear();
        self.slug.clear();
        self.show_slug = false;
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
            slug: String::new(),
            show_slug: false,
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
            RemoteEditorMsg::SetSlug(slug) => {
                self.slug = slug;
                true
            }
            RemoteEditorMsg::ToggleSlug => {
                self.show_slug = !self.show_slug;
                true
            }
            RemoteEditorMsg::ClearSlug => {
                self.slug.clear();
                true
            }
            RemoteEditorMsg::Submit => {
                let slug = match self.slug.trim() {
                    "" => None,
                    s => Some(s.to_string()),
                };

                match parse_remote(&self.source, &self.value) {
                    Ok(remote) => {
                        match self.editing.take() {
                            Some(id) => {
                                ctx.props().on_edit.emit((id, slug, remote));
                            }
                            None => {
                                ctx.props().on_add.emit((slug, remote));
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
            RemoteEditorMsg::Edit(entry) => {
                self.source = *entry.remote.source();
                self.value = entry.remote.value().to_string();
                self.slug = entry.slug.unwrap_or_default();
                self.show_slug = !self.slug.is_empty();
                self.editing = Some(entry.id);
                self.confirming_remove = None;
                self.error = None;
                true
            }
            RemoteEditorMsg::CancelEdit => {
                self.reset_form();
                true
            }
            RemoteEditorMsg::AskRemove(entry) => {
                self.confirming_remove = Some(entry);
                true
            }
            RemoteEditorMsg::CancelRemove => {
                self.confirming_remove = None;
                true
            }
            RemoteEditorMsg::ConfirmRemove(remote_id) => {
                if self.editing == Some(remote_id) {
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

        let on_slug = link.callback(|e: InputEvent| {
            let input: web_sys::HtmlInputElement = e.target_unchecked_into();
            RemoteEditorMsg::SetSlug(input.value())
        });

        let editing = self.editing.is_some();

        let title = html! {
            <>
                <span class="icon identification" />
                <span>{format!("Identifiers {MDASH} {}", props.title)}</span>
            </>
        };

        html! {
            <Modal {title} on_close={link.callback(|_| RemoteEditorMsg::Close)}>
                if props.remotes.is_empty() {
                    <div class="text-muted">{"No remote identifiers"}</div>
                } else {
                    { for props.remotes.iter().map(|r| {
                        let key = r.remote.to_string();

                        if self.confirming_remove.as_ref() == Some(r) {
                            let remote_id = r.id;

                            return html! {
                                <ConfirmDanger
                                    key={key}
                                    prompt="Remove"
                                    label={r.remote.to_string()}
                                    on_confirm={link.callback(move |_| RemoteEditorMsg::ConfirmRemove(remote_id))}
                                    on_cancel={link.callback(|_| RemoteEditorMsg::CancelRemove)}
                                />
                            };
                        }

                        let editing_this = self.editing == Some(r.id);
                        let edit_entry = r.clone();
                        let remove_entry = r.clone();

                        let url = match props.kind {
                            RemoteSourceKind::Show => r.remote.show_url(r.slug.as_deref()),
                            RemoteSourceKind::Movie => r.remote.movie_url(),
                        };

                        let identifier = html! {
                            <>
                                <span class="item-inline-lg">
                                    <span class={classes!("logo", r.remote.source().as_str().to_owned())} />
                                </span>

                                <span>{r.remote.value().to_string()}</span>

                                if let Some(slug) = r.slug.as_deref() {
                                    <span>{format!("/{slug}")}</span>
                                }

                            </>
                        };

                        html! {
                            <div key={key} class={classes!("row-fill", editing_this.then_some("active"))}>
                                if let Some(url) = url {
                                    <a class="row clickable" href={url} target="_blank" rel="noopener noreferrer" title="Visit remote">
                                        {identifier}
                                    </a>
                                } else {
                                    <div class="row">
                                        {identifier}
                                    </div>
                                }

                                <div class="row end">
                                    <div class="input-group">
                                        <button class="btn" onclick={link.callback(move |_| RemoteEditorMsg::Edit(edit_entry.clone()))} title="Edit identifier">
                                            <span class="icon pencil-square" />
                                        </button>

                                        <button class="btn-danger" onclick={link.callback(move |_| RemoteEditorMsg::AskRemove(remove_entry.clone()))} title="Remove identifier">
                                            <span class="icon trash" />
                                        </button>
                                    </div>
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

                            <button class={classes!("btn", self.show_slug.then_some("selected"))} onclick={link.callback(|_| RemoteEditorMsg::ToggleSlug)} title="Edit slug">
                                <span class="icon link" />
                            </button>

                            <button class="btn-success" onclick={link.callback(|_| RemoteEditorMsg::Submit)} disabled={self.value.trim().is_empty()} title={if editing { "Save identifier" } else { "Add identifier" }}>
                                <span class={classes!("icon", if editing { "check" } else { "plus" })} />
                                <span>{if editing { "Save" } else { "Add" }}</span>
                            </button>

                            if editing {
                                <button class="btn" onclick={link.callback(|_| RemoteEditorMsg::CancelEdit)} title="Cancel edit">
                                    <span class="icon x-mark" />
                                </button>
                            }
                        </div>

                        if self.show_slug {
                            <div class="input-group fill">
                                <span class="input-label" title="Slug">{"/"}</span>
                                <input type="text" class="input-text fill" placeholder="slug" value={self.slug.clone()} oninput={on_slug} />

                                if !self.slug.is_empty() {
                                    <button class="btn" title="Clear slug"
                                        onclick={link.callback(|_| RemoteEditorMsg::ClearSlug)}>
                                        <span class="icon backspace" />
                                    </button>
                                }
                            </div>
                        }

                        if let Some(ref error) = self.error {
                            <label>{error}</label>
                        }
                    </div>
                </div>
            </Modal>
        }
    }
}

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
                    <span class="icon language" />
                    <span>{label}</span>

                    if let Some(code) = self.language_to_country.get_by_part1(code) {
                        <span class={classes!("flag", code)}></span>
                    }
                </button>
            },
            None => html! {
                <button class="btn" onclick={link.callback(|_| Msg::Open)} title="Select language">
                    <span class="icon language" />
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
                        <div class="input-group">
                            <PaginationButtons
                                page={page}
                                total_pages={total_pages}
                                on_page={link.callback(Msg::Page)}
                            />
                        </div>
                    </div>
                </Modal>
            </>
        }
    }
}

const COUNTRY_PAGE_SIZE: usize = 8;

#[derive(Properties, PartialEq)]
pub(super) struct CountryPickerProps {
    /// Selected country alpha-2 codes. Empty means "all countries".
    pub(super) current: Vec<String>,
    pub(super) on_change: Callback<Vec<String>>,
}

pub(super) enum CountryMsg {
    Open,
    Close,
    Filter(String),
    Page(usize),
    Toggle(String),
    All,
}

/// Multi-select picker for countries, modeled on [`LanguagePicker`]. An empty
/// selection represents "all countries".
pub(super) struct CountryPicker {
    countries: Countries,
    open: bool,
    filter: String,
    page: usize,
}

impl Component for CountryPicker {
    type Message = CountryMsg;
    type Properties = CountryPickerProps;

    fn create(_ctx: &Context<Self>) -> Self {
        Self {
            countries: Countries::new(),
            open: false,
            filter: String::new(),
            page: 0,
        }
    }

    fn update(&mut self, ctx: &Context<Self>, msg: Self::Message) -> bool {
        match msg {
            CountryMsg::Open => {
                self.open = true;
                self.filter.clear();
                self.page = 0;
            }
            CountryMsg::Close => {
                self.open = false;
            }
            CountryMsg::Filter(s) => {
                self.filter = s;
                self.page = 0;
            }
            CountryMsg::Page(p) => {
                self.page = p;
            }
            CountryMsg::Toggle(code) => {
                let mut next = ctx.props().current.clone();

                if let Some(pos) = next.iter().position(|c| c.eq_ignore_ascii_case(&code)) {
                    next.remove(pos);
                } else {
                    next.push(code);
                }

                ctx.props().on_change.emit(next);
            }
            CountryMsg::All => {
                ctx.props().on_change.emit(Vec::new());
            }
        }

        true
    }

    fn view(&self, ctx: &Context<Self>) -> Html {
        let link = ctx.link();
        let current = &ctx.props().current;

        let trigger = html! {
            <button class="btn" onclick={link.callback(|_| CountryMsg::Open)} title="Select countries">
                <span class="icon globe-alt" />

                if current.is_empty() {
                    <span>{"All countries"}</span>
                } else {
                    <span>{format!("{} selected", current.len())}</span>
                    {
                        for current.iter().filter_map(|code| {
                            self.countries.flag(code).map(|flag| html! {
                                <span class={classes!("item-inline", "flag", flag)} />
                            })
                        })
                    }
                }
            </button>
        };

        if !self.open {
            return trigger;
        }

        let needle = self.filter.to_lowercase();

        let filtered: Vec<&'static Country> = self
            .countries
            .iter()
            .filter(|country| {
                needle.is_empty()
                    || country.name.to_lowercase().contains(&needle)
                    || country.alpha2.contains(&needle)
            })
            .collect();

        let total_pages = filtered.len().div_ceil(COUNTRY_PAGE_SIZE).max(1);
        let page = self.page.min(total_pages.saturating_sub(1));

        let on_filter = link.callback(|e: InputEvent| {
            let input: web_sys::HtmlInputElement = e.target_unchecked_into();
            CountryMsg::Filter(input.value())
        });

        html! {
            <>
                {trigger}

                <Modal title="Select Countries" on_close={link.callback(|_| CountryMsg::Close)}>
                    <div class="row">
                        <input autofocus={true} type="text" class="input-text fill" placeholder="Filter" value={self.filter.clone()} oninput={on_filter} />
                    </div>

                    <div class="table">
                        <div class="table-entry row clickable" onclick={link.callback(|_| CountryMsg::All)}>
                            <span class="fill">{"All countries"}</span>

                            <span class="item-inline">
                                <span class="icon globe-alt" />
                            </span>

                            <span class="item-inline">
                                <span class={classes!("icon", if current.is_empty() { "check" } else { "x-mark" })} />
                            </span>
                        </div>

                        {
                            for filtered.iter()
                                .skip(page.saturating_mul(COUNTRY_PAGE_SIZE))
                                .take(COUNTRY_PAGE_SIZE)
                                .map(|country| {
                                    let code = country.alpha2;
                                    let selected = current.iter().any(|c| c.eq_ignore_ascii_case(code));

                                    html! {
                                        <div key={code} class={classes!("table-entry", "row", "clickable", selected.then_some("active"))} onclick={link.callback(move |_| CountryMsg::Toggle(code.to_string()))}>
                                            <span class="fill">{country.name}</span>

                                            if let Some(flag) = self.countries.flag(code) {
                                                <span class={classes!("item-inline", "flag", flag)} />
                                            }

                                            <span class="item-inline" title={code}>
                                                <span class={classes!("icon", if selected { "check" } else { "x-mark" })} />
                                            </span>
                                        </div>
                                    }
                                })
                        }
                    </div>

                    <div class="row center">
                        <div class="input-group">
                            <PaginationButtons
                                page={page}
                                total_pages={total_pages}
                                on_page={link.callback(CountryMsg::Page)}
                            />
                        </div>
                    </div>
                </Modal>
            </>
        }
    }
}

/// Release types that may contribute to a movie's release date (excludes `Unknown`).
const RELEASE_TYPES: &[api::ReleaseType] = &[
    api::ReleaseType::Premiere,
    api::ReleaseType::TheatricalLimited,
    api::ReleaseType::Theatrical,
    api::ReleaseType::Digital,
    api::ReleaseType::Physical,
    api::ReleaseType::Tv,
];

#[derive(Properties, PartialEq)]
pub(super) struct ReleaseFiltersEditorProps {
    pub(super) filters: Vec<api::ReleaseFilter>,
    pub(super) on_change: Callback<Vec<api::ReleaseFilter>>,
}

/// Editor for a set of [`api::ReleaseFilter`]s: a checkbox per release type and,
/// when enabled, a [`CountryPicker`] restricting that type to certain countries.
#[function_component]
pub(super) fn ReleaseFiltersEditor(props: &ReleaseFiltersEditorProps) -> Html {
    html! {
        <div class="form">
            {
                for RELEASE_TYPES.iter().copied().map(|rt| {
                    let existing = props.filters.iter().find(|f| f.release_type == rt);
                    let enabled = existing.is_some();
                    let countries = existing.map(|f| f.countries.clone()).unwrap_or_default();

                    let on_toggle = {
                        let filters = props.filters.clone();
                        let cb = props.on_change.clone();
                        Callback::from(move |_: MouseEvent| {
                            let mut next = filters.clone();
                            if let Some(pos) = next.iter().position(|f| f.release_type == rt) {
                                next.remove(pos);
                            } else {
                                next.push(api::ReleaseFilter { release_type: rt, countries: Vec::new() });
                            }
                            cb.emit(next);
                        })
                    };

                    let on_countries = {
                        let filters = props.filters.clone();
                        let cb = props.on_change.clone();
                        Callback::from(move |countries: Vec<String>| {
                            let mut next = filters.clone();
                            if let Some(f) = next.iter_mut().find(|f| f.release_type == rt) {
                                f.countries = countries;
                            }
                            cb.emit(next);
                        })
                    };

                    html! {
                        <div class="field">
                            <label class="clickable" onclick={on_toggle.clone()}>{rt.as_str()}</label>
                            <div class="row input-group">
                                <span class={classes!("input-checkbox", enabled.then_some("checked"))} onclick={on_toggle}>
                                    <span class="mark" />
                                </span>

                                if enabled {
                                    <CountryPicker current={countries} on_change={on_countries} />
                                }
                            </div>
                        </div>
                    }
                })
            }
        </div>
    }
}

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
    pub(super) show_id: api::ShowId,
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
            .expect("Expected ws::Handle in context");

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
                show_id: ctx.props().show_id,
                season,
            })
            .on_packet(ctx.link().callback(EpisodePickerMsg::EpisodesLoaded))
            .send();
    }
}
