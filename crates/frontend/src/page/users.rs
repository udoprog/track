use std::collections::HashMap;

use api::TimeInfo;
use gloo::timers::callback::Timeout;
use musli_web::web03::prelude::*;
use wasm_bindgen::JsCast;
use yew::prelude::*;

use crate::SetupChannel;
use crate::background::Background;
use crate::ui::{Button, ConfirmDanger, FormRow, Skeleton, Variant};

#[derive(Properties, PartialEq)]
pub(crate) struct Props {
    /// The signed-in administrator, whose own row the server protects.
    pub(crate) me: api::UserId,
}

/// What became of the last action on a user, said in their row.
enum Status {
    Done(&'static str),
    Failed(String),
}

/// A login link generated in this visit; the server only hands out its token
/// once.
struct NewLink {
    url: String,
    expires_at: api::Timestamp,
}

pub(crate) struct Users {
    channel: ws::Channel,
    background: Background,
    time: TimeInfo,
    loaded: bool,
    users: Vec<api::User>,
    /// When each user's pending login link expires.
    pending: HashMap<api::UserId, api::Timestamp>,
    new_links: HashMap<api::UserId, NewLink>,
    status: Option<(api::UserId, Status)>,
    confirm_delete: Option<api::UserId>,
    /// The control to move focus to once rendered, so it is not lost when
    /// the delete confirmation replaces the button that opened it.
    focus: Option<String>,
    login: String,
    email: String,
    role: api::UserRole,
    create_error: Option<String>,
    creating: bool,
    _setup: SetupChannel,
    _time_handle: ContextHandle<TimeInfo>,
    _list_req: ws::Request,
    _create_req: ws::Request,
    _action_req: ws::Request,
}

pub(crate) enum Msg {
    Channel(Result<ws::Channel, ws::Error>),
    SetTime(TimeInfo),
    Listed(Result<ws::Packet<api::ListUsers>, ws::Error>),
    LoginInput(String),
    EmailInput(String),
    RoleInput(api::UserRole),
    Create,
    Created(Result<ws::Packet<api::CreateUser>, ws::Error>),
    SetRole(api::UserId, api::UserRole),
    RoleSet(api::UserId, Result<ws::Packet<api::SetUserRole>, ws::Error>),
    GenerateLink(api::UserId),
    LinkGenerated(
        api::UserId,
        Result<ws::Packet<api::GenerateLoginToken>, ws::Error>,
    ),
    Copy(api::UserId),
    RevokeLink(api::UserId),
    LinkRevoked(
        api::UserId,
        Result<ws::Packet<api::RevokeLoginToken>, ws::Error>,
    ),
    RevokeAccess(api::UserId),
    AccessRevoked(
        api::UserId,
        Result<ws::Packet<api::RevokeUserAccess>, ws::Error>,
    ),
    ConfirmDelete(api::UserId),
    CancelDelete,
    Delete(api::UserId),
    Deleted(api::UserId, Result<ws::Packet<api::DeleteUser>, ws::Error>),
}

impl Component for Users {
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

        let (time, _time_handle) = ctx
            .link()
            .context::<TimeInfo>(ctx.link().callback(Msg::SetTime))
            .expect("Expected a configured time zone");

        Self {
            channel: ws::Channel::default(),
            background,
            time,
            loaded: false,
            users: Vec::new(),
            pending: HashMap::new(),
            new_links: HashMap::new(),
            status: None,
            confirm_delete: None,
            focus: None,
            login: String::new(),
            email: String::new(),
            role: api::UserRole::Regular,
            create_error: None,
            creating: false,
            _setup: SetupChannel::new(ws, ctx.link().callback(Msg::Channel)),
            _time_handle,
            _list_req: ws::Request::default(),
            _create_req: ws::Request::default(),
            _action_req: ws::Request::default(),
        }
    }

    fn rendered(&mut self, _: &Context<Self>, first_render: bool) {
        if first_render {
            self.background.title(Some("Users".to_string()));
        }

        if let Some(selector) = self.focus.take() {
            // After the key press that got here, which would otherwise
            // activate the newly focused button.
            Timeout::new(0, move || {
                let element = web_sys::window()
                    .and_then(|w| w.document())
                    .and_then(|d| d.query_selector(&selector).ok().flatten())
                    .and_then(|e| e.dyn_into::<web_sys::HtmlElement>().ok());

                if let Some(element) = element {
                    _ = element.focus();
                }
            })
            .forget();
        }
    }

    fn destroy(&mut self, _: &Context<Self>) {
        self.background.title(None);
    }

    fn update(&mut self, ctx: &Context<Self>, msg: Self::Message) -> bool {
        let link = ctx.link();

        match msg {
            Msg::Channel(result) => {
                match result {
                    Ok(channel) => {
                        self.channel = channel;

                        if self.channel.id() == ws::ChannelId::NONE {
                            return true;
                        }

                        self._list_req = self
                            .channel
                            .request()
                            .body(api::ListUsersRequest)
                            .on_packet(link.callback(Msg::Listed))
                            .send();
                    }
                    Err(e) => self.background.error(e.into()),
                }

                true
            }
            Msg::SetTime(time) => {
                self.time = time;
                true
            }
            Msg::Listed(result) => {
                match result.and_then(|p| Ok(p.decode()?)) {
                    Ok(response) => {
                        self.users = response.users;
                        self.pending = response
                            .login_links
                            .into_iter()
                            .map(|link| (link.user_id, link.expires_at))
                            .collect();
                        self.loaded = true;
                    }
                    Err(e) => self.background.error(e.into()),
                }

                true
            }
            Msg::LoginInput(value) => {
                self.login = value;
                true
            }
            Msg::EmailInput(value) => {
                self.email = value;
                true
            }
            Msg::RoleInput(role) => {
                self.role = role;
                true
            }
            Msg::Create => {
                let email = self.email.trim();
                self.creating = true;
                self.create_error = None;
                self._create_req = self
                    .channel
                    .request()
                    .body(api::CreateUserRequest {
                        login: self.login.clone(),
                        email: (!email.is_empty()).then(|| email.to_owned()),
                        role: self.role,
                    })
                    .on_packet(link.callback(Msg::Created))
                    .send();
                true
            }
            Msg::Created(result) => {
                self.creating = false;

                match result.and_then(|p| Ok(p.decode()?.user)) {
                    Ok(user) => {
                        let id = user.id;
                        self.users.push(user);
                        self.login.clear();
                        self.email.clear();
                        self.role = api::UserRole::Regular;
                        // A new user can only sign in through a login link.
                        link.send_message(Msg::GenerateLink(id));
                    }
                    Err(e) => self.create_error = Some(e.to_string()),
                }

                true
            }
            Msg::SetRole(id, role) => {
                self._action_req = self
                    .channel
                    .request()
                    .body(api::SetUserRoleRequest { user_id: id, role })
                    .on_packet(link.callback(move |r| Msg::RoleSet(id, r)))
                    .send();
                false
            }
            Msg::RoleSet(id, result) => {
                match result.and_then(|p| Ok(p.decode()?.user)) {
                    Ok(user) => {
                        if let Some(u) = self.users.iter_mut().find(|u| u.id == id) {
                            *u = user;
                        }

                        self.status = Some((id, Status::Done("Role changed.")));
                    }
                    Err(e) => self.status = Some((id, Status::Failed(e.to_string()))),
                }

                true
            }
            Msg::GenerateLink(id) => {
                self._action_req = self
                    .channel
                    .request()
                    .body(api::GenerateLoginTokenRequest { user_id: id })
                    .on_packet(link.callback(move |r| Msg::LinkGenerated(id, r)))
                    .send();
                false
            }
            Msg::LinkGenerated(id, result) => {
                match result.and_then(|p| Ok(p.decode()?)) {
                    Ok(response) => {
                        let origin = web_sys::window()
                            .and_then(|w| w.location().origin().ok())
                            .unwrap_or_default();

                        self.pending.insert(id, response.expires_at);
                        self.new_links.insert(
                            id,
                            NewLink {
                                url: format!("{origin}/register/{}", response.token),
                                expires_at: response.expires_at,
                            },
                        );
                        self.status = None;
                    }
                    Err(e) => self.status = Some((id, Status::Failed(e.to_string()))),
                }

                true
            }
            Msg::Copy(id) => {
                if let Some(new) = self.new_links.get(&id)
                    && let Some(window) = web_sys::window()
                {
                    _ = window.navigator().clipboard().write_text(&new.url);
                    self.status = Some((id, Status::Done("Link copied.")));
                    return true;
                }

                false
            }
            Msg::RevokeLink(id) => {
                self._action_req = self
                    .channel
                    .request()
                    .body(api::RevokeLoginTokenRequest { user_id: id })
                    .on_packet(link.callback(move |r| Msg::LinkRevoked(id, r)))
                    .send();
                false
            }
            Msg::LinkRevoked(id, result) => {
                match result {
                    Ok(..) => {
                        self.pending.remove(&id);
                        self.new_links.remove(&id);
                        self.status = Some((id, Status::Done("Login link revoked.")));
                    }
                    Err(e) => self.status = Some((id, Status::Failed(e.to_string()))),
                }

                true
            }
            Msg::RevokeAccess(id) => {
                self._action_req = self
                    .channel
                    .request()
                    .body(api::RevokeUserAccessRequest { user_id: id })
                    .on_packet(link.callback(move |r| Msg::AccessRevoked(id, r)))
                    .send();
                false
            }
            Msg::AccessRevoked(id, result) => {
                self.status = Some(match result {
                    Ok(..) => (id, Status::Done("Signed out everywhere.")),
                    Err(e) => (id, Status::Failed(e.to_string())),
                });
                true
            }
            Msg::ConfirmDelete(id) => {
                self.confirm_delete = Some(id);
                self.focus = Some(format!("[data-user-id='{id}'] [title=No]"));
                true
            }
            Msg::CancelDelete => {
                if let Some(id) = self.confirm_delete.take() {
                    self.focus = Some(format!("[data-user-id='{id}'] .user-delete"));
                }

                true
            }
            Msg::Delete(id) => {
                self.confirm_delete = None;
                self._action_req = self
                    .channel
                    .request()
                    .body(api::DeleteUserRequest { user_id: id })
                    .on_packet(link.callback(move |r| Msg::Deleted(id, r)))
                    .send();
                true
            }
            Msg::Deleted(id, result) => {
                match result {
                    Ok(..) => {
                        self.users.retain(|u| u.id != id);
                        self.pending.remove(&id);
                        self.new_links.remove(&id);
                    }
                    Err(e) => self.status = Some((id, Status::Failed(e.to_string()))),
                }

                true
            }
        }
    }

    fn view(&self, ctx: &Context<Self>) -> Html {
        let link = ctx.link();
        let connected = self.channel.id() != ws::ChannelId::NONE;

        let on_create = link.callback(|e: SubmitEvent| {
            e.prevent_default();
            Msg::Create
        });

        let input = |msg: fn(String) -> Msg| {
            link.callback(move |e: InputEvent| msg(super::login::input_value(&e)))
        };

        let on_role = link.callback(|e: Event| {
            let select: web_sys::HtmlSelectElement = e.target_unchecked_into();
            Msg::RoleInput(parse_role(&select.value()))
        });

        html! {
            <>
                <h1 class="visually-hidden">{"Users"}</h1>

                <div class="settings users-page">
                    <section>
                        <h2>{"Users"}</h2>

                        if self.loaded {
                            <div class="user-rows" role="table" aria-label="Users">
                                <div class="user-row user-head" role="row">
                                    <span role="columnheader">{"Login"}</span>
                                    <span role="columnheader">{"Email"}</span>
                                    <span role="columnheader">{"Role"}</span>
                                    <span role="columnheader">{"Login link"}</span>
                                    <span role="columnheader"><span class="visually-hidden">{"Actions"}</span></span>
                                </div>

                                { for self.users.iter().map(|user| self.view_user(ctx, user)) }
                            </div>
                        } else {
                            <Skeleton class="user-skeleton" />
                        }
                    </section>

                    <section>
                        <h2>{"Add user"}</h2>

                        <form class="form-rows" onsubmit={on_create}>
                            <FormRow label="Login">
                                <input class="input-text fill" type="text" title="New user's login" autocomplete="off" autocapitalize="none" spellcheck="false" required=true value={self.login.clone()} oninput={input(Msg::LoginInput)} />
                            </FormRow>

                            <FormRow label="Email" hint="Optional. Lets them sign in with it, or through Cloudflare Access.">
                                <input class="input-text fill" type="email" title="New user's email" autocomplete="off" spellcheck="false" value={self.email.clone()} oninput={input(Msg::EmailInput)} />
                            </FormRow>

                            <FormRow label="Role">
                                <select class="input-select" title="New user's role" onchange={on_role}>
                                    <option value="regular" selected={self.role == api::UserRole::Regular}>{"Regular user"}</option>
                                    <option value="admin" selected={self.role == api::UserRole::Admin}>{"Administrator"}</option>
                                </select>
                            </FormRow>

                            <FormRow label="" hint="They get a login link to choose a password with.">
                                <Button icon="user-plus" label="Add user" title="Add user" variant={Variant::Primary} disabled={!connected || self.creating || self.login.trim().is_empty()} />

                                if let Some(error) = &self.create_error {
                                    <span class="field-error" role="alert">{error.clone()}</span>
                                }
                            </FormRow>
                        </form>
                    </section>
                </div>
            </>
        }
    }
}

impl Users {
    fn view_user(&self, ctx: &Context<Self>, user: &api::User) -> Html {
        let link = ctx.link();
        let id = user.id;
        let me = id == ctx.props().me;
        let login = user.login.clone();

        let on_role = link.callback(move |e: Event| {
            let select: web_sys::HtmlSelectElement = e.target_unchecked_into();
            Msg::SetRole(id, parse_role(&select.value()))
        });

        let role = html! {
            if me {
                <span class="user-role">{role_name(user.role)}</span>
            } else {
                <select class="input-select" title={format!("Role of {login}")} onchange={on_role}>
                    <option value="regular" selected={user.role == api::UserRole::Regular}>{"Regular user"}</option>
                    <option value="admin" selected={user.role == api::UserRole::Admin}>{"Administrator"}</option>
                </select>
            }
        };

        let pending = self.pending.get(&id).copied();

        let link_state = match pending {
            Some(expires_at) => html! {
                <span class="user-link" data-test="user-link">
                    {"Link expires "}
                    {expires_at.human_date_time(self.time.clone()).lower().view()}
                </span>
            },
            None => html! {
                <span class="user-link muted" data-test="user-link">{"No login link"}</span>
            },
        };

        let new_link = self.new_links.get(&id).map(|new| {
            html! {
                <div class="user-new-link" role="cell">
                    <div class="input-group">
                        <input class="input-text fill" type="text" readonly=true title={format!("Login link for {login}")} value={new.url.clone()} />
                        <Button icon="clipboard" label="Copy" title={format!("Copy login link for {login}")} onclick={link.callback(move |_| Msg::Copy(id))} />
                    </div>

                    <span class="hint">
                        {format!("Send this to {login}. It works once and expires {}.", new.expires_at.relative_to(self.time.now()))}
                    </span>
                </div>
            }
        });

        let status = match &self.status {
            Some((sid, Status::Done(message))) if *sid == id => html! {
                <span class="field-ok user-status" role="status">{*message}</span>
            },
            Some((sid, Status::Failed(message))) if *sid == id => html! {
                <span class="field-error user-status" role="alert">{message.clone()}</span>
            },
            _ => Html::default(),
        };

        html! {
            <div class="user-row" role="row" data-test="user" data-login={login.clone()} data-user-id={id.to_string()}>
                <span class="user-login" role="cell">
                    <span class="user-name">{login.clone()}</span>
                    if me {
                        <span class="user-you">{"you"}</span>
                    }
                </span>
                <span class={classes!("user-email", user.email.is_none().then_some("muted"))} role="cell">
                    {user.email.clone().unwrap_or_else(|| "No email".to_owned())}
                </span>
                <span class="user-role-cell" role="cell">{role}</span>
                <span class="user-link-cell" role="cell">{link_state}</span>

                <div class="user-actions" role="cell">
                    <Button icon="link" label={if pending.is_some() { "New link" } else { "Login link" }} title={format!("New login link for {login}")} onclick={link.callback(move |_| Msg::GenerateLink(id))} />

                    if pending.is_some() {
                        <Button icon="link-slash" label="Revoke link" title={format!("Revoke login link for {login}")} onclick={link.callback(move |_| Msg::RevokeLink(id))} />
                    }

                    if !me {
                        <Button icon="arrow-left-start-on-rectangle" label="Sign out" title={format!("Sign {login} out everywhere")} onclick={link.callback(move |_| Msg::RevokeAccess(id))} />

                        if self.confirm_delete == Some(id) {
                            <span class="user-confirm">{format!("Delete {login}?")}</span>
                            <ConfirmDanger on_confirm={link.callback(move |_| Msg::Delete(id))} on_cancel={link.callback(|_| Msg::CancelDelete)} />
                        } else {
                            <Button icon="trash" label="Delete" title={format!("Delete {login}")} class="user-delete" variant={Variant::Danger} onclick={link.callback(move |_| Msg::ConfirmDelete(id))} />
                        }
                    }
                </div>

                {new_link}
                {status}
            </div>
        }
    }
}

fn parse_role(value: &str) -> api::UserRole {
    match value {
        "admin" => api::UserRole::Admin,
        _ => api::UserRole::Regular,
    }
}

fn role_name(role: api::UserRole) -> &'static str {
    match role {
        api::UserRole::Admin => "Administrator",
        api::UserRole::Regular => "Regular user",
    }
}
