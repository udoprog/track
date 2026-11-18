use auth::yew023::{AuthContext, AuthSubscription};
use musli_web::web03::prelude::*;
use musli_web::yew023::ContextExt;
use yew::prelude::*;

use crate::background::Background;
use crate::class;
use crate::error::{CustomContext, Error, Message};

use super::{Button, Modal, Variant};

#[derive(Properties, PartialEq)]
pub(crate) struct Props {
    pub(crate) episode_id: api::EpisodeId,
    pub(crate) title: AttrValue,
    pub(crate) on_close: Callback<()>,
}

pub(crate) enum Msg {
    Auth(AuthContext),
    Channel(ws::ChannelEvent),
    Loaded(Result<ws::Packet<api::GetEpisodeCache>, ws::Error>),
    Purge(api::RemoteSource),
    PurgeDone(Result<ws::Packet<api::PurgeEpisodeCache>, ws::Error>),
}

/// The per-source conditional-request state stored for one episode, with a per-source
/// clear. The show-level equivalent lives in the remote editor; an episode is not
/// addressed by a remote id, so its entries are keyed by source alone.
pub(crate) struct EpisodeCacheModal {
    auth: AuthSubscription,
    channel: ws::Channel,
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
        let ws = ctx.handle();

        let (background, _) = ctx
            .link()
            .context::<Background>(Callback::noop())
            .expect("Expected Background in context");

        let channel = ws
            .channel()
            .on_change(ctx.link().callback(Msg::Channel))
            .open();

        Self {
            auth: AuthSubscription::new(ctx, Msg::Auth),
            channel,
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
            <Modal icon={class::ICON_CIRCLE_STACK} title={title} on_close={on_close}>
                { self.view_content(ctx) }
            </Modal>
        }
    }
}

impl EpisodeCacheModal {
    fn try_update(&mut self, ctx: &Context<Self>, msg: Msg) -> Result<bool, Error> {
        match msg {
            Msg::Auth(auth) => Ok(self.auth.set(auth)),
            Msg::Channel(event) => {
                if let ws::ChannelEvent::Failed(error) = event {
                    return Err(error.into());
                }
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
                if !self.channel.is_open() {
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
        if !self.channel.is_open() {
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
            return html!(<div class={class::TEXT_MUTED}>{"Loading…"}</div>);
        };

        if entries.is_empty() {
            return html!(<div class={class::TEXT_MUTED}>{"No cache entries recorded."}</div>);
        }

        let link = ctx.link();
        let admin = self.auth.is_admin();

        html! {
            <div class={sockel::class::STACK}>
                { for entries.iter().map(|e| {
                    let source = e.source;
                    let cache = &e.cache;

                    html! {
                        <div class={sockel::class::STACK}>
                            <div class={classes!(sockel::class::CLUSTER, sockel::class::BETWEEN, sockel::class::LOOSE, sockel::class::TOP)}>
                                <h4>{source.as_label()}</h4>

                                if admin {
                                    <Button icon={class::ICON_ARROW_PATH} variant={Variant::Danger} title="Clear cache" onclick={link.callback(move |_| Msg::Purge(source))} />
                                }
                            </div>

                            <div class={class::INPUT_GROUP}>
                                if let Some(ref etag) = cache.etag {
                                    <span class={classes!(class::INPUT_LABEL, class::HAS_TEXT)}>{"ETag"}</span>
                                    <span class={classes!(sockel::class::INPUT_BOX, class::INPUT_VALUE, class::FILL)}>{etag}</span>
                                }

                                if let Some(ref last_updated) = cache.last_updated {
                                    <span class={classes!(class::INPUT_LABEL, class::HAS_TEXT)}>{"Last Updated"}</span>
                                    <span class={classes!(sockel::class::INPUT_BOX, class::INPUT_VALUE, class::FILL)}>{last_updated}</span>
                                }

                                if !cache.kinds.is_empty() {
                                    <span class={classes!(class::INPUT_LABEL, class::HAS_TEXT)}>{"Kinds"}</span>
                                    {for cache.kinds.iter().map(|k| html!(<span class={classes!(sockel::class::INPUT_BOX, class::INPUT_VALUE)}>{k.as_label()}</span>))}
                                }
                            </div>

                            if !cache.errors.is_empty() {
                                <h4>{"Errors"}</h4>

                                { for cache.errors.iter().map(|e| html! {
                                    <div class={class::INPUT_GROUP}>
                                        <span class={classes!(class::INPUT_LABEL, class::HAS_TEXT)}>{e.kind.as_label()}</span>
                                        <span class={classes!(sockel::class::INPUT_BOX, class::INPUT_VALUE)}>{&e.key}</span>
                                        <span class={classes!(sockel::class::INPUT_BOX, class::INPUT_VALUE, class::FILL)}>{&e.message}</span>
                                        <span class={classes!(class::INPUT_LABEL, class::HAS_TEXT)}>{"Retries"}</span>
                                        <span class={classes!(sockel::class::INPUT_BOX, class::INPUT_VALUE)}>{e.expires_at().to_string()}</span>
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
