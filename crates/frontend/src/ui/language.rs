use web_sys::{InputEvent, MouseEvent};
use yew::prelude::*;

use crate::Modal;

use super::PaginationButtons;

/// App-wide context: the most-used custom language codes (ISO 639-1), ordered
/// most-used first, recomputed periodically by the backend. Surfaced as quick
/// picks in every [`LanguagePicker`]. Provided by `App`.
#[derive(Clone, Default, PartialEq)]
pub(crate) struct TopLanguages(pub(crate) Vec<api::Language>);

const LANGUAGE_PAGE_SIZE: usize = 5;

#[derive(Properties, PartialEq)]
pub(crate) struct LanguagePickerProps {
    pub(crate) current: api::Language,
    pub(crate) on_change: Callback<api::Language>,
    pub(crate) placeholder: &'static str,
    /// Whether to show the "top languages" quick-pick section. Disable for
    /// list-style usages (e.g. configuring which languages to sync).
    #[prop_or(true)]
    pub(crate) show_top: bool,
}

pub(crate) enum LanguagePickerMsg {
    Open,
    Close,
    Pick(api::Language),
}

/// A button showing the current language that opens a [`LanguageModal`] to
/// change it. Used where a value is displayed and edited in place.
pub(crate) struct LanguagePicker {
    open: bool,
}

impl Component for LanguagePicker {
    type Message = LanguagePickerMsg;
    type Properties = LanguagePickerProps;

    fn create(_ctx: &Context<Self>) -> Self {
        Self { open: false }
    }

    fn update(&mut self, ctx: &Context<Self>, msg: Self::Message) -> bool {
        match msg {
            LanguagePickerMsg::Open => {
                self.open = true;
            }
            LanguagePickerMsg::Close => {
                self.open = false;
            }
            LanguagePickerMsg::Pick(value) => {
                self.open = false;
                ctx.props().on_change.emit(value);
            }
        }

        true
    }

    fn view(&self, ctx: &Context<Self>) -> Html {
        let link = ctx.link();
        let props = ctx.props();

        let current = props.current;

        let value = if current.is_default() {
            None
        } else {
            current
                .to_iso639_3()
                .and_then(iso639::by_id)
                .map(|entry| (entry.ref_name, entry.part1))
        };

        let trigger = match value {
            Some((label, part1)) => html! {
                <button class="btn" onclick={link.callback(|_| LanguagePickerMsg::Open)} title="Select language">
                    <span class="icon language" />
                    <span>{label}</span>

                    if let Some(code) = part1.and_then(iso639::country_by_part1) {
                        <span class={classes!("flag", code)}></span>
                    }
                </button>
            },
            None => html! {
                <button class="btn" onclick={link.callback(|_| LanguagePickerMsg::Open)} title="Select language">
                    <span class="icon language" />
                    <span>{props.placeholder}</span>
                </button>
            },
        };

        if !self.open {
            return trigger;
        }

        html! {
            <>
                {trigger}

                <LanguageModal
                    current={current}
                    placeholder={props.placeholder}
                    title="Select Language"
                    show_top={props.show_top}
                    on_pick={link.callback(LanguagePickerMsg::Pick)}
                    on_close={link.callback(|_| LanguagePickerMsg::Close)}
                />
            </>
        }
    }
}

#[derive(Properties, PartialEq)]
pub(crate) struct LanguageModalProps {
    /// The currently-selected language, highlighted in the list. Pass
    /// [`api::Language::DEFAULT`] together with `allow_default = false` for a
    /// "fresh pick" with nothing pre-selected.
    pub(crate) current: api::Language,
    pub(crate) on_pick: Callback<api::Language>,
    pub(crate) on_close: Callback<()>,
    /// Label for the "Default" row.
    pub(crate) placeholder: &'static str,
    pub(crate) title: &'static str,
    /// Whether to show the "top languages" quick-pick section.
    #[prop_or(true)]
    pub(crate) show_top: bool,
    /// Whether to offer the "Default" (original language) row.
    #[prop_or(true)]
    pub(crate) allow_default: bool,
}

pub(crate) enum LanguageModalMsg {
    Filter(String),
    Page(usize),
    Pick(api::Language),
    Close,
    SetTopLanguages(TopLanguages),
}

/// The language-selection modal: a filterable, paginated list of languages with
/// optional "Default" and top-languages quick-pick rows. Pure selection UI with
/// no trigger of its own, so callers control when it opens.
pub(crate) struct LanguageModal {
    filter: String,
    page: usize,
    top_languages: Vec<api::Language>,
    _top_languages_handle: ContextHandle<TopLanguages>,
}

impl Component for LanguageModal {
    type Message = LanguageModalMsg;
    type Properties = LanguageModalProps;

    fn create(ctx: &Context<Self>) -> Self {
        let (top_languages, _top_languages_handle) = ctx
            .link()
            .context::<TopLanguages>(ctx.link().callback(LanguageModalMsg::SetTopLanguages))
            .expect("Expected TopLanguages in context");

        Self {
            filter: String::new(),
            page: 0,
            top_languages: top_languages.0,
            _top_languages_handle,
        }
    }

    fn update(&mut self, ctx: &Context<Self>, msg: Self::Message) -> bool {
        match msg {
            LanguageModalMsg::Filter(s) => {
                self.filter = s;
                self.page = 0;
            }
            LanguageModalMsg::Page(p) => {
                self.page = p;
            }
            LanguageModalMsg::Pick(value) => {
                ctx.props().on_pick.emit(value);
            }
            LanguageModalMsg::Close => {
                ctx.props().on_close.emit(());
            }
            LanguageModalMsg::SetTopLanguages(top_languages) => {
                self.top_languages = top_languages.0;
            }
        }

        true
    }

    fn view(&self, ctx: &Context<Self>) -> Html {
        let link = ctx.link();
        let props = ctx.props();

        let current = props.current;

        let needle = self.filter.to_lowercase();
        let filtered: Vec<&'static iso639::Entry> = iso639::iter()
            .filter(|entry| {
                entry.part1.is_some()
                    && (needle.is_empty() || entry.ref_name.to_lowercase().contains(&needle))
            })
            .collect();

        let total_pages = filtered.len().div_ceil(LANGUAGE_PAGE_SIZE).max(1);
        let page = self.page.min(total_pages.saturating_sub(1));

        let on_filter = link.callback(|e: InputEvent| {
            let input: web_sys::HtmlInputElement = e.target_unchecked_into();
            LanguageModalMsg::Filter(input.value())
        });

        html! {
            <Modal title={props.title} on_close={link.callback(|_| LanguageModalMsg::Close)}>
                <div class="row">
                    <input autofocus={true} type="text" class="input-text fill" placeholder="Filter" value={self.filter.clone()} oninput={on_filter} />
                </div>

                <div class="table">
                    // Quick picks: the most-used custom languages, shown right
                    // below "Default". Hidden while filtering to avoid duplicates,
                    // and entirely when `show_top` is disabled.
                    if self.filter.is_empty() {
                        if props.allow_default {
                            <div class="table-entry row clickable" onclick={link.callback(|_| LanguageModalMsg::Pick(api::Language::DEFAULT))}>
                                <span class="fill">{props.placeholder}</span>

                                if current.is_default() {
                                    <span class="item-inline">
                                        <span class="icon check" />
                                    </span>
                                }

                                <span class="item-inline">
                                    <span class="icon icon-4x3 language" />
                                </span>
                            </div>
                        }

                        if props.show_top {
                            { for self.top_languages.iter().filter_map(|code| {
                                let code = *code;
                                let entry = code.to_iso639_3().and_then(iso639::by_id)?;
                                let selected = current == code;

                                Some(html! {
                                    <div key={format!("top-{code}")} class={classes!("table-entry", "row", "clickable", selected.then_some("active"))} onclick={link.callback(move |_| LanguageModalMsg::Pick(code))}>
                                        <span class="fill">{entry.ref_name}</span>

                                        if selected {
                                            <span class="item-inline">
                                                <span class="icon check" />
                                            </span>
                                        }

                                        if let Some(country) = entry.part1.and_then(iso639::country_by_part1) {
                                            <span class={classes!("item-inline", "flag", country)} />
                                        } else {
                                            <span class="item-inline">
                                                <span class="text-muted">{entry.id}</span>
                                            </span>
                                        }
                                    </div>
                                })
                            }) }
                        }
                    }

                    if !filtered.is_empty() {
                        <div class="table-separator" />
                    }

                    {
                        for filtered.iter()
                            .skip(page.saturating_mul(LANGUAGE_PAGE_SIZE))
                            .take(LANGUAGE_PAGE_SIZE)
                            .map(|entry| {
                                let code = api::Language::from_iso639(entry.id)
                                    .unwrap_or(api::Language::DEFAULT);
                                let selected = current == code;

                                html! {
                                    <div key={entry.id} class={classes!("table-entry", "row", "clickable", selected.then_some("active"))} onclick={link.callback(move |_| LanguageModalMsg::Pick(code))}>
                                        <span class="fill">{entry.ref_name}</span>

                                        if selected {
                                            <span class="item-inline">
                                                <span class="icon check" />
                                            </span>
                                        }

                                        if let Some(country) = entry.part1.and_then(iso639::country_by_part1) {
                                            <span class={classes!("item-inline", "flag", country)} />
                                        } else {
                                            <span class="item-inline">
                                                <span class="text-muted">{entry.id}</span>
                                            </span>
                                        }
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
                            on_page={link.callback(LanguageModalMsg::Page)}
                        />
                    </div>
                </div>
            </Modal>
        }
    }
}

#[derive(Properties, PartialEq)]
pub(crate) struct SyncLanguagesEditorProps {
    pub(crate) languages: Vec<api::Language>,
    pub(crate) on_change: Callback<Vec<api::Language>>,
}

pub(crate) enum SyncLanguagesMsg {
    Open,
    Close,
    Add(api::Language),
}

/// Editor for the list of languages the sync path populates. Renders each
/// selected language with a remove button, plus an "Add language" button that
/// opens a [`LanguageModal`] to pick a fresh language. `Default` stands for each
/// media's own original language and can't be added here.
pub(crate) struct SyncLanguagesEditor {
    open: bool,
}

impl Component for SyncLanguagesEditor {
    type Message = SyncLanguagesMsg;
    type Properties = SyncLanguagesEditorProps;

    fn create(_ctx: &Context<Self>) -> Self {
        Self { open: false }
    }

    fn update(&mut self, ctx: &Context<Self>, msg: Self::Message) -> bool {
        match msg {
            SyncLanguagesMsg::Open => {
                self.open = true;
            }
            SyncLanguagesMsg::Close => {
                self.open = false;
            }
            SyncLanguagesMsg::Add(code) => {
                self.open = false;

                let current = &ctx.props().languages;

                if !current.contains(&code) {
                    let mut next = current.clone();
                    next.push(code);
                    ctx.props().on_change.emit(next);
                }
            }
        }

        true
    }

    fn view(&self, ctx: &Context<Self>) -> Html {
        let link = ctx.link();
        let props = ctx.props();

        html! {
            <div class="table">
                {
                    for props.languages.iter().copied().enumerate().map(|(index, code)| {
                        let on_remove = {
                            let current = props.languages.clone();
                            let on_change = props.on_change.clone();
                            Callback::from(move |_: MouseEvent| {
                                let mut next = current.clone();
                                next.remove(index);
                                on_change.emit(next);
                            })
                        };

                        let (label, country) = if code.is_default() {
                            ("Default (original language)".to_owned(), None)
                        } else {
                            match code.to_iso639_3().and_then(iso639::by_id) {
                                Some(entry) => (
                                    entry.ref_name.to_owned(),
                                    entry.part1.and_then(iso639::country_by_part1),
                                ),
                                None => (code.to_string(), None),
                            }
                        };

                        html! {
                            <div key={code.to_string()} class="table-entry row">
                                <span class="fill">{label}</span>

                                if let Some(country) = country {
                                    <span class={classes!("item-inline", "flag", country)} />
                                }

                                <button class="btn-danger" onclick={on_remove} title="Remove language">
                                    <span class="icon trash" />
                                </button>
                            </div>
                        }
                    })
                }

                <div class="table-entry row">
                    <button class="btn" onclick={link.callback(|_| SyncLanguagesMsg::Open)} title="Add language">
                        <span class="icon plus" />
                        <span>{"Add language"}</span>
                    </button>
                </div>

                if self.open {
                    <LanguageModal
                        current={api::Language::DEFAULT}
                        placeholder="Add language"
                        title="Add Language"
                        show_top={false}
                        allow_default={false}
                        on_pick={link.callback(SyncLanguagesMsg::Add)}
                        on_close={link.callback(|_| SyncLanguagesMsg::Close)}
                    />
                }
            </div>
        }
    }
}
