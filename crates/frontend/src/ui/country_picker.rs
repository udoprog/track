use web_sys::InputEvent;
use yew::prelude::*;

use super::{Modal, PaginationButtons};

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
}

impl Component for CountryPicker {
    type Message = Msg;
    type Properties = Props;

    fn create(_ctx: &Context<Self>) -> Self {
        Self {
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
            <button class="btn" onclick={link.callback(|_| Msg::Open)} title="Select countries">
                <span class="icon globe-alt" />

                if current.is_empty() {
                    <span>{"All countries"}</span>
                } else {
                    <span>{format!("{} selected", current.len())}</span>

                    {for current.iter().filter_map(|code| {
                        code.to_iso().filter(|c| c.has_flag).map(|c| html! {
                            <span class={classes!("item-inline", "flag", c.alpha2)} />
                        })
                    })}
                }
            </button>
        };

        if !self.open {
            return trigger;
        }

        let needle = self.filter.to_lowercase();

        let filtered: Vec<(&'static iso3166::Country, api::Country)> = iso3166::iter()
            .filter(|country| {
                needle.is_empty()
                    || country.name.to_lowercase().contains(&needle)
                    || country.alpha2.contains(&needle)
            })
            .flat_map(|country| Some((country, api::Country::from_iso(country.alpha2)?)))
            .collect();

        let total_pages = filtered.len().div_ceil(COUNTRY_PAGE_SIZE).max(1);
        let page = self.page.min(total_pages.saturating_sub(1));

        let on_filter = link.callback(|e: InputEvent| {
            let input: web_sys::HtmlInputElement = e.target_unchecked_into();
            Msg::Filter(input.value())
        });

        html! {
            <>
                {trigger}

                <Modal icon="globe" title="Select Countries" on_close={link.callback(|_| Msg::Close)}>
                    <div class="row">
                        <input autofocus={true} type="text" class="input-text fill" placeholder="Filter" value={self.filter.clone()} oninput={on_filter} />
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

                        {
                            for filtered.iter()
                                .skip(page.saturating_mul(COUNTRY_PAGE_SIZE))
                                .take(COUNTRY_PAGE_SIZE)
                                .map(|(country, code)| {
                                    let selected = current.contains(code);
                                    let code = *code;

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
                                })
                        }
                    </div>

                    <PaginationButtons
                        page={page}
                        total_pages={total_pages}
                        on_page={link.callback(Msg::Page)}
                    />
                </Modal>
            </>
        }
    }
}
