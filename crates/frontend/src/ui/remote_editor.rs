use web_sys::{Event, InputEvent};
use yew::prelude::*;

use super::{ConfirmDanger, MDASH, Modal};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum RemoteSourceKind {
    Show,
    Movie,
}

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
                return Err(format!("{} identifier must be a number", source.as_label()));
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
pub(crate) struct Props {
    pub(crate) title: String,
    pub(crate) kind: RemoteSourceKind,
    pub(crate) remotes: Vec<api::RemoteEntry>,
    pub(crate) on_add: Callback<(Option<String>, api::Remote)>,
    pub(crate) on_edit: Callback<(api::RemoteId, Option<String>, api::Remote)>,
    pub(crate) on_remove: Callback<api::RemoteId>,
    pub(crate) on_set_enabled: Callback<(api::RemoteId, bool)>,
    pub(crate) on_reorder: Callback<Vec<api::RemoteId>>,
    pub(crate) on_set_sync_kinds: Callback<(api::RemoteId, Option<api::SyncKindSet>)>,
    pub(crate) global_sync_kinds: Vec<api::SourceSyncKinds>,
    pub(crate) on_close: Callback<()>,
}

pub(crate) enum Msg {
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
    SetEnabled(api::RemoteId, bool),
    SetSyncKinds(api::RemoteId, Option<api::SyncKindSet>),
    Move(usize, isize),
    Close,
}

pub(crate) struct RemoteEditor {
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
    /// When set, display this error message related to the identifier form.
    error: Option<String>,
    /// The source `<select>`; its displayed selection is a DOM property that
    /// must be set imperatively when `source` changes programmatically.
    source_ref: NodeRef,
}

impl RemoteEditor {
    fn reset_form(&mut self) {
        self.source = api::RemoteSource::Tmdb;
        self.value.clear();
        self.slug.clear();
        self.show_slug = false;
        self.editing = None;
        self.error = None;
    }
}

impl Component for RemoteEditor {
    type Message = Msg;
    type Properties = Props;

    fn create(_ctx: &Context<Self>) -> Self {
        Self {
            source: api::RemoteSource::Tmdb,
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
            select.set_value(self.source.as_id());
        }
    }

    fn update(&mut self, ctx: &Context<Self>, msg: Self::Message) -> bool {
        match msg {
            Msg::SetSource(source) => {
                self.source = source;
                self.error = None;
                true
            }
            Msg::SetValue(value) => {
                self.value = value;
                self.error = None;
                true
            }
            Msg::SetSlug(slug) => {
                self.slug = slug;
                true
            }
            Msg::ToggleSlug => {
                self.show_slug = !self.show_slug;
                true
            }
            Msg::ClearSlug => {
                self.slug.clear();
                true
            }
            Msg::Submit => {
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
            Msg::Edit(entry) => {
                self.source = *entry.remote.source();
                self.value = entry.remote.value().to_string();
                self.slug = entry.slug.unwrap_or_default();
                self.show_slug = !self.slug.is_empty();
                self.editing = Some(entry.id);
                self.confirming_remove = None;
                self.error = None;
                true
            }
            Msg::CancelEdit => {
                self.reset_form();
                true
            }
            Msg::AskRemove(entry) => {
                self.confirming_remove = Some(entry);
                true
            }
            Msg::CancelRemove => {
                self.confirming_remove = None;
                true
            }
            Msg::ConfirmRemove(remote_id) => {
                if self.editing == Some(remote_id) {
                    self.reset_form();
                }

                self.confirming_remove = None;
                ctx.props().on_remove.emit(remote_id);
                true
            }
            Msg::SetEnabled(remote_id, enabled) => {
                ctx.props().on_set_enabled.emit((remote_id, enabled));
                false
            }
            Msg::SetSyncKinds(remote_id, sync_kinds) => {
                ctx.props().on_set_sync_kinds.emit((remote_id, sync_kinds));
                false
            }
            Msg::Move(index, delta) => {
                let mut ids: Vec<api::RemoteId> =
                    ctx.props().remotes.iter().map(|r| r.id).collect();
                let target = index as isize + delta;

                if target >= 0 && (target as usize) < ids.len() {
                    ids.swap(index, target as usize);
                    ctx.props().on_reorder.emit(ids);
                }

                false
            }
            Msg::Close => {
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

            let source = api::RemoteSource::ALL
                .iter()
                .find(|source| source.as_id() == value)
                .copied()
                .unwrap_or(api::RemoteSource::Tmdb);

            Msg::SetSource(source)
        });

        let on_value = link.callback(|e: InputEvent| {
            let input: web_sys::HtmlInputElement = e.target_unchecked_into();
            Msg::SetValue(input.value())
        });

        let on_slug = link.callback(|e: InputEvent| {
            let input: web_sys::HtmlInputElement = e.target_unchecked_into();
            Msg::SetSlug(input.value())
        });

        let editing = self.editing.is_some();

        let title = html! {
            <>
                <span class="icon identification" />
                <span>{format!("Remotes {MDASH} {}", props.title)}</span>
            </>
        };

        html! {
            <Modal {title} on_close={link.callback(|_| Msg::Close)}>
                if props.remotes.is_empty() {
                    <div class="text-muted">{"No remotes"}</div>
                } else {
                    { for props.remotes.iter().enumerate().map(|(index, r)| {
                        let key = r.remote.to_string();
                        let count = props.remotes.len();

                        if self.confirming_remove.as_ref() == Some(r) {
                            let remote_id = r.id;

                            return html! {
                                <ConfirmDanger
                                    key={key}
                                    prompt="Remove"
                                    label={r.remote.to_string()}
                                    on_confirm={link.callback(move |_| Msg::ConfirmRemove(remote_id))}
                                    on_cancel={link.callback(|_| Msg::CancelRemove)}
                                />
                            };
                        }

                        let editing_this = self.editing == Some(r.id);
                        let edit_entry = r.clone();
                        let remove_entry = r.clone();
                        let enable_id = r.id;
                        let enabled = r.enabled;

                        // Per-remote sync-kind selection: show the effective set
                        // (this remote's override, else the global default for its
                        // source), clamped to what the source can provide.
                        let source = *r.remote.source();
                        let capability = source.default_sync_kinds();
                        let global_default = props
                            .global_sync_kinds
                            .iter()
                            .find(|s| s.source == source)
                            .map(|s| s.kinds)
                            .unwrap_or(capability)
                            .intersect(capability);

                        let effective = r.sync_kinds.unwrap_or(global_default).intersect(capability);
                        let overriding = r.sync_kinds.is_some();
                        let sync_id = r.id;

                        let kind_toggles = (!capability.is_empty()).then(|| html! {
                            <div class="input-group" title="Kinds synced from this source">
                                { for capability.iter().map(|kind| {
                                    let on = effective.contains(kind);
                                    let next = effective.with(kind, !on);
                                    html! {
                                        <span
                                            class={classes!("input-checkbox", on.then_some("checked"))}
                                            onclick={link.callback(move |_| Msg::SetSyncKinds(sync_id, Some(next)))}
                                            title={kind.as_label()}
                                        >
                                            <span class="mark" />
                                            <span>{kind.as_label()}</span>
                                        </span>
                                    }
                                }) }

                                if overriding {
                                    <button class="btn" onclick={link.callback(move |_| Msg::SetSyncKinds(sync_id, None))} title="Reset to global default">
                                        <span class="icon arrow-uturn-left" />
                                    </button>
                                }
                            </div>
                        });

                        let url = match props.kind {
                            RemoteSourceKind::Show => r.remote.show_url(r.slug.as_deref()),
                            RemoteSourceKind::Movie => r.remote.movie_url(),
                        };

                        let identifier = html! {
                            <>
                                <span class="item-inline-lg">
                                    <span class={classes!("logo", r.remote.source().as_id())} />
                                </span>

                                <span>{r.remote.value().to_string()}</span>

                                if let Some(slug) = r.slug.as_deref() {
                                    <span>{format!("/{slug}")}</span>
                                }

                            </>
                        };

                        html! {
                            <div class="column">
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
                                            <button class="btn" disabled={index == 0} onclick={link.callback(move |_| Msg::Move(index, -1))} title="Higher priority">
                                                <span class="icon chevron-up" />
                                            </button>

                                            <button class="btn" disabled={index + 1 == count} onclick={link.callback(move |_| Msg::Move(index, 1))} title="Lower priority">
                                                <span class="icon chevron-down" />
                                            </button>

                                            <button class="btn" onclick={link.callback(move |_| Msg::Edit(edit_entry.clone()))} title="Edit identifier">
                                                <span class="icon pencil-square" />
                                            </button>

                                            <button class="btn-danger" onclick={link.callback(move |_| Msg::AskRemove(remove_entry.clone()))} title="Remove identifier">
                                                <span class="icon trash" />
                                            </button>
                                        </div>
                                    </div>
                                </div>

                                <div class="row-fill">
                                    <div class="row">
                                        { for kind_toggles }
                                    </div>

                                    <div class="row end">
                                        <span class={classes!("input-checkbox", enabled.then_some("checked"))} onclick={link.callback(move |_| Msg::SetEnabled(enable_id, !enabled))} title="Use this source for air dates and sync">
                                            <span class="mark" />
                                        </span>
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
                                { for api::RemoteSource::ALL.iter().map(|source| html! {
                                    <option value={source.as_id()} selected={self.source == *source}>{source.as_label()}</option>
                                }) }
                            </select>

                            <input type="text" class="input-text fill" placeholder="Identifier" value={self.value.clone()} oninput={on_value} />

                            <button class={classes!("btn", self.show_slug.then_some("selected"))} onclick={link.callback(|_| Msg::ToggleSlug)} title="Edit slug">
                                <span class="icon link" />
                            </button>

                            <button class="btn-success" onclick={link.callback(|_| Msg::Submit)} disabled={self.value.trim().is_empty()} title={if editing { "Save identifier" } else { "Add identifier" }}>
                                <span class={classes!("icon", if editing { "check" } else { "plus" })} />
                                <span>{if editing { "Save" } else { "Add" }}</span>
                            </button>

                            if editing {
                                <button class="btn" onclick={link.callback(|_| Msg::CancelEdit)} title="Cancel edit">
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
                                        onclick={link.callback(|_| Msg::ClearSlug)}>
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
