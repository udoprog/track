use std::borrow::Cow;

use web_sys::InputEvent;
use yew::prelude::*;

use super::{Modal, PaginationButtons};

/// App-wide context: the most-used custom locales, ordered most-used first,
/// recomputed periodically by the backend. Surfaced as quick picks in every
/// [`LanguagePicker`]. Provided by `App`.
#[derive(Clone, Default, PartialEq)]
pub(crate) struct TopLanguages(pub(crate) Vec<api::Locale>);

const LANGUAGE_PAGE_SIZE: usize = 5;

/// The display name, flag CSS class and fallback code for a locale. The flag
/// prefers the country (for a concrete locale) and falls back to the language's
/// own flag; the fallback code (3-letter language id) is shown when no flag is
/// available.
pub(crate) fn locale_label(
    locale: api::Locale,
    default: &'static str,
) -> (Cow<'static, str>, Option<&'static str>) {
    let lang = locale.language().to_iso();
    let country = locale.country().to_iso();

    let name = match (lang, country) {
        (Some(l), Some(c)) => Cow::Owned(format!("{} ({})", l.name, c.name)),
        (Some(l), None) => Cow::Borrowed(l.name),
        (None, Some(c)) => Cow::Borrowed(c.name),
        (None, None) => Cow::Borrowed(default),
    };

    (name, locale.flag())
}

/// Every selectable locale: a language-only "any country" row for each language
/// with an ISO 639-1 code, followed by each valid language+country combination.
/// Filtered against `filter` when set: the filter is tokenized and every query
/// token must prefix-match one of the entry's tokens (see [`locale_tokens`]).
fn populate_filtered(filter: &str, out: &mut Vec<api::Locale>) {
    out.clear();

    // Language-only rows: one per language that has a 2-letter form.
    for entry in iso639::iter().filter(|e| e.part1.is_some()) {
        if let Some(language) = api::Language::from_iso(entry.id) {
            out.push(api::Locale::new(language, api::Country::DEFAULT));
        }
    }

    // Concrete language+country combinations.
    for locale in locales::iter() {
        if let (Some(language), Some(country)) = (
            api::Language::from_iso(locale.language),
            api::Country::from_iso(locale.country),
        ) {
            out.push(api::Locale::new(language, country));
        }
    }

    let query = tokenize(filter);

    if !query.is_empty() {
        // Each query token must prefix-match some entry token, so order-independent
        // multi-word queries like "english united" or "eng us" match.
        out.retain(|l| {
            let tokens = locale_tokens(*l);
            query
                .iter()
                .all(|q| tokens.iter().any(|t| t.starts_with(q.as_str())))
        });
    }

    out.sort();
}

/// Split a string into lowercase alphanumeric tokens, dropping punctuation and
/// whitespace (e.g. `"English (United States)"` -> `["english", "united", "states"]`).
fn tokenize(s: &str) -> Vec<String> {
    s.split(|c: char| !c.is_alphanumeric())
        .filter(|t| !t.is_empty())
        .map(str::to_lowercase)
        .collect()
}

/// The searchable tokens for a locale: the words of its language and country
/// names, plus the language id (`eng`), its ISO 639-1 code (`en`), and the
/// country alpha-2 code (`US`).
fn locale_tokens(locale: api::Locale) -> Vec<String> {
    let mut tokens = Vec::new();

    if let Some(lang) = locale.language().to_iso() {
        tokens.extend(tokenize(lang.name));
        tokens.push(lang.id.to_lowercase());

        if let Some(part1) = lang.part1 {
            tokens.push(part1.to_lowercase());
        }
    }

    if let Some(country) = locale.country().to_iso() {
        tokens.extend(tokenize(country.name));
        tokens.push(country.alpha2.to_lowercase());
    }

    tokens
}

#[derive(Properties, PartialEq)]
pub(crate) struct Props {
    /// The currently-selected locale, highlighted in the list. Pass
    /// [`api::Locale::DEFAULT`] together with `allow_default = false` for a
    /// "fresh pick" with nothing pre-selected.
    #[prop_or_default]
    pub(crate) current: Option<api::Locale>,
    pub(crate) on_pick: Callback<api::Locale>,
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
    Pick(api::Locale),
    Close,
    SetTopLanguages(TopLanguages),
}

/// The locale-selection modal: a filterable, paginated list of locales with
/// optional "Default" and top-locales quick-pick rows. Pure selection UI with
/// no trigger of its own, so callers control when it opens.
pub(crate) struct LanguageModal {
    filter: String,
    page: usize,
    top_languages: Vec<api::Locale>,
    filtered: Vec<api::Locale>,
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

        let mut filtered = Vec::new();

        populate_filtered("", &mut filtered);

        Self {
            filter: String::new(),
            page: 0,
            top_languages: top_languages.0,
            filtered,
            _top_languages_handle,
        }
    }

    fn update(&mut self, ctx: &Context<Self>, msg: Self::Message) -> bool {
        match msg {
            Msg::Filter(s) => {
                self.filter = s;
                self.page = 0;
                populate_filtered(&self.filter, &mut self.filtered);
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

        let total_pages = self.filtered.len().div_ceil(LANGUAGE_PAGE_SIZE).max(1);
        let page = self.page.min(total_pages.saturating_sub(1));

        let on_filter = link.callback(|e: InputEvent| {
            let input: web_sys::HtmlInputElement = e.target_unchecked_into();
            Msg::Filter(input.value())
        });

        html! {
            <Modal icon="language" title={props.title} on_close={link.callback(|_| Msg::Close)}>
                <div class="row">
                    <input autofocus={true} type="text" class="input-text fill" placeholder="Filter" value={self.filter.clone()} oninput={on_filter} />
                </div>

                <div class="table">
                    // Quick picks: the most-used custom languages, shown right
                    // below "Default". Hidden while filtering to avoid duplicates,
                    // and entirely when `show_top` is disabled.
                    if self.filter.is_empty() {
                        if props.allow_default {
                            <div class="row clickable" onclick={link.callback(|_| Msg::Pick(api::Locale::DEFAULT))}>
                                <span class="fill">{props.placeholder}</span>

                                if let Some(current) = current &&  current.is_default() {
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
                            { for self.top_languages.iter().copied().map(|locale| {
                                let selected = current == Some(locale);
                                let (name, flag) = locale_label(locale, "Default Language");

                                html! {
                                    <div key={format!("top-{locale}")} class={classes!("row", "clickable", selected.then_some("active"))} onclick={link.callback(move |_| Msg::Pick(locale))}>
                                        <span class="fill">{name}</span>

                                        if selected {
                                            <span class="item-inline">
                                                <span class="icon check" />
                                            </span>
                                        }

                                        if let Some(flag) = flag {
                                            <span class={classes!("item-inline", "flag", flag)} title={locale} />
                                        } else {
                                            <span class="item-inline">
                                                <span class="text-muted">{locale}</span>
                                            </span>
                                        }
                                    </div>
                                }
                            }) }
                        }
                    }

                    if !self.filtered.is_empty() {
                        <table-separator />
                    }

                    {
                        for self.filtered.iter()
                            .copied()
                            .skip(page.saturating_mul(LANGUAGE_PAGE_SIZE))
                            .take(LANGUAGE_PAGE_SIZE)
                            .map(|locale| {
                                let selected = current == Some(locale);
                                let (name, flag) = locale_label(locale, "Default Language");

                                html! {
                                    <div key={locale.to_string()} class={classes!("row", "clickable", selected.then_some("active"))} onclick={link.callback(move |_| Msg::Pick(locale))}>
                                        <span class="fill">{name}</span>

                                        if selected {
                                            <span class="item-inline">
                                                <span class="icon check" />
                                            </span>
                                        }

                                        <span class="item-inline">
                                            {locale}
                                        </span>

                                        if let Some(flag) = flag {
                                            <span class={classes!("item-inline", "flag", flag)} title={locale} />
                                        } else {
                                            <span class="item-inline">
                                                <span class="text-muted">{locale}</span>
                                            </span>
                                        }
                                    </div>
                                }
                            })
                    }
                </div>

                <div class="row desktop-align-end">
                    <PaginationButtons page={page} total_pages={total_pages} on_page={link.callback(Msg::Page)} />
                </div>
            </Modal>
        }
    }
}
