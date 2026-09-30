use web_sys::InputEvent;
use yew::prelude::*;

use super::{Button, Modal, PaginationButtons};

const COUNTRY_PAGE_SIZE: usize = 8;

#[derive(Properties, PartialEq)]
pub(crate) struct Props {
    /// Selected country alpha-2 codes. Empty means "all countries".
    pub(crate) current: Vec<api::Country>,
    pub(crate) on_change: Callback<Vec<api::Country>>,
}

pub(crate) enum Msg {
    Open,
    Close,
    Filter(String),
    Page(usize),
    Toggle(api::Country),
    All,
}

/// Multi-select picker for countries, modeled on [`LanguagePicker`]. An empty
/// selection represents "all countries".
pub(crate) struct CountryPicker {
    open: bool,
    filter: String,
    page: usize,
    /// Selected countries pinned at the top, filtered by the search term. Held in
    /// the component and rebuilt on open, filter changes and selection changes
    /// rather than reallocated on every render.
    selected_list: Vec<(&'static iso3166::Country, api::Country)>,
    /// The full browse list (filtered, excluding the selection). Rebuilt
    /// alongside `selected_list`.
    browse: Vec<(&'static iso3166::Country, api::Country)>,
}

impl CountryPicker {
    /// Recompute `selected_list` and `browse` from the current filter and
    /// selection into the reused buffers.
    fn rebuild(&mut self, ctx: &Context<Self>) {
        let current = &ctx.props().current;
        let needle = self.filter.to_lowercase();

        let matches = |country: &iso3166::Country| {
            needle.is_empty()
                || country.name.to_lowercase().contains(&needle)
                || country.alpha2.contains(&needle)
        };

        // Selected countries are pinned at the top (filtered by the search term)
        // so they stay easy to deselect while still browsing the full list below.
        self.selected_list.clear();
        self.selected_list.extend(current.iter().filter_map(|code| {
            let iso = code.to_iso()?;
            matches(iso).then_some((iso, *code))
        }));

        // The full, paginated browse list (filtered), excluding what's selected.
        self.browse.clear();
        self.browse.extend(
            iso3166::iter()
                .filter(|country| matches(country))
                .flat_map(|country| Some((country, api::Country::from_iso(country.alpha2)?)))
                .filter(|(_, code)| !current.contains(code)),
        );
    }
}

impl Component for CountryPicker {
    type Message = Msg;
    type Properties = Props;

    fn create(ctx: &Context<Self>) -> Self {
        let mut this = Self {
            open: false,
            filter: String::new(),
            page: 0,
            selected_list: Vec::new(),
            browse: Vec::new(),
        };

        this.rebuild(ctx);
        this
    }

    fn changed(&mut self, ctx: &Context<Self>, _old: &Props) -> bool {
        // The selection lives in props, so rebuild the derived lists when it
        // changes (e.g. after toggling a country while the picker is open).
        self.rebuild(ctx);
        true
    }

    fn update(&mut self, ctx: &Context<Self>, msg: Self::Message) -> bool {
        match msg {
            Msg::Open => {
                self.open = true;
                self.filter.clear();
                self.page = 0;
                self.rebuild(ctx);
            }
            Msg::Close => {
                self.open = false;
            }
            Msg::Filter(s) => {
                self.filter = s;
                self.page = 0;
                self.rebuild(ctx);
            }
            Msg::Page(p) => {
                self.page = p;
            }
            Msg::Toggle(code) => {
                let mut next = ctx.props().current.clone();

                if let Some(pos) = next.iter().position(|c| *c == code) {
                    next.remove(pos);
                } else {
                    next.push(code);
                }

                ctx.props().on_change.emit(next);
            }
            Msg::All => {
                ctx.props().on_change.emit(Vec::new());
            }
        }

        true
    }

    fn view(&self, ctx: &Context<Self>) -> Html {
        let link = ctx.link();
        let current = &ctx.props().current;

        let trigger = html! {
            <Button icon="globe-alt" label={if current.is_empty() { String::from("All countries") } else { format!("{} selected", current.len()) }} title="Select countries" onclick={link.callback(|_| Msg::Open)}>
                {for current.iter().filter_map(|code| {
                    code.to_iso().filter(|c| c.has_flag).map(|c| html! {
                        <span class={classes!("flag", c.alpha2)} />
                    })
                })}
            </Button>
        };

        if !self.open {
            return trigger;
        }

        let selected_list = &self.selected_list;
        let browse = &self.browse;

        let total_pages = browse.len().div_ceil(COUNTRY_PAGE_SIZE).max(1);
        let page = self.page.min(total_pages.saturating_sub(1));

        let on_filter = link.callback(|e: InputEvent| {
            let input: web_sys::HtmlInputElement = e.target_unchecked_into();
            Msg::Filter(input.value())
        });

        let render_row = |country: &iso3166::Country, code: api::Country| {
            let selected = current.contains(&code);

            html! {
                <div key={code} class={classes!("row", "clickable", selected.then_some("active"))} onclick={link.callback(move |_| Msg::Toggle(code))}>
                    <span class="fill">{country.name}</span>

                    if let Some(c) = code.to_iso().filter(|c| c.has_flag) {
                        <span class={classes!("item-inline", "flag", c.alpha2)} />
                    }

                    <span class="item-inline" title={code}>
                        <span class={classes!("icon", if selected { "check" } else { "x-mark" })} />
                    </span>
                </div>
            }
        };

        html! {
            <>
                {trigger}

                <Modal icon="globe-alt" title="Select Countries" on_close={link.callback(|_| Msg::Close)}>
                    <div class="row">
                        <input class="input-text fill" type="text" autofocus={true} placeholder="Filter" value={self.filter.clone()} oninput={on_filter} />
                    </div>

                    <div class="table">
                        <div class="row clickable" onclick={link.callback(|_| Msg::All)}>
                            <span class="fill">{"All countries"}</span>

                            <span class="item-inline">
                                <span class="icon globe-alt" />
                            </span>

                            <span class="item-inline">
                                <span class={classes!("icon", if current.is_empty() { "check" } else { "x-mark" })} />
                            </span>
                        </div>

                        {for selected_list.iter().map(|(country, code)| render_row(country, *code))}

                        if !selected_list.is_empty() && !browse.is_empty() {
                            <table-separator />
                        }

                        {
                            for browse.iter()
                                .skip(page.saturating_mul(COUNTRY_PAGE_SIZE))
                                .take(COUNTRY_PAGE_SIZE)
                                .map(|(country, code)| render_row(country, *code))
                        }
                    </div>

                    <div class="row desktop-align-end">
                        <PaginationButtons page={page} total_pages={total_pages} on_page={link.callback(Msg::Page)} />
                    </div>
                </Modal>
            </>
        }
    }
}
