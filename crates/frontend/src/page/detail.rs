//! Message handling shared by the show and movie detail pages: the owner's
//! image selection and its remotes. The person detail page shares the remotes.

use std::collections::{BTreeMap, BTreeSet, HashSet};

use musli_web::api::Request;
use musli_web::web03::prelude::*;
use yew::html::Scope;
use yew::prelude::*;

use crate::error::{CustomContext, Error, Message};
use crate::ui::{Button, GraphicsSourceFilter, ImageGallery, ImageItem, Modal, Variant};

pub(crate) enum ImageMsg {
    SelectImage(api::ImageKind, api::ImageId),
    ClearSelectedImage(api::ImageKind),
    SelectImageDone(Result<ws::Packet<api::SelectImage>, ws::Error>),
    ClearSelectedImageDone(Result<ws::Packet<api::ClearSelectedImage>, ws::Error>),
    PickBestImage(Option<api::ImageKind>),
    ResetImageSelection(api::ImageKind),
    PickBestImageDone(Result<ws::Packet<api::PickBestImages>, ws::Error>),
    ResetImageSelectionDone(Result<ws::Packet<api::ResetImageSelection>, ws::Error>),
    ToggleGraphicsSource(api::ImageSource),
}

/// What the page does after [`Graphics::update`].
pub(crate) enum ImageUpdate {
    Render(bool),
    /// The selection changed on the server: reload the owner.
    Reload,
    /// A selection was cleared: close the graphics modal and reload the owner.
    Cleared,
}

/// The owner's graphics, grouped by kind, and the requests that change which
/// one is selected.
#[derive(Default)]
pub(crate) struct Graphics {
    items: BTreeMap<api::ImageKind, Vec<ImageItem>>,
    present: BTreeSet<api::ImageSource>,
    hidden_sources: HashSet<api::ImageSource>,
    _select_req: ws::Request,
    _clear_req: ws::Request,
    _pick_best_req: ws::Request,
    _reset_req: ws::Request,
}

impl Graphics {
    pub(crate) fn clear(&mut self) {
        self.items.clear();
        self.present.clear();
    }

    pub(crate) fn set(
        &mut self,
        images: &[api::MediaImage],
        is_selected: impl Fn(api::ImageKind, &api::ImageKey) -> bool,
    ) {
        self.clear();

        for i in images {
            self.items.entry(i.kind).or_default().push(ImageItem {
                selected: is_selected(i.kind, i.image.key()),
                id: i.id,
                kind: i.kind,
                source: i.source,
                image: i.image.clone(),
            });

            self.present.insert(i.source);
        }
    }

    pub(crate) fn update<C>(
        &mut self,
        link: &Scope<C>,
        channel: &ws::Channel,
        owner: api::ImageOwner,
        msg: ImageMsg,
    ) -> Result<ImageUpdate, Error>
    where
        C: Component,
        C::Message: From<ImageMsg>,
    {
        let connected = channel.id() != ws::ChannelId::NONE;

        match msg {
            ImageMsg::SelectImage(kind, id) => {
                if let Some(images) = self.items.get_mut(&kind) {
                    for image in images {
                        image.selected = image.id == id;
                    }
                }

                if connected {
                    self._select_req = channel
                        .request()
                        .body(api::SelectImageRequest { id })
                        .on_packet(link.callback(ImageMsg::SelectImageDone))
                        .send();
                }

                Ok(ImageUpdate::Render(true))
            }
            ImageMsg::ClearSelectedImage(kind) => {
                if let Some(images) = self.items.get_mut(&kind) {
                    for image in images {
                        image.selected = false;
                    }
                }

                if connected {
                    self._clear_req = channel
                        .request()
                        .body(api::ClearSelectedImageRequest { owner, kind })
                        .on_packet(link.callback(ImageMsg::ClearSelectedImageDone))
                        .send();
                }

                Ok(ImageUpdate::Render(false))
            }
            ImageMsg::SelectImageDone(result) => {
                result.context(Message::SelectingImage)?;
                Ok(ImageUpdate::Reload)
            }
            ImageMsg::ClearSelectedImageDone(result) => {
                result.context(Message::ClearingImage)?;
                Ok(ImageUpdate::Cleared)
            }
            ImageMsg::PickBestImage(kind) => {
                if connected {
                    self._pick_best_req = channel
                        .request()
                        .body(api::PickBestImagesRequest { owner, kind })
                        .on_packet(link.callback(ImageMsg::PickBestImageDone))
                        .send();
                }

                Ok(ImageUpdate::Render(false))
            }
            ImageMsg::ResetImageSelection(kind) => {
                if connected {
                    self._reset_req = channel
                        .request()
                        .body(api::ResetImageSelectionRequest { owner, kind })
                        .on_packet(link.callback(ImageMsg::ResetImageSelectionDone))
                        .send();
                }

                Ok(ImageUpdate::Render(false))
            }
            ImageMsg::PickBestImageDone(result) => {
                result.context(Message::SelectingImage)?;
                Ok(ImageUpdate::Reload)
            }
            ImageMsg::ResetImageSelectionDone(result) => {
                result.context(Message::SelectingImage)?;
                Ok(ImageUpdate::Reload)
            }
            ImageMsg::ToggleGraphicsSource(source) => {
                if !self.hidden_sources.remove(&source) {
                    self.hidden_sources.insert(source);
                }

                Ok(ImageUpdate::Render(true))
            }
        }
    }

    /// The graphics modal: every kind's gallery, filtered by source.
    pub(crate) fn view_modal<C>(
        &self,
        link: &Scope<C>,
        user_selected: impl Fn(api::ImageKind) -> bool,
        on_close: Callback<()>,
    ) -> Html
    where
        C: Component,
        C::Message: From<ImageMsg>,
    {
        let hidden = self.hidden_sources.clone();

        html! {
            <Modal icon="photo" title="Graphics" {on_close}>
                <div class="row desktop-align-end">
                    <GraphicsSourceFilter present={self.present.clone()} hidden={hidden.clone()} on_toggle={link.callback(ImageMsg::ToggleGraphicsSource)} />
                    <Button icon="sparkles" variant={Variant::Primary} title="Pick the best graphic for every kind" label="Pick best (all)" onclick={link.callback(|_| ImageMsg::PickBestImage(None))} />
                </div>
                {for self.items.iter().filter_map(|(&kind, items)| {
                    let items: Vec<ImageItem> = items
                        .iter()
                        .filter(|item| !hidden.contains(&item.source))
                        .cloned()
                        .collect();

                    if items.is_empty() {
                        return None;
                    }

                    Some(html! {
                        <ImageGallery
                            {items}
                            {kind}
                            user_selected={user_selected(kind)}
                            on_select={link.callback(move |id| ImageMsg::SelectImage(kind, id))}
                            on_clear={link.callback(move |_| ImageMsg::ClearSelectedImage(kind))}
                            on_pick_best={Some(link.callback(move |_| ImageMsg::PickBestImage(Some(kind))))}
                            on_reset={Some(link.callback(move |_| ImageMsg::ResetImageSelection(kind)))}
                        />
                    })
                })}
            </Modal>
        }
    }
}

pub(crate) enum RemoteMsg {
    SetRemoteEnabled(api::RemoteId, bool),
    SetRemoteEnabledDone(Result<(), ws::Error>),
    SetRemoteSyncKinds(api::RemoteId, Option<api::SyncKindSet>),
    SetRemoteSyncKindsDone(Result<(), ws::Error>),
    ReorderRemotes(Vec<api::RemoteId>),
    ReorderRemotesDone(Result<(), ws::Error>),
    AddRemote(Option<String>, api::Remote),
    EditRemote(api::RemoteId, Option<String>, api::Remote),
    RemoveRemote(api::RemoteId),
    PurgeRemoteCache(api::RemoteId),
    RemoteDone(Result<(), ws::Error>),
}

/// What the page does after [`Remotes::update`].
pub(crate) enum RemoteUpdate {
    Render(bool),
    /// A remote was added, edited or removed: reload the owner.
    Reload,
}

/// A remote edit, as sent to the server for one owner.
pub(crate) enum RemoteOp {
    SetEnabled(api::RemoteId, bool),
    SetSyncKinds(api::RemoteId, Option<api::SyncKindSet>),
    Reorder(Vec<api::RemoteId>),
    Add(Option<String>, api::Remote),
    Edit(api::RemoteId, Option<String>, api::Remote),
    Remove(api::RemoteId),
    PurgeCache(api::RemoteId),
}

/// An owner of remotes, which picks its own request type for each edit.
pub(crate) trait RemoteOwner: Copy {
    fn send(
        self,
        channel: &ws::Channel,
        op: RemoteOp,
        done: Callback<Result<(), ws::Error>>,
    ) -> ws::Request;
}

fn send<B>(channel: &ws::Channel, body: B, done: Callback<Result<(), ws::Error>>) -> ws::Request
where
    B: Request,
{
    channel
        .request()
        .body(body)
        .on_packet(done.reform(|r: Result<ws::Packet<B::Endpoint>, ws::Error>| r.map(|_| ())))
        .send()
}

impl RemoteOwner for api::ShowId {
    fn send(
        self,
        channel: &ws::Channel,
        op: RemoteOp,
        done: Callback<Result<(), ws::Error>>,
    ) -> ws::Request {
        let id = self;

        match op {
            RemoteOp::SetEnabled(remote_id, enabled) => send(
                channel,
                api::SetShowRemoteEnabledRequest {
                    id,
                    remote_id,
                    enabled,
                },
                done,
            ),
            RemoteOp::SetSyncKinds(remote_id, sync_kinds) => send(
                channel,
                api::SetShowRemoteSyncKindsRequest {
                    id,
                    remote_id,
                    sync_kinds,
                },
                done,
            ),
            RemoteOp::Reorder(remote_ids) => send(
                channel,
                api::ReorderShowRemotesRequest { id, remote_ids },
                done,
            ),
            RemoteOp::Add(slug, remote) => send(
                channel,
                api::AddShowRemoteRequest { id, slug, remote },
                done,
            ),
            RemoteOp::Edit(remote_id, slug, remote) => send(
                channel,
                api::UpdateShowRemoteRequest {
                    id,
                    remote_id,
                    slug,
                    remote,
                },
                done,
            ),
            RemoteOp::Remove(remote_id) => send(
                channel,
                api::RemoveShowRemoteRequest { id, remote_id },
                done,
            ),
            RemoteOp::PurgeCache(remote_id) => send(
                channel,
                api::PurgeShowRemoteCacheRequest { id, remote_id },
                done,
            ),
        }
    }
}

impl RemoteOwner for api::MovieId {
    fn send(
        self,
        channel: &ws::Channel,
        op: RemoteOp,
        done: Callback<Result<(), ws::Error>>,
    ) -> ws::Request {
        let id = self;

        match op {
            RemoteOp::SetEnabled(remote_id, enabled) => send(
                channel,
                api::SetMovieRemoteEnabledRequest {
                    id,
                    remote_id,
                    enabled,
                },
                done,
            ),
            RemoteOp::SetSyncKinds(remote_id, sync_kinds) => send(
                channel,
                api::SetMovieRemoteSyncKindsRequest {
                    id,
                    remote_id,
                    sync_kinds,
                },
                done,
            ),
            RemoteOp::Reorder(remote_ids) => send(
                channel,
                api::ReorderMovieRemotesRequest { id, remote_ids },
                done,
            ),
            RemoteOp::Add(slug, remote) => send(
                channel,
                api::AddMovieRemoteRequest { id, slug, remote },
                done,
            ),
            RemoteOp::Edit(remote_id, slug, remote) => send(
                channel,
                api::UpdateMovieRemoteRequest {
                    id,
                    remote_id,
                    slug,
                    remote,
                },
                done,
            ),
            RemoteOp::Remove(remote_id) => send(
                channel,
                api::RemoveMovieRemoteRequest { id, remote_id },
                done,
            ),
            RemoteOp::PurgeCache(remote_id) => send(
                channel,
                api::PurgeMovieRemoteCacheRequest { id, remote_id },
                done,
            ),
        }
    }
}

impl RemoteOwner for api::PersonId {
    fn send(
        self,
        channel: &ws::Channel,
        op: RemoteOp,
        done: Callback<Result<(), ws::Error>>,
    ) -> ws::Request {
        let id = self;

        match op {
            RemoteOp::SetEnabled(remote_id, enabled) => send(
                channel,
                api::SetPersonRemoteEnabledRequest {
                    id,
                    remote_id,
                    enabled,
                },
                done,
            ),
            RemoteOp::SetSyncKinds(remote_id, sync_kinds) => send(
                channel,
                api::SetPersonRemoteSyncKindsRequest {
                    id,
                    remote_id,
                    sync_kinds,
                },
                done,
            ),
            RemoteOp::Reorder(remote_ids) => send(
                channel,
                api::ReorderPersonRemotesRequest { id, remote_ids },
                done,
            ),
            RemoteOp::Add(slug, remote) => send(
                channel,
                api::AddPersonRemoteRequest { id, slug, remote },
                done,
            ),
            RemoteOp::Edit(remote_id, slug, remote) => send(
                channel,
                api::UpdatePersonRemoteRequest {
                    id,
                    remote_id,
                    slug,
                    remote,
                },
                done,
            ),
            RemoteOp::Remove(remote_id) => send(
                channel,
                api::RemovePersonRemoteRequest { id, remote_id },
                done,
            ),
            RemoteOp::PurgeCache(remote_id) => send(
                channel,
                api::PurgePersonRemoteCacheRequest { id, remote_id },
                done,
            ),
        }
    }
}

/// The requests editing an owner's remotes. Each kind of edit has its own
/// slot so one does not cancel another in flight.
#[derive(Default)]
pub(crate) struct Remotes {
    _set_enabled_req: ws::Request,
    _set_sync_kinds_req: ws::Request,
    _reorder_req: ws::Request,
    _edit_req: ws::Request,
}

impl Remotes {
    /// Handles `msg` for `owner`, updating its loaded `remotes` in place
    /// ahead of the server.
    pub(crate) fn update<C>(
        &mut self,
        link: &Scope<C>,
        channel: &ws::Channel,
        owner: impl RemoteOwner,
        remotes: Option<&mut Vec<api::RemoteEntry>>,
        msg: RemoteMsg,
    ) -> Result<RemoteUpdate, Error>
    where
        C: Component,
        C::Message: From<RemoteMsg>,
    {
        let connected = channel.id() != ws::ChannelId::NONE;

        let (op, render) = match msg {
            RemoteMsg::SetRemoteEnabled(remote_id, enabled) => {
                if let Some(entry) = remotes.and_then(|r| r.iter_mut().find(|e| e.id == remote_id))
                {
                    entry.enabled = enabled;
                }

                (RemoteOp::SetEnabled(remote_id, enabled), true)
            }
            RemoteMsg::SetRemoteSyncKinds(remote_id, sync_kinds) => {
                if let Some(entry) = remotes.and_then(|r| r.iter_mut().find(|e| e.id == remote_id))
                {
                    entry.sync_kinds = sync_kinds;
                }

                (RemoteOp::SetSyncKinds(remote_id, sync_kinds), true)
            }
            RemoteMsg::ReorderRemotes(remote_ids) => {
                if let Some(remotes) = remotes {
                    remotes.sort_by_key(|e| remote_ids.iter().position(|id| *id == e.id));
                }

                (RemoteOp::Reorder(remote_ids), true)
            }
            RemoteMsg::AddRemote(slug, remote) => (RemoteOp::Add(slug, remote), false),
            RemoteMsg::EditRemote(remote_id, slug, remote) => {
                (RemoteOp::Edit(remote_id, slug, remote), false)
            }
            RemoteMsg::RemoveRemote(remote_id) => (RemoteOp::Remove(remote_id), false),
            RemoteMsg::PurgeRemoteCache(remote_id) => (RemoteOp::PurgeCache(remote_id), false),
            RemoteMsg::SetRemoteEnabledDone(result) => {
                result.context(Message::SettingRemoteEnabled)?;
                return Ok(RemoteUpdate::Render(false));
            }
            RemoteMsg::SetRemoteSyncKindsDone(result) => {
                result.context(Message::SettingRemoteSyncKinds)?;
                return Ok(RemoteUpdate::Render(false));
            }
            RemoteMsg::ReorderRemotesDone(result) => {
                result.context(Message::ReorderingRemotes)?;
                return Ok(RemoteUpdate::Render(false));
            }
            RemoteMsg::RemoteDone(result) => {
                result.context(Message::EditingRemotes)?;
                return Ok(RemoteUpdate::Reload);
            }
        };

        if connected {
            let (slot, done): (_, fn(_) -> RemoteMsg) = match &op {
                RemoteOp::SetEnabled(..) => {
                    (&mut self._set_enabled_req, RemoteMsg::SetRemoteEnabledDone)
                }
                RemoteOp::SetSyncKinds(..) => (
                    &mut self._set_sync_kinds_req,
                    RemoteMsg::SetRemoteSyncKindsDone,
                ),
                RemoteOp::Reorder(..) => (&mut self._reorder_req, RemoteMsg::ReorderRemotesDone),
                _ => (&mut self._edit_req, RemoteMsg::RemoteDone),
            };

            *slot = owner.send(channel, op, link.callback(done));
        }

        Ok(RemoteUpdate::Render(render))
    }
}
