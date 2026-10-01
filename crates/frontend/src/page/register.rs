use yew::platform::spawn_local;
use yew::prelude::*;

use super::login::input_value;
use crate::http::{self, HttpError, RegisterInfo};
use crate::ui::{Button, FormRow};

#[derive(Properties, PartialEq)]
pub(crate) struct Props {
    pub(crate) token: AttrValue,
    pub(crate) on_register: Callback<api::User>,
}

#[derive(Clone, PartialEq)]
enum Link {
    Loading,
    Valid(RegisterInfo),
    Invalid(AttrValue),
}

/// Redeeming a login link: the user picks a password and is signed in.
#[function_component]
pub(crate) fn Register(props: &Props) -> Html {
    let link = use_state(|| Link::Loading);
    let password = use_state(String::new);
    let confirm = use_state(String::new);
    let busy = use_state(|| false);
    let error = use_state(|| None::<AttrValue>);

    {
        let link = link.clone();

        use_effect_with(props.token.clone(), move |token| {
            let token = token.clone();

            spawn_local(async move {
                link.set(match http::register_info(&token).await {
                    Ok(info) => Link::Valid(info),
                    Err(e) => Link::Invalid(link_error(&e)),
                });
            });
        });
    }

    let on_password_input = {
        let password = password.clone();
        Callback::from(move |e: InputEvent| password.set(input_value(&e)))
    };

    let on_confirm_input = {
        let confirm = confirm.clone();
        Callback::from(move |e: InputEvent| confirm.set(input_value(&e)))
    };

    let onsubmit = {
        let token = props.token.clone();
        let link = link.clone();
        let password = password.clone();
        let confirm = confirm.clone();
        let busy = busy.clone();
        let error = error.clone();
        let on_register = props.on_register.clone();

        Callback::from(move |e: SubmitEvent| {
            e.prevent_default();

            if *busy {
                return;
            }

            if *password != *confirm {
                error.set(Some(AttrValue::from("The passwords do not match.")));
                return;
            }

            busy.set(true);
            error.set(None);

            let token = token.clone();
            let password = (*password).clone();
            let link = link.clone();
            let busy = busy.clone();
            let error = error.clone();
            let on_register = on_register.clone();

            spawn_local(async move {
                match http::register(&token, &password).await {
                    Ok(user) => on_register.emit(user),
                    Err(HttpError::BadRequest(message)) => {
                        error.set(Some(AttrValue::from(message)));
                        busy.set(false);
                    }
                    Err(e) => {
                        link.set(Link::Invalid(link_error(&e)));
                        busy.set(false);
                    }
                }
            });
        })
    };

    let body = match &*link {
        Link::Loading => html! {
            <p class="hint" role="status">{"Checking the link…"}</p>
        },
        Link::Invalid(message) => html! {
            <p class="field-error" role="alert">{message.clone()}</p>
        },
        Link::Valid(info) => html! {
            <>
                <div class="auth-message" role="alert">
                    if let Some(error) = &*error {
                        <p class="field-error">{error.clone()}</p>
                    }
                </div>

                <div class="form-rows">
                    <FormRow label="Login">
                        <span class="auth-value" data-test="register-login">{info.login.clone()}</span>
                    </FormRow>

                    if let Some(email) = &info.email {
                        <FormRow label="Email">
                            <span class="auth-value" data-test="register-email">{email.clone()}</span>
                        </FormRow>
                    }

                    <FormRow label="Password" hint="At least 8 characters.">
                        <input class="input-text fill" type="password" title="Password" autocomplete="new-password" required=true value={(*password).clone()} oninput={on_password_input} />
                    </FormRow>

                    <FormRow label="Confirm password">
                        <input class="input-text fill" type="password" title="Confirm password" autocomplete="new-password" required=true value={(*confirm).clone()} oninput={on_confirm_input} />
                    </FormRow>
                </div>

                <div class="auth-actions">
                    <Button variant="primary" icon="key" label="Set password" title="Set password" disabled={*busy} />
                </div>
            </>
        },
    };

    html! {
        <main class="auth-page">
            <form class="auth-card" {onsubmit} aria-labelledby="auth-heading">
                <h1 id="auth-heading">{"Choose a password"}</h1>
                { body }
            </form>
        </main>
    }
}

fn link_error(error: &HttpError) -> AttrValue {
    match error {
        HttpError::NotFound => AttrValue::from("This login link is not valid."),
        HttpError::Gone => AttrValue::from(
            "This login link has already been used or has expired. Ask an administrator for a new one.",
        ),
        HttpError::Request(error) => format!("Could not reach the server: {error}").into(),
        HttpError::BadRequest(message) => AttrValue::from(message.clone()),
        HttpError::Status(status) => {
            format!("The login link could not be checked (HTTP {status}).").into()
        }
        HttpError::Unauthorized => AttrValue::from("The login link could not be checked."),
    }
}
