use wasm_bindgen::JsValue;
use yew::prelude::*;

use crate::app::App;
use crate::http::{self, HttpError};
use crate::page::{Login, Register};

/// Signs the user in before the app, and its websocket, start.
pub(super) struct Root {
    state: State,
}

enum State {
    /// Asking the server who is signed in.
    Checking,
    SignedOut {
        notice: Option<AttrValue>,
    },
    /// On a `/register/{token}` login link.
    Register(AttrValue),
    SignedIn(api::User),
}

pub(super) enum Msg {
    Checked(Result<api::User, HttpError>),
    SignedIn(api::User),
    UserChanged(api::User),
    SignOut,
    SignedOut,
    /// The server no longer knows the session, such as after signing out
    /// elsewhere.
    SessionEnded,
}

impl Component for Root {
    type Message = Msg;
    type Properties = ();

    fn create(ctx: &Context<Self>) -> Self {
        if let Some(token) = register_token() {
            return Self {
                state: State::Register(token.into()),
            };
        }

        ctx.link()
            .send_future(async { Msg::Checked(http::me().await) });

        Self {
            state: State::Checking,
        }
    }

    fn update(&mut self, ctx: &Context<Self>, msg: Self::Message) -> bool {
        match msg {
            Msg::Checked(Ok(user)) | Msg::SignedIn(user) => {
                if matches!(self.state, State::Register(..)) {
                    replace_url("/");
                }

                self.state = State::SignedIn(user);
            }
            Msg::Checked(Err(error)) => {
                let notice = match error {
                    HttpError::Unauthorized => None,
                    _ => Some(AttrValue::from("Could not reach the server.")),
                };

                self.state = State::SignedOut { notice };
            }
            Msg::UserChanged(user) => {
                self.state = State::SignedIn(user);
            }
            Msg::SignOut => {
                ctx.link().send_future(async {
                    // The session is forgotten here either way; a failed
                    // request leaves it to expire on the server.
                    if let Err(error) = http::logout().await {
                        tracing::warn!(?error, "Signing out");
                    }

                    Msg::SignedOut
                });

                return false;
            }
            Msg::SignedOut => {
                replace_url("/");
                self.state = State::SignedOut { notice: None };
            }
            Msg::SessionEnded => {
                self.state = State::SignedOut {
                    notice: Some(AttrValue::from("Your session has ended. Sign in again.")),
                };
            }
        }

        true
    }

    fn view(&self, ctx: &Context<Self>) -> Html {
        let link = ctx.link();

        match &self.state {
            State::Checking => html! {
                <main class="auth-page" aria-busy="true" />
            },
            State::SignedOut { notice } => html! {
                <Login on_login={link.callback(Msg::SignedIn)} notice={notice.clone()} />
            },
            State::Register(token) => html! {
                <Register token={token.clone()} on_register={link.callback(Msg::SignedIn)} />
            },
            State::SignedIn(user) => html! {
                <App
                    user={user.clone()}
                    on_user_changed={link.callback(Msg::UserChanged)}
                    on_sign_out={link.callback(|()| Msg::SignOut)}
                    on_session_ended={link.callback(|()| Msg::SessionEnded)}
                />
            },
        }
    }
}

/// The token of a `/register/{token}` URL.
fn register_token() -> Option<String> {
    let path = web_sys::window()?.location().pathname().ok()?;
    let token = path.strip_prefix("/register/")?.trim_end_matches('/');
    (!token.is_empty()).then(|| token.to_owned())
}

fn replace_url(url: &str) {
    if let Some(history) = web_sys::window().and_then(|w| w.history().ok())
        && let Err(error) = history.replace_state_with_url(&JsValue::NULL, "", Some(url))
    {
        tracing::warn!(?error, "Replacing the URL");
    }
}
