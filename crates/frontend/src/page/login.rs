use yew::platform::spawn_local;
use yew::prelude::*;

use crate::http::{self, HttpError};
use crate::ui::{Button, FormRow};

#[derive(Properties, PartialEq)]
pub(crate) struct Props {
    pub(crate) on_login: Callback<api::User>,
    /// Said above the form, such as why the user has to sign in again.
    #[prop_or_default]
    pub(crate) notice: Option<AttrValue>,
}

/// Signing in with a password or Cloudflare Access.
#[function_component]
pub(crate) fn Login(props: &Props) -> Html {
    let login = use_state(String::new);
    let password = use_state(String::new);
    let busy = use_state(|| false);
    let error = use_state(|| None::<AttrValue>);

    let cloudflare = use_state(|| false);

    {
        let cloudflare = cloudflare.clone();
        use_effect_with((), move |_| {
            spawn_local(async move {
                cloudflare.set(http::cloudflare_available().await.unwrap_or(false));
            });
        });
    }

    let on_cloudflare = {
        let busy = busy.clone();
        let error = error.clone();
        let on_login = props.on_login.clone();
        Callback::from(move |e: MouseEvent| {
            e.prevent_default();
            if *busy {
                return;
            }
            busy.set(true);
            error.set(None);
            let busy = busy.clone();
            let error = error.clone();
            let on_login = on_login.clone();
            spawn_local(async move {
                match http::cloudflare_login().await {
                    Ok(user) => on_login.emit(user),
                    Err(e) => {
                        error.set(Some(match e {
                            HttpError::Unauthorized => {
                                "Cloudflare could not sign you in. Reload to try again.".into()
                            }
                            _ => login_error(&e),
                        }));
                        busy.set(false);
                    }
                }
            });
        })
    };

    let on_login_input = {
        let login = login.clone();
        Callback::from(move |e: InputEvent| login.set(input_value(&e)))
    };

    let on_password_input = {
        let password = password.clone();
        Callback::from(move |e: InputEvent| password.set(input_value(&e)))
    };

    let onsubmit = {
        let login = login.clone();
        let password = password.clone();
        let busy = busy.clone();
        let error = error.clone();
        let on_login = props.on_login.clone();

        Callback::from(move |e: SubmitEvent| {
            e.prevent_default();

            if *busy {
                return;
            }

            busy.set(true);
            error.set(None);

            let login = (*login).clone();
            let password = (*password).clone();
            let busy = busy.clone();
            let error = error.clone();
            let on_login = on_login.clone();

            spawn_local(async move {
                match http::login(&login, &password).await {
                    Ok(user) => on_login.emit(user),
                    Err(e) => {
                        error.set(Some(login_error(&e)));
                        busy.set(false);
                    }
                }
            });
        })
    };

    let message = (*error).clone().or_else(|| props.notice.clone());

    html! {
        <main class="auth-page">
            <form class="auth-card" {onsubmit} aria-labelledby="auth-heading">
                <h1 id="auth-heading">{"Sign in"}</h1>

                <div class="auth-message" role="alert">
                    if let Some(message) = message {
                        <p class={classes!(error.is_some().then_some("field-error"))}>{message}</p>
                    }
                </div>

                <div class="form-rows">
                    <FormRow label="Login or email">
                        <input class="input-text fill" type="text" title="Login or email" autocomplete="username" autocapitalize="none" spellcheck="false" required=true value={(*login).clone()} oninput={on_login_input} />
                    </FormRow>

                    <FormRow label="Password">
                        <input class="input-text fill" type="password" title="Password" autocomplete="current-password" required=true value={(*password).clone()} oninput={on_password_input} />
                    </FormRow>
                </div>

                if *cloudflare {
                    <div class="auth-actions">
                        <button type="button" class="has-text" title="Login using Cloudflare" onclick={on_cloudflare} disabled={*busy}>
                            <span>{"Login using Cloudflare"}</span>
                        </button>
                    </div>
                }

                <div class="auth-actions">
                    <Button variant="primary" icon="arrow-right-end-on-rectangle" label="Sign in" title="Sign in" disabled={*busy} />
                </div>
            </form>
        </main>
    }
}

fn login_error(error: &HttpError) -> AttrValue {
    match error {
        HttpError::Unauthorized => AttrValue::from("Wrong login or password."),
        HttpError::Request(error) => format!("Could not reach the server: {error}").into(),
        HttpError::BadRequest(message) => AttrValue::from(message.clone()),
        HttpError::Status(status) => format!("Signing in failed (HTTP {status}).").into(),
        HttpError::NotFound | HttpError::Gone => AttrValue::from("Signing in failed."),
    }
}

pub(crate) fn input_value(e: &InputEvent) -> String {
    let input: web_sys::HtmlInputElement = e.target_unchecked_into();
    input.value()
}
