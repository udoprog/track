use web_sys::MouseEvent;
use yew::prelude::*;

use super::{LanguageModal, locale_label};

#[derive(Properties, PartialEq)]
pub(crate) struct Props {
    pub(crate) languages: Vec<api::Locale>,
    pub(crate) on_change: Callback<Vec<api::Locale>>,
}

pub(crate) enum Msg {
    Open,
    Close,
    Add(api::Locale),
}

/// Editor for the list of locales the sync path populates. Renders each
/// selected locale with a remove button, plus an "Add language" button that
/// opens a [`LanguageModal`] to pick a fresh locale. `Default` stands for each
/// media's own original language and can't be added here.
pub(crate) struct SyncLanguagesEditor {
    open: bool,
}

impl Component for SyncLanguagesEditor {
    type Message = Msg;
    type Properties = Props;

    fn create(_ctx: &Context<Self>) -> Self {
        Self { open: false }
    }

    fn update(&mut self, ctx: &Context<Self>, msg: Self::Message) -> bool {
        match msg {
            Msg::Open => {
                self.open = true;
            }
            Msg::Close => {
                self.open = false;
            }
            Msg::Add(code) => {
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
                    for props.languages.iter().copied().enumerate().map(|(index, l)| {
                        let on_remove = {
                            let current = props.languages.clone();
                            let on_change = props.on_change.clone();
                            Callback::from(move |_: MouseEvent| {
                                let mut next = current.clone();
                                next.remove(index);
                                on_change.emit(next);
                            })
                        };

                        let (label, flag) = locale_label(l, "Default Language");

                        html! {
                            <div key={l.to_string()} class="table-entry row">
                                <span class="fill">{label}</span>

                                if let Some(flag) = flag {
                                    <span class={classes!("item-inline", "flag", flag)} />
                                }

                                <button class="btn-danger" onclick={on_remove} title="Remove language">
                                    <span class="icon trash" />
                                    <span class="hide-desktop">{"Remove"}</span>
                                </button>
                            </div>
                        }
                    })
                }

                <div class="table-entry row">
                    <button class="btn" onclick={link.callback(|_| Msg::Open)} title="Add language">
                        <span class="icon plus" />
                        <span class="hide-desktop">{"Add language"}</span>
                    </button>
                </div>

                if self.open {
                    <LanguageModal
                        current={api::Locale::DEFAULT}
                        placeholder="Add language"
                        title="Add Language"
                        show_top={false}
                        allow_default={false}
                        on_pick={link.callback(Msg::Add)}
                        on_close={link.callback(|_| Msg::Close)}
                    />
                }
            </div>
        }
    }
}
