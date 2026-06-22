use yew::prelude::*;

use super::LanguageModal;

#[derive(Properties, PartialEq)]
pub(crate) struct Props {
    pub(crate) current: api::Language,
    pub(crate) on_change: Callback<api::Language>,
    pub(crate) placeholder: &'static str,
    /// Whether to show the "top languages" quick-pick section. Disable for
    /// list-style usages (e.g. configuring which languages to sync).
    #[prop_or(true)]
    pub(crate) show_top: bool,
}

pub(crate) enum Msg {
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
        let value = current.to_iso().map(|e| (e.name, e.flag));

        let trigger = match value {
            Some((label, flag)) => html! {
                <button class="btn" onclick={link.callback(|_| Msg::Open)} title="Select language">
                    <span class="icon language" />
                    <span>{label}</span>

                    if let Some(code) = flag {
                        <span class={classes!("flag", code)}></span>
                    }
                </button>
            },
            None => html! {
                <button class="btn" onclick={link.callback(|_| Msg::Open)} title="Select language">
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
                    on_pick={link.callback(Msg::Pick)}
                    on_close={link.callback(|_| Msg::Close)}
                />
            </>
        }
    }
}
