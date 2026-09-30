use gloo::timers::callback::Timeout;
use web_sys::{Event, MouseEvent};
use yew::prelude::*;

use crate::ui::Button;

/// How long a revealed secret stays visible before auto-hiding.
const SECRET_REVEAL_MS: u32 = 3000;

/// Reusable input for sensitive values (API keys, PINs). Renders as a password
/// field with three actions: reveal (shows the value, then auto-hides after a
/// few seconds), copy to clipboard, and clear. Controlled `value` comes from
/// the parent and edits are emitted through `on_change`.
#[derive(Properties, PartialEq)]
pub(crate) struct Props {
    #[prop_or_default]
    pub(crate) id: Option<AttrValue>,
    pub(crate) value: String,
    #[prop_or_default]
    pub(crate) placeholder: AttrValue,
    pub(crate) on_change: Callback<String>,
}

pub(crate) enum Msg {
    Input(String),
    Reveal,
    Hide,
    Copy,
    Clear,
}

pub(crate) struct SecretInput {
    revealed: bool,
    // Held so the scheduled auto-hide fires; dropping it cancels the timer.
    _hide_timer: Option<Timeout>,
}

impl Component for SecretInput {
    type Message = Msg;
    type Properties = Props;

    fn create(_ctx: &Context<Self>) -> Self {
        Self {
            revealed: false,
            _hide_timer: None,
        }
    }

    fn update(&mut self, ctx: &Context<Self>, msg: Self::Message) -> bool {
        match msg {
            Msg::Input(value) => {
                ctx.props().on_change.emit(value);
                false
            }
            Msg::Reveal => {
                self.revealed = true;
                let link = ctx.link().clone();
                self._hide_timer = Some(Timeout::new(SECRET_REVEAL_MS, move || {
                    link.send_message(Msg::Hide);
                }));
                true
            }
            Msg::Hide => {
                self.revealed = false;
                self._hide_timer = None;
                true
            }
            Msg::Copy => {
                if let Some(window) = web_sys::window() {
                    let _ = window
                        .navigator()
                        .clipboard()
                        .write_text(&ctx.props().value);
                }
                false
            }
            Msg::Clear => {
                ctx.props().on_change.emit(String::new());
                self.revealed = false;
                self._hide_timer = None;
                true
            }
        }
    }

    fn view(&self, ctx: &Context<Self>) -> Html {
        let link = ctx.link();
        let props = ctx.props();
        let is_empty = props.value.is_empty();

        // Commit on `change` (blur/Enter) rather than `input` so the value is
        // emitted once the user finishes editing, not on every keystroke.
        let on_change = link.callback(|e: Event| {
            let input: web_sys::HtmlInputElement = e.target_unchecked_into();
            Msg::Input(input.value())
        });

        let revealed = self.revealed;
        let on_toggle = link.callback(
            move |_: MouseEvent| {
                if revealed { Msg::Hide } else { Msg::Reveal }
            },
        );

        let (toggle_icon, toggle_title) = if revealed {
            ("eye-slash", "Hide")
        } else {
            ("eye", "Reveal")
        };

        let ty = if revealed { "text" } else { "password" };

        html! {
            <div class="input-group">
                <input id={props.id.clone()} class="input-text fill" type={ty} placeholder={props.placeholder.clone()} value={props.value.clone()} onchange={on_change} autocomplete="off" spellcheck="false" />

                <Button icon={toggle_icon} title={toggle_title} disabled={is_empty} onclick={on_toggle} />
                <Button icon="clipboard" title="Copy to clipboard" disabled={is_empty} onclick={link.callback(|_| Msg::Copy)} />
                <Button icon="x-mark" title="Clear" disabled={is_empty} onclick={link.callback(|_| Msg::Clear)} />
            </div>
        }
    }
}
