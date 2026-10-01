use musli_web::web03::prelude::*;
use yew::prelude::*;

use crate::SetupChannel;
use crate::background::Background;
use crate::ui::{Button, FormRow};

#[derive(Properties, PartialEq)]
pub(crate) struct Props {
    pub(crate) user: api::User,
    pub(crate) on_user_changed: Callback<api::User>,
    pub(crate) on_sign_out: Callback<()>,
}

/// How a form's last save went, said under it.
#[derive(Default)]
enum Status {
    #[default]
    Idle,
    Saving,
    Saved(&'static str),
    Failed(String),
}

impl Status {
    fn view(&self) -> Html {
        match self {
            Status::Idle | Status::Saving => Html::default(),
            Status::Saved(message) => html! {
                <span class="field-ok" role="status">{*message}</span>
            },
            Status::Failed(message) => html! {
                <span class="field-error" role="alert">{message.clone()}</span>
            },
        }
    }

    fn saving(&self) -> bool {
        matches!(self, Status::Saving)
    }
}

pub(crate) struct Account {
    channel: ws::Channel,
    background: Background,
    login: String,
    email: String,
    old_password: String,
    new_password: String,
    confirm_password: String,
    login_status: Status,
    email_status: Status,
    password_status: Status,
    _setup: SetupChannel,
    _login_req: ws::Request,
    _email_req: ws::Request,
    _password_req: ws::Request,
}

pub(crate) enum Msg {
    Channel(Result<ws::Channel, ws::Error>),
    LoginInput(String),
    EmailInput(String),
    OldPasswordInput(String),
    NewPasswordInput(String),
    ConfirmPasswordInput(String),
    SaveLogin,
    SaveEmail,
    SavePassword,
    LoginSaved(Result<ws::Packet<api::SetLogin>, ws::Error>),
    EmailSaved(Result<ws::Packet<api::SetEmail>, ws::Error>),
    PasswordSaved(Result<ws::Packet<api::SetPassword>, ws::Error>),
}

impl Component for Account {
    type Message = Msg;
    type Properties = Props;

    fn create(ctx: &Context<Self>) -> Self {
        let (ws, _) = ctx
            .link()
            .context::<ws::Handle>(Callback::noop())
            .expect("Expected ws::Handle in context");

        let (background, _) = ctx
            .link()
            .context::<Background>(Callback::noop())
            .expect("Expected background handle in context");

        let user = &ctx.props().user;

        Self {
            channel: ws::Channel::default(),
            background,
            login: user.login.clone(),
            email: user.email.clone().unwrap_or_default(),
            old_password: String::new(),
            new_password: String::new(),
            confirm_password: String::new(),
            login_status: Status::Idle,
            email_status: Status::Idle,
            password_status: Status::Idle,
            _setup: SetupChannel::new(ws, ctx.link().callback(Msg::Channel)),
            _login_req: ws::Request::default(),
            _email_req: ws::Request::default(),
            _password_req: ws::Request::default(),
        }
    }

    fn rendered(&mut self, _: &Context<Self>, first_render: bool) {
        if first_render {
            self.background.title(Some("Account".to_string()));
        }
    }

    fn destroy(&mut self, _: &Context<Self>) {
        self.background.title(None);
    }

    fn changed(&mut self, ctx: &Context<Self>, old: &Self::Properties) -> bool {
        let user = &ctx.props().user;

        if user.login != old.user.login {
            self.login = user.login.clone();
        }

        if user.email != old.user.email {
            self.email = user.email.clone().unwrap_or_default();
        }

        true
    }

    fn update(&mut self, ctx: &Context<Self>, msg: Self::Message) -> bool {
        let link = ctx.link();

        match msg {
            Msg::Channel(result) => {
                match result {
                    Ok(channel) => self.channel = channel,
                    Err(e) => self.background.error(e.into()),
                }

                false
            }
            Msg::LoginInput(value) => {
                self.login = value;
                true
            }
            Msg::EmailInput(value) => {
                self.email = value;
                true
            }
            Msg::OldPasswordInput(value) => {
                self.old_password = value;
                true
            }
            Msg::NewPasswordInput(value) => {
                self.new_password = value;
                true
            }
            Msg::ConfirmPasswordInput(value) => {
                self.confirm_password = value;
                true
            }
            Msg::SaveLogin => {
                self.login_status = Status::Saving;
                self._login_req = self
                    .channel
                    .request()
                    .body(api::SetLoginRequest {
                        login: self.login.clone(),
                    })
                    .on_packet(link.callback(Msg::LoginSaved))
                    .send();
                true
            }
            Msg::SaveEmail => {
                let email = self.email.trim();
                self.email_status = Status::Saving;
                self._email_req = self
                    .channel
                    .request()
                    .body(api::SetEmailRequest {
                        email: (!email.is_empty()).then(|| email.to_owned()),
                    })
                    .on_packet(link.callback(Msg::EmailSaved))
                    .send();
                true
            }
            Msg::SavePassword => {
                if self.new_password != self.confirm_password {
                    self.password_status =
                        Status::Failed("The new passwords do not match.".to_owned());
                    return true;
                }

                self.password_status = Status::Saving;
                self._password_req = self
                    .channel
                    .request()
                    .body(api::SetPasswordRequest {
                        old_password: self.old_password.clone(),
                        new_password: self.new_password.clone(),
                    })
                    .on_packet(link.callback(Msg::PasswordSaved))
                    .send();
                true
            }
            Msg::LoginSaved(result) => {
                self.login_status = match result.and_then(|p| Ok(p.decode()?.user)) {
                    Ok(user) => {
                        ctx.props().on_user_changed.emit(user);
                        Status::Saved("Login changed.")
                    }
                    Err(e) => Status::Failed(e.to_string()),
                };
                true
            }
            Msg::EmailSaved(result) => {
                self.email_status = match result.and_then(|p| Ok(p.decode()?.user)) {
                    Ok(user) => {
                        ctx.props().on_user_changed.emit(user);
                        Status::Saved("Email changed.")
                    }
                    Err(e) => Status::Failed(e.to_string()),
                };
                true
            }
            Msg::PasswordSaved(result) => {
                self.password_status = match result {
                    Ok(..) => {
                        self.old_password.clear();
                        self.new_password.clear();
                        self.confirm_password.clear();

                        let mut user = ctx.props().user.clone();
                        user.has_password = true;
                        ctx.props().on_user_changed.emit(user);
                        Status::Saved("Password changed.")
                    }
                    Err(e) => Status::Failed(e.to_string()),
                };
                true
            }
        }
    }

    fn view(&self, ctx: &Context<Self>) -> Html {
        let link = ctx.link();
        let user = &ctx.props().user;

        let submit = |msg: fn() -> Msg| {
            link.callback(move |e: SubmitEvent| {
                e.prevent_default();
                msg()
            })
        };

        let input = |msg: fn(String) -> Msg| {
            link.callback(move |e: InputEvent| msg(super::login::input_value(&e)))
        };

        let connected = self.channel.id() != ws::ChannelId::NONE;
        let login_unchanged = self.login.trim() == user.login;
        let email_unchanged = self.email.trim() == user.email.as_deref().unwrap_or_default();

        let role = match user.role {
            api::UserRole::Admin => "Administrator",
            api::UserRole::Regular => "Regular user",
        };

        html! {
            <>
                <h1 class="visually-hidden">{"Account"}</h1>

                <div class="settings">
                    <section>
                        <h2>{"Account"}</h2>

                        <div class="form-rows">
                            <FormRow label="Role">
                                <span class="auth-value" data-test="account-role">{role}</span>
                            </FormRow>
                        </div>

                        <form class="form-rows" onsubmit={submit(|| Msg::SaveLogin)}>
                            <FormRow label="Login" hint="Sign in with this name or your email.">
                                <div class="input-group">
                                    <input class="input-text fill" type="text" title="Login" autocomplete="username" autocapitalize="none" spellcheck="false" required=true value={self.login.clone()} oninput={input(Msg::LoginInput)} />
                                    <Button icon="check" label="Save" title="Save login" disabled={!connected || login_unchanged || self.login_status.saving()} />
                                </div>

                                { self.login_status.view() }
                            </FormRow>
                        </form>

                        <form class="form-rows" onsubmit={submit(|| Msg::SaveEmail)}>
                            <FormRow label="Email" hint="Optional. Leave empty to remove it.">
                                <div class="input-group">
                                    <input class="input-text fill" type="email" title="Email" autocomplete="email" spellcheck="false" value={self.email.clone()} oninput={input(Msg::EmailInput)} />
                                    <Button icon="check" label="Save" title="Save email" disabled={!connected || email_unchanged || self.email_status.saving()} />
                                </div>

                                { self.email_status.view() }
                            </FormRow>
                        </form>
                    </section>

                    <section>
                        <h2>{"Password"}</h2>

                        <form class="form-rows" onsubmit={submit(|| Msg::SavePassword)}>
                            if user.has_password {
                                <FormRow label="Current password">
                                    <input class="input-text fill" type="password" title="Current password" autocomplete="current-password" required=true value={self.old_password.clone()} oninput={input(Msg::OldPasswordInput)} />
                                </FormRow>
                            }

                            <FormRow label="New password" hint="At least 8 characters.">
                                <input class="input-text fill" type="password" title="New password" autocomplete="new-password" required=true value={self.new_password.clone()} oninput={input(Msg::NewPasswordInput)} />
                            </FormRow>

                            <FormRow label="Confirm new password">
                                <input class="input-text fill" type="password" title="Confirm new password" autocomplete="new-password" required=true value={self.confirm_password.clone()} oninput={input(Msg::ConfirmPasswordInput)} />
                            </FormRow>

                            <FormRow label="">
                                <Button icon="key" label="Change password" title="Change password" disabled={!connected || self.password_status.saving()} />
                                { self.password_status.view() }
                            </FormRow>
                        </form>
                    </section>

                    <section>
                        <h2>{"Session"}</h2>

                        <div class="form-rows">
                            <FormRow label="Signed in as">
                                <span class="auth-value" data-test="account-login">{user.login.clone()}</span>
                                <Button icon="arrow-left-start-on-rectangle" label="Sign out" title="Sign out" onclick={ctx.props().on_sign_out.reform(|_| ())} />
                            </FormRow>
                        </div>
                    </section>
                </div>
            </>
        }
    }
}
