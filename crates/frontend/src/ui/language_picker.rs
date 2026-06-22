use yew::prelude::*;

use super::{LanguageModal, locale_label};

#[derive(Properties, PartialEq)]
pub(crate) struct Props {
    pub(crate) current: api::Locale,
    pub(crate) on_change: Callback<api::Locale>,
    pub(crate) placeholder: &'static str,
    /// Whether to show the "top locales" quick-pick section. Disable for
    /// list-style usages (e.g. configuring which locales to sync).
    #[prop_or(true)]
    pub(crate) show_top: bool,
}

pub(crate) enum Msg {
    Open,
    Close,
    Pick(api::Locale),
}

/// A button showing the current language that opens a [`LanguageModal`] to
/// change it. Used where a value is displayed and edited in place.
pub(crate) struct LanguagePicker {
    open: bool,
}

impl Component for LanguagePicker {
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

        let current = props.current;

        let trigger = if current.is_default() {
            html! {
                <button class="btn" onclick={link.callback(|_| Msg::Open)} title="Select language">
                    <span class="icon language" />
                    <span>{props.placeholder}</span>
                </button>
            }
        } else {
            let (label, flag) = locale_label(current, "Default Language");

            html! {
                <button class="btn" onclick={link.callback(|_| Msg::Open)} title="Select language">
                    <span class="icon language" />
                    <span>{label}</span>

                    if let Some(code) = flag {
                        <span class={classes!("flag", code)}></span>
                    }
                </button>
            }
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
                    on_pick={link.callback(Msg::Pick)}
                    on_close={link.callback(|_| Msg::Close)}
                />
            </>
        }
    }
}
