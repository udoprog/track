use api::TimeInfo;
use musli_web::web03::prelude::*;
use yew::prelude::*;

use crate::SetupChannel;
use crate::background::Background;
use crate::error::{CustomContext, Error, Message};
use crate::router::{PersonQuery, Route, Router, ShowDetailQuery};
use crate::ui::{
    Button, ConfirmDanger, ContextMenu, DetailSkeleton, Image, Modal, RemoteEditor,
    RemoteSourceKind, TranslatedText, Variant,
};

/// Load state for the person this page renders.
enum PersonState {
    Loading,
    Missing,
    Loaded(Box<api::Person>),
}

pub(crate) struct PersonDetail {
    channel: ws::Channel,
    person: PersonState,
    credits: Vec<api::PersonCredit>,
    remote_editor: bool,
    settings: bool,
    confirming_delete: bool,
    remove_anchor: NodeRef,
    syncing: bool,
    global_sync_kinds: Vec<api::SourceSyncKinds>,
    time: TimeInfo,
    _time_handle: ContextHandle<TimeInfo>,
    background: Background,
    router: Router,
    _setup: SetupChannel,
    _broadcast: ws::Listener,
    _person_req: ws::Request,
    _credits_req: ws::Request,
    _config_req: ws::Request,
    _remote_req: ws::Request,
    _sync_req: ws::Request,
    _delete_req: ws::Request,
}

pub(crate) enum Msg {
    Channel(Result<ws::Channel, ws::Error>),
    AppBroadcast(Result<ws::Packet<api::AppBroadcast>, ws::Error>),
    PersonLoaded(Result<ws::Packet<api::GetPerson>, ws::Error>),
    CreditsLoaded(Result<ws::Packet<api::ListPersonCredits>, ws::Error>),
    ConfigLoaded(Result<ws::Packet<api::GetConfig>, ws::Error>),
    Navigate(Route),
    SyncPerson,
    SyncDone(Result<ws::Packet<api::SyncPerson>, ws::Error>),
    OpenSettings,
    CloseSettings,
    AskDelete,
    CancelDelete,
    DeletePerson,
    DeleteDone(Result<ws::Packet<api::DeletePerson>, ws::Error>),
    SetTime(TimeInfo),
    OpenRemoteEditor,
    CloseRemoteEditor,
    AddRemote(Option<String>, api::Remote),
    EditRemote(api::RemoteId, Option<String>, api::Remote),
    RemoveRemote(api::RemoteId),
    PurgeRemoteCache(api::RemoteId),
    SetRemoteEnabled(api::RemoteId, bool),
    SetRemoteSyncKinds(api::RemoteId, Option<api::SyncKindSet>),
    ReorderRemotes(Vec<api::RemoteId>),
    RemoteDone(Result<(), ws::Error>),
}

#[derive(Properties, PartialEq)]
pub(crate) struct Props {
    pub(crate) person_id: api::PersonId,
}

impl Component for PersonDetail {
    type Message = Msg;
    type Properties = Props;

    fn create(ctx: &Context<Self>) -> Self {
        let (ws, _) = ctx
            .link()
            .context::<ws::Handle>(Callback::noop())
            .expect("Expected ws::Handle in context");

        let _setup = SetupChannel::new(ws.clone(), ctx.link().callback(Msg::Channel));
        let _broadcast = ws.on_broadcast(ctx.link().callback(Msg::AppBroadcast));

        let (background, _) = ctx
            .link()
            .context::<Background>(Callback::noop())
            .expect("Expected background handle in context");

        let (router, _) = ctx
            .link()
            .context::<Router>(Callback::noop())
            .expect("Expected router in context");

        let (time, _time_handle) = ctx
            .link()
            .context::<TimeInfo>(ctx.link().callback(Msg::SetTime))
            .expect("Expected a configured time zone");

        Self {
            channel: ws::Channel::default(),
            person: PersonState::Loading,
            credits: Vec::new(),
            remote_editor: false,
            settings: false,
            confirming_delete: false,
            remove_anchor: NodeRef::default(),
            syncing: false,
            global_sync_kinds: Vec::new(),
            time,
            _time_handle,
            background,
            router,
            _setup,
            _broadcast,
            _person_req: ws::Request::default(),
            _credits_req: ws::Request::default(),
            _config_req: ws::Request::default(),
            _remote_req: ws::Request::default(),
            _sync_req: ws::Request::default(),
            _delete_req: ws::Request::default(),
        }
    }

    fn update(&mut self, ctx: &Context<Self>, msg: Self::Message) -> bool {
        match self.try_update(ctx, msg) {
            Ok(render) => render,
            Err(e) => {
                self.background.error(e);
                false
            }
        }
    }

    fn destroy(&mut self, _ctx: &Context<Self>) {
        self.background.title(None);
    }

    fn view(&self, ctx: &Context<Self>) -> Html {
        let person = match &self.person {
            PersonState::Loading => return html! { <DetailSkeleton /> },
            PersonState::Missing => {
                return html! {
                    <div class="row center">
                        <span class="item-inline-more">{"No such person"}</span>
                    </div>
                };
            }
            PersonState::Loaded(person) => person,
        };

        html! {
            <>
                { self.view_header(ctx, person) }
                { self.view_filmography(ctx) }

                if self.settings {
                    { self.view_settings(ctx, person) }
                }

                if self.remote_editor {
                    <RemoteEditor
                        title={person.name.title().unwrap_or("Unknown").to_owned()}
                        kind={RemoteSourceKind::Person}
                        remotes={person.remotes.clone()}
                        on_add={ctx.link().callback(|(slug, remote)| Msg::AddRemote(slug, remote))}
                        on_edit={ctx.link().callback(|(id, slug, remote)| Msg::EditRemote(id, slug, remote))}
                        on_remove={ctx.link().callback(Msg::RemoveRemote)}
                        on_purge_cache={ctx.link().callback(Msg::PurgeRemoteCache)}
                        on_set_enabled={ctx.link().callback(|(id, enabled)| Msg::SetRemoteEnabled(id, enabled))}
                        on_reorder={ctx.link().callback(Msg::ReorderRemotes)}
                        on_set_sync_kinds={ctx.link().callback(|(id, kinds)| Msg::SetRemoteSyncKinds(id, kinds))}
                        global_sync_kinds={self.global_sync_kinds.clone()}
                        on_close={ctx.link().callback(|_| Msg::CloseRemoteEditor)}
                    />
                }
            </>
        }
    }
}

impl PersonDetail {
    fn try_update(&mut self, ctx: &Context<Self>, msg: Msg) -> Result<bool, Error> {
        match msg {
            Msg::Channel(result) => {
                self.channel = result?;

                if self.channel.id() != ws::ChannelId::NONE {
                    self.load(ctx);
                }

                Ok(true)
            }
            Msg::AppBroadcast(packet) => {
                let event = packet?.decode_event()?;

                if event.channel == self.channel.id() {
                    return Ok(false);
                }

                let id = ctx.props().person_id;

                match &event.kind {
                    api::AppEventKind::PersonChanged { person_id } if *person_id == id => {
                        if self.channel.id() != ws::ChannelId::NONE {
                            self.load(ctx);
                        }
                        Ok(false)
                    }
                    // A person's filmography changes when a show/movie's credits sync.
                    api::AppEventKind::CreditsChanged { .. } => {
                        if self.channel.id() != ws::ChannelId::NONE {
                            self.load(ctx);
                        }
                        Ok(false)
                    }
                    api::AppEventKind::TaskAdded { task }
                    | api::AppEventKind::TaskStarted { task } => {
                        if matches!(&task.kind, api::TaskKind::SyncPerson { person_id, .. } if *person_id == id)
                        {
                            self.syncing = true;
                            return Ok(true);
                        }
                        Ok(false)
                    }
                    api::AppEventKind::TaskCompleted { task } => {
                        if matches!(&task.kind, api::TaskKind::SyncPerson { person_id, .. } if *person_id == id)
                        {
                            self.syncing = false;
                            self.load(ctx);
                            return Ok(true);
                        }
                        Ok(false)
                    }
                    _ => Ok(false),
                }
            }
            Msg::PersonLoaded(result) => {
                let person = result
                    .context(Message::LoadingPerson)?
                    .decode()
                    .context(Message::LoadingPerson)?;

                match person {
                    Some(person) => {
                        self.background
                            .title(person.name.title().map(str::to_owned));
                        self.person = PersonState::Loaded(Box::new(person));
                    }
                    None => {
                        self.background.title(None);
                        self.person = PersonState::Missing;
                    }
                }

                Ok(true)
            }
            Msg::CreditsLoaded(result) => {
                self.credits = result
                    .context(Message::LoadingPersonCredits)?
                    .decode()
                    .context(Message::LoadingPersonCredits)?
                    .credits;
                Ok(true)
            }
            Msg::ConfigLoaded(result) => {
                let config = result
                    .context(Message::LoadingConfig)?
                    .decode()
                    .context(Message::LoadingConfig)?
                    .config;
                self.global_sync_kinds = config.sync_kinds;
                Ok(true)
            }
            Msg::Navigate(route) => {
                self.router.push(route);
                Ok(false)
            }
            Msg::SyncPerson => {
                let id = ctx.props().person_id;

                if self.channel.id() != ws::ChannelId::NONE {
                    self._sync_req = self
                        .channel
                        .request()
                        .body(api::SyncPersonRequest { id })
                        .on_packet(ctx.link().callback(Msg::SyncDone))
                        .send();
                }

                Ok(true)
            }
            Msg::SyncDone(result) => {
                result.context(Message::SyncingPerson)?;
                Ok(false)
            }
            Msg::SetTime(time) => {
                self.time = time;
                Ok(true)
            }
            Msg::OpenSettings => {
                self.settings = true;
                Ok(true)
            }
            Msg::CloseSettings => {
                self.settings = false;
                Ok(true)
            }
            Msg::AskDelete => {
                self.confirming_delete = true;
                Ok(true)
            }
            Msg::CancelDelete => {
                self.confirming_delete = false;
                Ok(true)
            }
            Msg::DeletePerson => {
                let id = ctx.props().person_id;

                if self.channel.id() != ws::ChannelId::NONE {
                    self._delete_req = self
                        .channel
                        .request()
                        .body(api::DeletePersonRequest { id })
                        .on_packet(ctx.link().callback(Msg::DeleteDone))
                        .send();
                }

                Ok(false)
            }
            Msg::DeleteDone(result) => {
                result.context(Message::DeletingPerson)?;
                // The person is gone; leave the page.
                self.router.push(Route::People(PersonQuery::default()));
                Ok(false)
            }
            Msg::OpenRemoteEditor => {
                self.remote_editor = true;
                self.settings = false;
                Ok(true)
            }
            Msg::CloseRemoteEditor => {
                self.remote_editor = false;
                Ok(true)
            }
            Msg::AddRemote(slug, remote) => {
                let id = ctx.props().person_id;

                if self.channel.id() != ws::ChannelId::NONE {
                    self._remote_req = self
                        .channel
                        .request()
                        .body(api::AddPersonRemoteRequest { id, slug, remote })
                        .on_packet(ctx.link().callback(
                            |r: Result<ws::Packet<api::AddPersonRemote>, ws::Error>| {
                                Msg::RemoteDone(r.map(|_| ()))
                            },
                        ))
                        .send();
                }

                Ok(false)
            }
            Msg::EditRemote(remote_id, slug, remote) => {
                let id = ctx.props().person_id;

                if self.channel.id() != ws::ChannelId::NONE {
                    self._remote_req = self
                        .channel
                        .request()
                        .body(api::UpdatePersonRemoteRequest {
                            id,
                            remote_id,
                            slug,
                            remote,
                        })
                        .on_packet(ctx.link().callback(
                            |r: Result<ws::Packet<api::UpdatePersonRemote>, ws::Error>| {
                                Msg::RemoteDone(r.map(|_| ()))
                            },
                        ))
                        .send();
                }

                Ok(false)
            }
            Msg::RemoveRemote(remote_id) => {
                let id = ctx.props().person_id;

                if self.channel.id() != ws::ChannelId::NONE {
                    self._remote_req = self
                        .channel
                        .request()
                        .body(api::RemovePersonRemoteRequest { id, remote_id })
                        .on_packet(ctx.link().callback(
                            |r: Result<ws::Packet<api::RemovePersonRemote>, ws::Error>| {
                                Msg::RemoteDone(r.map(|_| ()))
                            },
                        ))
                        .send();
                }

                Ok(false)
            }
            Msg::PurgeRemoteCache(remote_id) => {
                let id = ctx.props().person_id;

                if self.channel.id() != ws::ChannelId::NONE {
                    self._remote_req = self
                        .channel
                        .request()
                        .body(api::PurgePersonRemoteCacheRequest { id, remote_id })
                        .on_packet(ctx.link().callback(
                            |r: Result<ws::Packet<api::PurgePersonRemoteCache>, ws::Error>| {
                                Msg::RemoteDone(r.map(|_| ()))
                            },
                        ))
                        .send();
                }

                Ok(false)
            }
            Msg::SetRemoteEnabled(remote_id, enabled) => {
                if let PersonState::Loaded(person) = &mut self.person
                    && let Some(entry) = person.remotes.iter_mut().find(|e| e.id == remote_id)
                {
                    entry.enabled = enabled;
                }

                let id = ctx.props().person_id;

                if self.channel.id() != ws::ChannelId::NONE {
                    self._remote_req = self
                        .channel
                        .request()
                        .body(api::SetPersonRemoteEnabledRequest {
                            id,
                            remote_id,
                            enabled,
                        })
                        .on_packet(ctx.link().callback(
                            |r: Result<ws::Packet<api::SetPersonRemoteEnabled>, ws::Error>| {
                                Msg::RemoteDone(r.map(|_| ()))
                            },
                        ))
                        .send();
                }

                Ok(true)
            }
            Msg::SetRemoteSyncKinds(remote_id, sync_kinds) => {
                if let PersonState::Loaded(person) = &mut self.person
                    && let Some(entry) = person.remotes.iter_mut().find(|e| e.id == remote_id)
                {
                    entry.sync_kinds = sync_kinds;
                }

                let id = ctx.props().person_id;

                if self.channel.id() != ws::ChannelId::NONE {
                    self._remote_req = self
                        .channel
                        .request()
                        .body(api::SetPersonRemoteSyncKindsRequest {
                            id,
                            remote_id,
                            sync_kinds,
                        })
                        .on_packet(ctx.link().callback(
                            |r: Result<ws::Packet<api::SetPersonRemoteSyncKinds>, ws::Error>| {
                                Msg::RemoteDone(r.map(|_| ()))
                            },
                        ))
                        .send();
                }

                Ok(true)
            }
            Msg::ReorderRemotes(remote_ids) => {
                if let PersonState::Loaded(person) = &mut self.person {
                    person
                        .remotes
                        .sort_by_key(|e| remote_ids.iter().position(|id| *id == e.id));
                }

                let id = ctx.props().person_id;

                if self.channel.id() != ws::ChannelId::NONE {
                    self._remote_req = self
                        .channel
                        .request()
                        .body(api::ReorderPersonRemotesRequest { id, remote_ids })
                        .on_packet(ctx.link().callback(
                            |r: Result<ws::Packet<api::ReorderPersonRemotes>, ws::Error>| {
                                Msg::RemoteDone(r.map(|_| ()))
                            },
                        ))
                        .send();
                }

                Ok(true)
            }
            Msg::RemoteDone(result) => {
                result.context(Message::EditingRemotes)?;
                self.load(ctx);
                Ok(false)
            }
        }
    }

    fn load(&mut self, ctx: &Context<Self>) {
        if self.channel.id() == ws::ChannelId::NONE {
            return;
        }

        let id = ctx.props().person_id;

        self._person_req = self
            .channel
            .request()
            .body(api::GetPersonRequest { id })
            .on_packet(ctx.link().callback(Msg::PersonLoaded))
            .send();

        self._credits_req = self
            .channel
            .request()
            .body(api::ListPersonCreditsRequest { id })
            .on_packet(ctx.link().callback(Msg::CreditsLoaded))
            .send();

        self._config_req = self
            .channel
            .request()
            .body(api::GetConfigRequest)
            .on_packet(ctx.link().callback(Msg::ConfigLoaded))
            .send();
    }

    fn view_header(&self, ctx: &Context<Self>, person: &api::Person) -> Html {
        let link = ctx.link();
        let name = person.name.title().unwrap_or("Unknown").to_owned();

        html! {
            <div class="person-detail-header">
                <Image class="person-detail-photo" placeholder={true} placeholder_icon="user" src={person.profile.clone()} alt={name.clone()} />

                <div class="person-detail-info">
                    <div class="row-split">
                        <h1>{ name }</h1>
                        <div class="input-group">
                            <Button icon="arrow-path" spin={self.syncing} title="Sync now" text="Sync" onclick={link.callback(|_| Msg::SyncPerson)} />
                            <Button icon="cog-6-tooth" title="Settings" onclick={link.callback(|_| Msg::OpenSettings)} />
                            <Button node_ref={self.remove_anchor.clone()} icon="trash" variant={Variant::Danger} class="detached" title="Delete person" expanded={Some(self.confirming_delete)} haspopup="dialog" onclick={link.callback(|_| Msg::AskDelete)} />

                            if self.confirming_delete {
                                <ContextMenu prompt="Delete person" label={person.name.title().map(str::to_owned)} anchor={self.remove_anchor.clone()} on_close={link.callback(|_| Msg::CancelDelete)}>
                                    <ConfirmDanger
                                        on_confirm={link.callback(|_| Msg::DeletePerson)}
                                        on_cancel={link.callback(|_| Msg::CancelDelete)}
                                    />
                                </ContextMenu>
                            }
                        </div>
                    </div>

                    if let Some(department) = &person.department {
                        <div class="text-muted">{ department.clone() }</div>
                    }

                    <div class="row detail-sources">
                        {for person.remotes.iter().filter_map(|r| {
                            let url = r.remote.person_url(r.slug.as_deref())?;
                            let id = r.remote.source().as_id();

                            Some(html! {
                                <a class="item-inline-source" href={url} target="_blank" rel="noopener noreferrer" title={format!("Open on {id}")}>
                                    <span class={classes!("logo", id)} />
                                </a>
                            })
                        })}
                    </div>

                    <TranslatedText strings={person.biography.clone()} class="person-biography" />
                </div>
            </div>
        }
    }

    fn view_settings(&self, ctx: &Context<Self>, person: &api::Person) -> Html {
        let link = ctx.link();

        let last_synced = person
            .last_synced_at
            .map(|ts| AttrValue::from(ts.human_date_time(self.time.clone())));

        html! {
            <Modal icon="cog-6-tooth" title="Settings" on_close={link.callback(|_| Msg::CloseSettings)}>
                <div class="form">
                    <div class="input-group">
                        <span class="input-label has-text">{"Last Sync"}</span>

                        if let Some(ts) = last_synced {
                            <div class="input-text has-text fill" title="Last synced at">
                                <span>{ts}</span>
                            </div>
                        } else {
                            <div class="input-text has-text fill text-muted">
                                <span>{"Never synced"}</span>
                            </div>
                        }

                        if !person.remotes.is_empty() {
                            <Button icon="arrow-path" spin={self.syncing} onclick={link.callback(|_| Msg::SyncPerson)} title="Sync now" text="Sync" />
                        }
                    </div>

                    <div class="field">
                        <Button icon="identification" label="Remotes" title="Edit remotes" onclick={link.callback(|_| Msg::OpenRemoteEditor)} />

                        <span class="hint">{"Edit the TMDB, IMDb, and other remote identifiers used to sync."}</span>
                    </div>
                </div>
            </Modal>
        }
    }

    fn view_filmography(&self, ctx: &Context<Self>) -> Html {
        if self.credits.is_empty() {
            return html! {};
        }

        // One card per show or movie, in order of first appearance, listing
        // every role the person had in it.
        let mut titles = Vec::<(&api::PersonCredit, Vec<String>)>::new();

        for credit in &self.credits {
            // Prefer the character (cast); fall back to the crew job.
            let role = credit
                .character
                .character()
                .map(str::to_owned)
                .or_else(|| credit.job.clone());

            let at = match titles.iter().position(|(c, _)| c.owner == credit.owner) {
                Some(at) => at,
                None => {
                    titles.push((credit, Vec::new()));
                    titles.len() - 1
                }
            };

            if let Some(role) = role
                && !titles[at].1.contains(&role)
            {
                titles[at].1.push(role);
            }
        }

        html! {
            <section class="filmography">
                <h2>{"Known for"}</h2>

                <div class="person-grid">
                    { for titles.iter().map(|(c, roles)| self.view_credit(ctx, c, roles)) }
                </div>
            </section>
        }
    }

    fn view_credit(
        &self,
        ctx: &Context<Self>,
        credit: &api::PersonCredit,
        roles: &[String],
    ) -> Html {
        let title = credit.title.title().unwrap_or("Untitled").to_owned();

        let route = match credit.owner {
            api::CreditOwner::Show(id) => Route::ShowDetail(id, ShowDetailQuery::default()),
            api::CreditOwner::Movie(id) => Route::MovieDetail(id),
        };

        let onclick = ctx.link().callback(move |_| Msg::Navigate(route.clone()));

        html! {
            <div class="person-card clickable lift" {onclick}>
                <Image class="person-photo artwork" placeholder={true} src={credit.poster.clone()} alt={title.clone()} />

                <div class="person-info">
                    <div class="person-name">{ title }</div>

                    if !roles.is_empty() {
                        <div class="person-department person-roles text-muted" title={roles.join(", ")}>{ roles.join(", ") }</div>
                    }
                </div>
            </div>
        }
    }
}
