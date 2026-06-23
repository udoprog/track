use musli_web::web03::prelude::*;
use yew::prelude::*;

use crate::SetupChannel;
use crate::error::{CustomContext, Error, Message};

use super::Modal;

/// Fields are shown in this order, each as its own section listing every
/// language's value for that field.
const FIELDS: [(api::StringKind, &str); 2] = [
    (api::StringKind::Title, "Title"),
    (api::StringKind::Overview, "Overview"),
];

#[derive(Properties, PartialEq)]
pub(crate) struct Props {
    /// The entity whose translations are displayed.
    pub(crate) target: api::TranslationTarget,
    pub(crate) onerror: Callback<Error>,
    pub(crate) on_close: Callback<()>,
}

pub(crate) enum Msg {
    Channel(Result<ws::Channel, ws::Error>),
    AppBroadcast(Result<ws::Packet<api::AppBroadcast>, ws::Error>),
    Loaded(Result<ws::Packet<api::GetTranslations>, ws::Error>),
}

pub(crate) struct TranslationsModal {
    channel: ws::Channel,
    _setup: SetupChannel,
    _broadcast: ws::Listener,
    _req: ws::Request,
    /// `None` while loading, `Some` once the response has arrived.
    translations: Option<Vec<api::Translation>>,
}

impl Component for TranslationsModal {
    type Message = Msg;
    type Properties = Props;

    fn create(ctx: &Context<Self>) -> Self {
        let (ws, _) = ctx
            .link()
            .context::<ws::Handle>(Callback::noop())
            .expect("Expected ws::Handle in context");

        let _setup = SetupChannel::new(ws.clone(), ctx.link().callback(Msg::Channel));
        let _broadcast = ws.on_broadcast(ctx.link().callback(Msg::AppBroadcast));

        Self {
            channel: ws::Channel::default(),
            _setup,
            _broadcast,
            _req: ws::Request::default(),
            translations: None,
        }
    }

    fn update(&mut self, ctx: &Context<Self>, msg: Self::Message) -> bool {
        match self.try_update(ctx, msg) {
            Ok(render) => render,
            Err(e) => {
                ctx.props().onerror.emit(e);
                false
            }
        }
    }

    fn view(&self, ctx: &Context<Self>) -> Html {
        let on_close = ctx.props().on_close.clone();

        html! {
            <Modal icon="language" title="Translations" on_close={on_close}>
                { self.view_content() }
            </Modal>
        }
    }
}

impl TranslationsModal {
    fn try_update(&mut self, ctx: &Context<Self>, msg: Msg) -> Result<bool, Error> {
        match msg {
            Msg::Channel(result) => {
                self.channel = result?;
                self.load(ctx);
                Ok(true)
            }
            Msg::AppBroadcast(packet) => {
                let event = packet?.decode_event()?;

                // Sync broadcasts originate from another channel; ignore our own.
                if event.channel == self.channel.id() {
                    return Ok(false);
                }

                if let api::AppEventKind::TranslationsChanged { target } = &event.kind
                    && covers(*target, ctx.props().target)
                {
                    self.load(ctx);
                }

                Ok(false)
            }
            Msg::Loaded(result) => {
                let translations = result
                    .context(Message::LoadingTranslations)?
                    .decode()
                    .context(Message::LoadingTranslations)?
                    .translations;

                self.translations = Some(translations);
                Ok(true)
            }
        }
    }

    /// Request the translations for this modal's target on the current channel.
    fn load(&mut self, ctx: &Context<Self>) {
        if self.channel.id() == ws::ChannelId::NONE {
            return;
        }

        self._req = self
            .channel
            .request()
            .body(api::GetTranslationsRequest {
                target: ctx.props().target,
            })
            .on_packet(ctx.link().callback(Msg::Loaded))
            .send();
    }

    fn view_content(&self) -> Html {
        let Some(ref translations) = self.translations else {
            return html!(<div class="text-muted">{"Loading…"}</div>);
        };

        if translations.is_empty() {
            return html!(<div class="text-muted">{"No translations available."}</div>);
        }

        html! {
            <div class="column">
                { for FIELDS.iter().filter_map(|&(kind, label)| {
                    let rows: Vec<&api::Translation> =
                        translations.iter().filter(|t| t.kind == kind).collect();

                    if rows.is_empty() {
                        return None;
                    }

                    Some(html! {
                        <div class="column">
                            <h3>{label}</h3>

                            { for rows.into_iter().map(view_row) }
                        </div>
                    })
                }) }
            </div>
        }
    }
}

/// Whether a `TranslationsChanged` event for `event` should refresh a modal
/// showing `mine`. Sync rewrites a show's seasons and episodes together but the
/// event is emitted at show level, so any show-level change refreshes the leaf
/// season/episode views as well.
fn covers(event: api::TranslationTarget, mine: api::TranslationTarget) -> bool {
    use api::TranslationTarget::{Episode, Movie, Season, Show};

    match (event, mine) {
        (Show(a), Show(b)) => a == b,
        (Movie(a), Movie(b)) => a == b,
        (Show(_), Season(_) | Episode(_)) => true,
        _ => false,
    }
}

/// Render a single translation as a row: language name with its flag (mirroring
/// the `LanguagePicker` treatment, flag wrapped in `item-inline` so it sizes,
/// falling back to the 3-letter code when no flag exists) followed by the text.
fn view_row(translation: &api::Translation) -> Html {
    let (label, flag) = super::locale_label(translation.language, "Default Language");

    html! {
        <div class="column">
            <div class="row text-gap">
                if let Some(flag) = flag {
                    <span class={classes!("item-inline", "flag", flag)} title={translation.language} />
                } else {
                    <span class="item-inline">
                        <span class="text-muted">{translation.language}</span>
                    </span>
                }

                <span>{label}</span>
            </div>

            <div>{&translation.text}</div>
        </div>
    }
}
