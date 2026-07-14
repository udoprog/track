use musli_web::web03::prelude::*;
use yew::prelude::*;

use crate::SetupChannel;
use crate::background::Background;
use crate::error::{CustomContext, Error, Message};

use super::{Button, Modal, Variant};

#[derive(Properties, PartialEq)]
pub(crate) struct Props {
    pub(crate) episode_id: api::EpisodeId,
    pub(crate) title: AttrValue,
    pub(crate) on_close: Callback<()>,
}

pub(crate) enum Msg {
    Channel(Result<ws::Channel, ws::Error>),
    Loaded(Result<ws::Packet<api::GetEpisodeCache>, ws::Error>),
    Purge(api::RemoteSource),
    PurgeDone(Result<ws::Packet<api::PurgeEpisodeCache>, ws::Error>),
}

/// The per-source conditional-request state stored for one episode, with a per-source
/// clear. The show-level equivalent lives in the remote editor; an episode is not
/// addressed by a remote id, so its entries are keyed by source alone.
pub(crate) struct EpisodeCacheModal {
    channel: ws::Channel,
    _setup: SetupChannel,
    _req: ws::Request,
    _purge_req: ws::Request,
    /// `None` while loading, `Some` once the response has arrived.
    entries: Option<Vec<api::EpisodeCacheEntry>>,
    background: Background,
}

impl Component for EpisodeCacheModal {
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
            .expect("Expected Background in context");

        let _setup = SetupChannel::new(ws.clone(), ctx.link().callback(Msg::Channel));

        Self {
            channel: ws::Channel::default(),
            _setup,
            _req: ws::Request::default(),
            _purge_req: ws::Request::default(),
            entries: None,
            background,
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

    fn view(&self, ctx: &Context<Self>) -> Html {
        let on_close = ctx.props().on_close.clone();
        let title = ctx.props().title.clone();

        html! {
            <Modal icon="circle-stack" title={title} on_close={on_close}>
                { self.view_content(ctx) }
            </Modal>
        }
    }
}

impl EpisodeCacheModal {
    fn try_update(&mut self, ctx: &Context<Self>, msg: Msg) -> Result<bool, Error> {
        match msg {
            Msg::Channel(result) => {
                self.channel = result?;
                self.load(ctx);
                Ok(true)
            }
            Msg::Loaded(result) => {
                let resp = result
                    .context(Message::LoadingEpisodeCache)?
                    .decode()
                    .context(Message::LoadingEpisodeCache)?;
                self.entries = Some(resp.entries);
                Ok(true)
            }
            Msg::Purge(source) => {
                if self.channel.id() == ws::ChannelId::NONE {
                    return Ok(false);
                }

                self._purge_req = self
                    .channel
                    .request()
                    .body(api::PurgeEpisodeCacheRequest {
                        episode_id: ctx.props().episode_id,
                        source,
                    })
                    .on_packet(ctx.link().callback(Msg::PurgeDone))
                    .send();

                Ok(false)
            }
            Msg::PurgeDone(result) => {
                result.context(Message::PurgingEpisodeCache)?;
                // No broadcast carries episode cache, so reload to reflect the deletion.
                self.load(ctx);
                Ok(false)
            }
        }
    }

    fn load(&mut self, ctx: &Context<Self>) {
        if self.channel.id() == ws::ChannelId::NONE {
            return;
        }

        self._req = self
            .channel
            .request()
            .body(api::GetEpisodeCacheRequest {
                episode_id: ctx.props().episode_id,
            })
            .on_packet(ctx.link().callback(Msg::Loaded))
            .send();
    }

    fn view_content(&self, ctx: &Context<Self>) -> Html {
        let Some(entries) = self.entries.as_ref() else {
            return html!(<div class="text-muted">{"Loading…"}</div>);
        };

        if entries.is_empty() {
            return html!(<div class="text-muted">{"No cache entries recorded."}</div>);
        }

        let link = ctx.link();

        html! {
            <div class="column">
                { for entries.iter().map(|e| {
                    let source = e.source;
                    let cache = &e.cache;

                    html! {
                        <div class="column">
                            <div class="row-split align-top">
                                <h4>{source.as_label()}</h4>

                                <Button icon="arrow-path" variant={Variant::Danger} title="Clear cache" onclick={link.callback(move |_| Msg::Purge(source))} />
                            </div>

                            <div class="input-group">
                                if let Some(ref etag) = cache.etag {
                                    <span class="input-label has-text">{"ETag"}</span>
                                    <span class="input-text has-text fill">{etag}</span>
                                }

                                if let Some(ref last_updated) = cache.last_updated {
                                    <span class="input-label has-text">{"Last Updated"}</span>
                                    <span class="input-text has-text fill">{last_updated}</span>
                                }

                                if !cache.kinds.is_empty() {
                                    <span class="input-label has-text">{"Kinds"}</span>
                                    {for cache.kinds.iter().map(|k| html!(<span class="input-text has-text">{k.as_label()}</span>))}
                                }
                            </div>

                            if !cache.errors.is_empty() {
                                <h4>{"Errors"}</h4>

                                { for cache.errors.iter().map(|e| html! {
                                    <div class="input-group">
                                        <span class="input-label has-text">{e.kind.as_label()}</span>
                                        <span class="input-text has-text">{&e.key}</span>
                                        <span class="input-text has-text fill">{&e.message}</span>
                                        <span class="input-label has-text">{"Retries"}</span>
                                        <span class="input-text has-text">{e.expires_at().to_string()}</span>
                                    </div>
                                }) }
                            }
                        </div>
                    }
                }) }
            </div>
        }
    }
}
