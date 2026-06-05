use musli_web::web03::prelude::*;
use yew::prelude::*;

use crate::error::{CustomContext, Error, Message};
use crate::router::Route;

pub(super) struct SeriesList {
    channel: ws::Channel,
    series: Vec<api::Series>,
    _setup: crate::SetupChannel,
    _broadcast: ws::Listener,
    _list_req: ws::Request,
}

pub(super) enum Msg {
    Channel(Result<ws::Channel, ws::Error>),
    AppBroadcast(Result<ws::Packet<api::AppBroadcast>, ws::Error>),
    SeriesLoaded(Result<ws::Packet<api::ListSeries>, ws::Error>),
    Navigate(Route),
}

#[derive(Properties, PartialEq)]
pub(super) struct Props {
    pub(super) onerror: Callback<Error>,
    pub(super) on_navigate: Callback<Route>,
}

impl Component for SeriesList {
    type Message = Msg;
    type Properties = Props;

    fn create(ctx: &Context<Self>) -> Self {
        let (ws, _) = ctx
            .link()
            .context::<ws::Handle>(Callback::noop())
            .expect("ws::Handle context not found");

        let _setup = crate::SetupChannel::new(ws.clone(), ctx.link().callback(Msg::Channel));
        let _broadcast = ws.on_broadcast(ctx.link().callback(Msg::AppBroadcast));

        Self {
            channel: ws::Channel::default(),
            series: Vec::new(),
            _setup,
            _broadcast,
            _list_req: ws::Request::default(),
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
        html! {
            <div class="outline">
                <div class="outline-title">{"Series"}</div>
                if self.series.is_empty() {
                    <div class="empty text-muted">{"No series tracked."}</div>
                } else {
                    { for self.series.iter().map(|s| self.view_row(ctx, s)) }
                }
            </div>
        }
    }
}

impl SeriesList {
    fn try_update(&mut self, ctx: &Context<Self>, msg: Msg) -> Result<bool, Error> {
        match msg {
            Msg::Channel(result) => {
                self.channel = result?;
                if self.channel.id() != ws::ChannelId::NONE {
                    self.load(ctx);
                } else {
                    self.series.clear();
                }
                Ok(true)
            }
            Msg::AppBroadcast(packet) => {
                let event = packet?.decode_event()?;
                if event.channel == self.channel.id() {
                    return Ok(false);
                }
                match event.kind {
                    api::AppEventKind::SeriesCreated { .. }
                    | api::AppEventKind::SeriesChanged { .. }
                    | api::AppEventKind::SeriesDeleted { .. } => {
                        if self.channel.id() != ws::ChannelId::NONE {
                            self.load(ctx);
                        }
                        Ok(false)
                    }
                    _ => Ok(false),
                }
            }
            Msg::SeriesLoaded(result) => {
                self.series = result
                    .context(Message::LoadingSeries)?
                    .decode()
                    .context(Message::LoadingSeries)?
                    .series;
                Ok(true)
            }
            Msg::Navigate(route) => {
                ctx.props().on_navigate.emit(route);
                Ok(false)
            }
        }
    }

    fn load(&mut self, ctx: &Context<Self>) {
        self._list_req = self
            .channel
            .request()
            .body(api::ListSeriesRequest)
            .on_packet(ctx.link().callback(Msg::SeriesLoaded))
            .send();
    }

    fn view_row(&self, ctx: &Context<Self>, s: &api::Series) -> Html {
        let id = s.id;
        let onclick = ctx
            .link()
            .callback(move |_| Msg::Navigate(Route::SeriesDetail(id)));

        html! {
            <div class="group row clickable" {onclick}>
                if let Some(ref poster) = s.poster {
                    <img class="poster-sm" src={poster.proxy_url()} alt="" />
                } else {
                    <div class="poster-sm" />
                }
                <span class="fill">{&s.title}</span>
                if let Some(date) = s.first_air_date {
                    <span class="text-muted">{date.year().to_string()}</span>
                }
                if !s.tracked {
                    <span class="license">{"Untracked"}</span>
                }
                <span class="icon-inline"><span class="icon chevron-right" /></span>
            </div>
        }
    }
}
