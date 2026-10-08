use web_sys::{Event, InputEvent};
use yew::prelude::*;

use crate::help;
use crate::ui::{ContextMenu, Help};

use super::{Button, ConfirmDanger, DragHandle, Modal, Reorder, Variant};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum RemoteSourceKind {
    Show,
    Movie,
    Person,
}

/// Validate a source/value pair and build the `Remote`, or return a
/// user-facing error explaining why the identifier is invalid.
fn parse_remote(source: &api::RemoteSource, value: &str) -> Result<api::Remote, String> {
    let value = value.trim();

    if value.is_empty() {
        return Err("Identifier must not be empty".to_string());
    }

    let value = match *source {
        api::RemoteSource::Tvdb
        | api::RemoteSource::Tmdb
        | api::RemoteSource::Tvmaze
        | api::RemoteSource::Anidb => {
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
        api::RemoteSource::Xem => {
            let valid = value.split_once('/').is_some_and(|(origin, id)| {
                matches!(origin, "tvdb" | "anidb")
                    && !id.is_empty()
                    && id.bytes().all(|b| b.is_ascii_digit())
            });

            if !valid {
                return Err("XEM identifier must look like tvdb/424536 or anidb/17617".to_string());
            }

            api::RemoteValue::Str(value.to_string())
        }
        api::RemoteSource::Scene => api::RemoteValue::Str(value.to_string()),
        _ => {
            return Err("Unknown remote source".to_string());
        }
    };

    Ok(api::Remote::new(*source, value))
}

/// The sources offered when adding a remote. XEM, AniDB and scene only number
/// show episodes.
fn addable_sources(kind: RemoteSourceKind) -> impl Iterator<Item = api::RemoteSource> {
    api::RemoteSource::ALL
        .iter()
        .copied()
        .filter(move |source| {
            kind == RemoteSourceKind::Show
                || !matches!(
                    source,
                    api::RemoteSource::Xem | api::RemoteSource::Anidb | api::RemoteSource::Scene
                )
        })
}

fn placeholder(source: api::RemoteSource) -> &'static str {
    match source {
        api::RemoteSource::Scene => "Scene name",
        api::RemoteSource::Anidb => "Anime id",
        api::RemoteSource::Xem => "tvdb/<id>",
        _ => "Identifier",
    }
}

fn hint(source: api::RemoteSource) -> Option<&'static str> {
    match source {
        api::RemoteSource::Scene => Some("Scene names have no link."),
        api::RemoteSource::Anidb => Some(
            "AniDB takes an anime id (anidb.net/anime/17617); a show split into cours can have one per cour.",
        ),
        api::RemoteSource::Xem => Some("XEM takes tvdb/<id> or anidb/<id>."),
        _ => None,
    }
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
    pub(crate) on_purge_cache: Callback<api::RemoteId>,
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
    AskRemove(api::RemoteId),
    CancelRemove,
    ConfirmRemove(api::RemoteId),
    PurgeCache(api::RemoteId),
    SetEnabled(api::RemoteId, bool),
    SetSyncKinds(api::RemoteId, Option<api::SyncKindSet>),
    Move(usize, usize),
    Close,
}

struct RemoteState {
    context_anchor: NodeRef,
    remote: api::RemoteEntry,
}

impl PartialEq<api::RemoteEntry> for RemoteState {
    #[inline]
    fn eq(&self, other: &api::RemoteEntry) -> bool {
        self.remote == *other
    }
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
    confirming_remove: Option<api::RemoteId>,
    /// When set, display this error message related to the identifier form.
    error: Option<String>,
    /// The source `<select>`; its displayed selection is a DOM property that
    /// must be set imperatively when `source` changes programmatically.
    source_ref: NodeRef,
    remotes: Vec<RemoteState>,
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

    fn populate_remotes(&mut self, ctx: &Context<Self>) {
        for (r, o) in ctx.props().remotes.iter().zip(self.remotes.iter_mut()) {
            o.remote = r.clone();
        }

        for remote in ctx.props().remotes.iter().skip(self.remotes.len()) {
            self.remotes.push(RemoteState {
                context_anchor: NodeRef::default(),
                remote: remote.clone(),
            });
        }

        if self.remotes.len() > ctx.props().remotes.len() {
            self.remotes.truncate(ctx.props().remotes.len());
        }
    }
}

impl Component for RemoteEditor {
    type Message = Msg;
    type Properties = Props;

    fn create(ctx: &Context<Self>) -> Self {
        let mut this = Self {
            source: api::RemoteSource::Tmdb,
            value: String::new(),
            slug: String::new(),
            show_slug: false,
            editing: None,
            confirming_remove: None,
            error: None,
            source_ref: NodeRef::default(),
            remotes: Vec::new(),
        };

        this.populate_remotes(ctx);
        this
    }

    fn changed(&mut self, ctx: &Context<Self>, _old: &Self::Properties) -> bool {
        if self.remotes != ctx.props().remotes {
            self.populate_remotes(ctx);
        }

        true
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
            Msg::AskRemove(id) => {
                self.confirming_remove = Some(id);
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
            Msg::PurgeCache(remote_id) => {
                ctx.props().on_purge_cache.emit(remote_id);
                false
            }
            Msg::SetEnabled(remote_id, enabled) => {
                ctx.props().on_set_enabled.emit((remote_id, enabled));
                false
            }
            Msg::SetSyncKinds(remote_id, sync_kinds) => {
                ctx.props().on_set_sync_kinds.emit((remote_id, sync_kinds));
                false
            }
            Msg::Move(from, to) => {
                let mut identifiers: Vec<api::RemoteId> =
                    ctx.props().remotes.iter().map(|r| r.id).collect();

                if from < identifiers.len() && to < identifiers.len() {
                    let id = identifiers.remove(from);
                    identifiers.insert(to, id);
                    ctx.props().on_reorder.emit(identifiers);
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

        html! {
            <Modal icon="identification" title="Remotes" on_close={link.callback(|_| Msg::Close)}>
                if props.remotes.is_empty() {
                    <div class="table">
                        <div class="text-muted">{"No remotes"}</div>
                    </div>
                } else {
                    <Reorder class="table" on_move={link.callback(|(from, to)| Msg::Move(from, to))}>
                        { for self.remotes.iter().enumerate().map(|(index, r)| {
                            let key = r.remote.remote.to_string();

                            let edit_entry = r.remote.clone();
                            let id = r.remote.id;
                            let enabled = r.remote.enabled;

                            let source = *r.remote.remote.source();
                            // A person is a single sync unit, so the per-kind override
                            // toggles don't apply; suppress them for the person editor.
                            let capability = match props.kind {
                                RemoteSourceKind::Person => api::SyncKindSet::empty(),
                                _ => source.default_sync_kinds(),
                            };

                            let global_default = props
                                .global_sync_kinds
                                .iter()
                                .find(|s| s.source == source)
                                .map(|s| s.kinds)
                                .unwrap_or(capability)
                                .intersect(capability);

                            let effective = r.remote.sync_kinds.unwrap_or(global_default).intersect(capability);
                            let overriding = r.remote.sync_kinds.is_some();

                            let kind_toggles = (!capability.is_empty()).then(|| html! {
                                <div class="remote-kinds" role="group" aria-label="Kinds synced from this source">
                                    <span class="remote-caption">{"Syncs"}</span>
                                    <Help section={help::REMOTES} />

                                    { for capability.iter().map(|kind| {
                                        let on = effective.contains(kind);
                                        let next = effective.with(kind, !on);

                                        html! {
                                            <Button class={classes!("input-checkbox", "has-text", on.then_some("checked"))} role="switch" checked={Some(on)} title={kind.as_label()} onclick={link.callback(move |_| Msg::SetSyncKinds(id, Some(next)))}>
                                                <span class="mark" />
                                                <span>{kind.as_label()}</span>
                                            </Button>
                                        }
                                    }) }

                                    if overriding {
                                        <Button icon="arrow-uturn-left" label="Use default" title="Reset to global default" onclick={link.callback(move |_| Msg::SetSyncKinds(id, None))} />
                                    }
                                </div>
                            });

                            let url = match props.kind {
                                RemoteSourceKind::Show => r.remote.remote.show_url(r.remote.slug.as_deref()),
                                RemoteSourceKind::Movie => r.remote.remote.movie_url(),
                                RemoteSourceKind::Person => r.remote.remote.person_url(r.remote.slug.as_deref()),
                            };

                            let identifier = html! {
                                <>
                                    <span class={classes!("logo", r.remote.remote.source().as_id())} aria-hidden="true" />

                                    <span class="remote-id">
                                        {r.remote.remote.value().to_string()}

                                        if let Some(slug) = r.remote.slug.as_deref() {
                                            {format!("/{slug}")}
                                        }
                                    </span>
                                </>
                            };

                            html! {
                                <div {key} class="remote">
                                    <div class="remote-head">
                                        <DragHandle {index} />

                                        if let Some(url) = url {
                                            <a class="remote-link" href={url} target="_blank" rel="noopener noreferrer" title={format!("Open on {}", r.remote.remote.source())}>
                                                {identifier}
                                            </a>
                                        } else {
                                            <span class="remote-link">{identifier}</span>
                                        }

                                        <div class="remote-actions" ref={r.context_anchor.clone()}>
                                            <Button class={classes!("input-checkbox", "has-text", enabled.then_some("checked"))} role="switch" checked={Some(enabled)} title="Enable this remote" onclick={link.callback(move |_| Msg::SetEnabled(id, !enabled))}>
                                                <span class="mark" />
                                                <span>{if enabled { "Enabled" } else { "Disabled" }}</span>
                                            </Button>

                                            <Button icon="pencil-square" label="Edit" title="Edit identifier" onclick={link.callback(move |_| Msg::Edit(edit_entry.clone()))} />
                                            <Button icon="trash" label="Remove" title="Remove identifier" expanded={Some(self.confirming_remove == Some(id))} haspopup="dialog" onclick={link.callback(move |_| Msg::AskRemove(id))} />
                                        </div>

                                        if self.confirming_remove == Some(id) {
                                            <ContextMenu prompt="Remove" label={r.remote.remote.to_string()} anchor={r.context_anchor.clone()} on_close={link.callback(|_| Msg::CancelRemove)}>
                                                <ConfirmDanger
                                                    on_confirm={link.callback(move |_| Msg::ConfirmRemove(id))}
                                                    on_cancel={link.callback(|_| Msg::CancelRemove)}
                                                />
                                            </ContextMenu>
                                        }
                                    </div>

                                    {kind_toggles}

                                    if let Some(ref cache) = r.remote.cache {
                                        <div class="remote-cache">
                                            <dl>
                                                if let Some(ref last_updated) = cache.last_updated {
                                                    <dt>{"Last updated"}</dt>
                                                    <dd>{last_updated}</dd>
                                                }

                                                if let Some(ref etag) = cache.etag {
                                                    <dt>{"ETag"}</dt>
                                                    <dd class="remote-etag" title={etag.clone()}>{etag}</dd>
                                                }

                                                if !cache.kinds.is_empty() {
                                                    <dt>{"Cached"}</dt>
                                                    <dd>{cache.kinds.iter().map(|k| k.as_label()).collect::<Vec<_>>().join(", ")}</dd>
                                                }
                                            </dl>

                                            <Button icon="arrow-path" label="Clear cache" title="Clear cache and force resync" onclick={link.callback(move |_| Msg::PurgeCache(id))} />

                                            // Sub-requests that failed on the last sync. They are
                                            // suppressed (not retried) until "Retries", so a source
                                            // that simply doesn't carry an entity stops costing a
                                            // call every sync - at the price of a silently partial
                                            // entity, which is why they are surfaced here.
                                            if !cache.errors.is_empty() {
                                                <div class="remote-errors">
                                                    <span class="remote-caption">{"Kind"}</span>
                                                    <span class="remote-caption">{"Request"}</span>
                                                    <span class="remote-caption">{"Error"}</span>
                                                    <span class="remote-caption">{"Retries"}</span>

                                                    { for cache.errors.iter().map(|e| html! {
                                                        <>
                                                            <span>{e.kind.as_label()}</span>
                                                            <span class="remote-id">{&e.key}</span>
                                                            <span>{&e.message}</span>
                                                            <span>{e.expires_at().to_string()}</span>
                                                        </>
                                                    }) }
                                                </div>
                                            }
                                        </div>
                                    }
                                </div>
                            }
                        }) }
                    </Reorder>
                }

                <div class="form">
                    <div class={classes!("field", self.error.is_some().then_some("error"))}>
                        <div class="input-group fill">
                            <select ref={self.source_ref.clone()} class="input-select" onchange={on_source} title="Source">
                                { for addable_sources(props.kind).map(|source| html! {
                                    <option value={source.as_id()} selected={self.source == source}>{source.as_label()}</option>
                                }) }
                            </select>

                            <input class="input-text fill" type="text" placeholder={placeholder(self.source)} aria-label="Identifier" value={self.value.clone()} oninput={on_value} />

                            <Button icon="link" label="Slug" title="Edit slug" class={classes!(self.show_slug.then_some("selected"))} pressed={Some(self.show_slug)} onclick={link.callback(|_| Msg::ToggleSlug)} />

                            <Button icon={if editing { "check" } else { "plus" }} label={if editing { "Save" } else { "Add" }} title={if editing { "Save identifier" } else { "Add identifier" }} variant={Variant::Primary} disabled={self.value.trim().is_empty()} onclick={link.callback(|_| Msg::Submit)} />

                            if editing {
                                <Button icon="x-mark" title="Cancel edit" onclick={link.callback(|_| Msg::CancelEdit)} />
                            }
                        </div>

                        if self.show_slug {
                            <div class="input-group fill">
                                <span class="input-label has-text" title="Slug">{"/"}</span>
                                <input class="input-text fill" type="text" placeholder="slug" value={self.slug.clone()} oninput={on_slug} />

                                if !self.slug.is_empty() {
                                    <Button icon="backspace" title="Clear slug" onclick={link.callback(|_| Msg::ClearSlug)} />
                                }
                            </div>
                        }

                        if let Some(ref error) = self.error {
                            <span class="field-error" role="alert">{error}</span>
                        }

                        if let Some(hint) = hint(self.source) {
                            <span class="hint">{hint}</span>
                        }
                    </div>
                </div>
            </Modal>
        }
    }
}
