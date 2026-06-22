use web_sys::InputEvent;
use yew::prelude::*;

use super::{Modal, PaginationButtons};

/// App-wide context: the most-used custom language codes (ISO 639-1), ordered
/// most-used first, recomputed periodically by the backend. Surfaced as quick
/// picks in every [`LanguagePicker`]. Provided by `App`.
#[derive(Clone, Default, PartialEq)]
pub(crate) struct TopLanguages(pub(crate) Vec<api::Language>);

const LANGUAGE_PAGE_SIZE: usize = 5;

#[derive(Properties, PartialEq)]
pub(crate) struct Props {
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

pub(crate) enum Msg {
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
    type Message = Msg;
    type Properties = Props;

    fn create(ctx: &Context<Self>) -> Self {
        let (top_languages, _top_languages_handle) = ctx
            .link()
            .context::<TopLanguages>(ctx.link().callback(Msg::SetTopLanguages))
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
            Msg::Filter(s) => {
                self.filter = s;
                self.page = 0;
            }
            Msg::Page(p) => {
                self.page = p;
            }
            Msg::Pick(value) => {
                ctx.props().on_pick.emit(value);
            }
            Msg::Close => {
                ctx.props().on_close.emit(());
            }
            Msg::SetTopLanguages(top_languages) => {
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
            Msg::Filter(input.value())
        });

        html! {
            <Modal title={props.title} on_close={link.callback(|_| Msg::Close)}>
                <div class="row">
                    <input autofocus={true} type="text" class="input-text fill" placeholder="Filter" value={self.filter.clone()} oninput={on_filter} />
                </div>

                <div class="table">
                    // Quick picks: the most-used custom languages, shown right
                    // below "Default". Hidden while filtering to avoid duplicates,
                    // and entirely when `show_top` is disabled.
                    if self.filter.is_empty() {
                        if props.allow_default {
                            <div class="table-entry row clickable" onclick={link.callback(|_| Msg::Pick(api::Language::DEFAULT))}>
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
                                    <div key={format!("top-{code}")} class={classes!("table-entry", "row", "clickable", selected.then_some("active"))} onclick={link.callback(move |_| Msg::Pick(code))}>
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
                                    <div key={entry.id} class={classes!("table-entry", "row", "clickable", selected.then_some("active"))} onclick={link.callback(move |_| Msg::Pick(code))}>
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
                            on_page={link.callback(Msg::Page)}
                        />
                    </div>
                </div>
            </Modal>
        }
    }
}
