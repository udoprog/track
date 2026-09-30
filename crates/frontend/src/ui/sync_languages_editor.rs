use web_sys::MouseEvent;
use yew::prelude::*;

use super::{Button, LanguageModal, Variant, locale_label};

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
                    next.sort();
                    next.dedup();
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
            <>
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
                                <div key={l.to_string()} class="row">
                                    <span class="fill">{label}</span>

                                    if let Some(flag) = flag {
                                        <span class={classes!("item-inline", "flag", flag)} />
                                    } else {
                                        <span class="item-inline"><span class="icon language" /></span>
                                    }

                                    <Button icon="trash" variant={Variant::Danger} title="Remove language" text="Remove" onclick={on_remove} />
                                </div>
                            }
                        })
                    }
                </div>

                <div class="row">
                    <Button icon="plus" label="Add language" title="Add language" onclick={link.callback(|_| Msg::Open)} />
                </div>

                if self.open {
                    <LanguageModal
                        placeholder="Default Language"
                        title="Add Language"
                        on_pick={link.callback(Msg::Add)}
                        on_close={link.callback(|_| Msg::Close)}
                    />
                }
            </>
        }
    }
}
