use api::TimeZone;
use jiff::tz::TimeZone as JiffTimeZone;
use musli_web::web03::prelude::*;
use yew::prelude::*;

use crate::background::Background;
use crate::error::{CustomContext, Error, Message, RcError};
use crate::router::{DashboardQuery, PagedQuery, Route, SearchQuery};
use crate::setup_channel::SetupChannel;
use crate::ui::{ErrorBox, Loading};
use crate::{Dashboard, MovieDetail, MoviesList, Queue, Search, Settings, ShowDetail, ShowList};

pub(super) struct App {
    channel: ws::Channel,
    ws: ws::Service,
    tz: Option<TimeZone>,
    _setup: SetupChannel,
    _broadcast: ws::Listener,
    _config_req: ws::Request,
}

pub(super) enum Msg {
    Channel(Result<ws::Channel, ws::Error>),
    AppBroadcast(Result<ws::Packet<api::AppBroadcast>, ws::Error>),
    ConfigLoaded(Result<ws::Packet<api::GetConfig>, ws::Error>),
    WsError(ws::Error),
    Navigate(Route),
}

#[derive(Properties, PartialEq)]
pub(super) struct Props {
    pub(super) error: Option<RcError>,
    pub(super) onerror: Callback<Option<Error>>,
    pub(super) route: Route,
    pub(super) on_navigate: Callback<Route>,
    pub(super) on_background: Callback<String>,
    pub(super) on_title: Callback<Option<String>>,
}

impl Component for App {
    type Message = Msg;
    type Properties = Props;

    fn create(ctx: &Context<Self>) -> Self {
        let ws = ws::connect(ws::Connect::location("/ws"))
            .close_before_unload()
            .on_error(ctx.link().callback(Msg::WsError))
            .build();

        let _setup = SetupChannel::new(ws.handle().clone(), ctx.link().callback(Msg::Channel));

        let _broadcast = ws
            .handle()
            .clone()
            .on_broadcast(ctx.link().callback(Msg::AppBroadcast));

        Self {
            channel: ws::Channel::default(),
            ws,
            tz: None,
            _setup,
            _broadcast,
            _config_req: ws::Request::default(),
        }
    }

    fn update(&mut self, ctx: &Context<Self>, msg: Self::Message) -> bool {
        match self.try_update(ctx, msg) {
            Ok(render) => render,
            Err(e) => {
                ctx.props().onerror.emit(Some(e));
                false
            }
        }
    }

    fn view(&self, ctx: &Context<Self>) -> Html {
        let Some(tz) = &self.tz else {
            return html! {
                <div class="page">
                    <Loading />
                </div>
            };
        };

        let link = ctx.link();
        let on_nav = link.callback(Msg::Navigate);
        let background = Background::new(
            ctx.props().on_background.clone(),
            ctx.props().on_title.clone(),
        );

        html! {
            <ContextProvider<TimeZone> context={tz.clone()}>
                <ContextProvider<ws::Handle> context={self.ws.handle()}>
                    <ContextProvider<Background> context={background}>
                        <div id="application">
                            <Toolbar on_navigate={on_nav} />

                            <div class="page">
                                if let Some(error) = &ctx.props().error {
                                    <ErrorBox error={error.clone()} onclearerror={ctx.props().onerror.reform(|()| None)} />
                                }

                                { self.view_page(ctx) }
                            </div>
                        </div>
                    </ContextProvider<Background>>
                </ContextProvider<ws::Handle>>
            </ContextProvider<TimeZone>>
        }
    }
}

impl App {
    fn try_update(&mut self, ctx: &Context<Self>, msg: Msg) -> Result<bool, Error> {
        match msg {
            Msg::Channel(result) => {
                self.channel = result?;

                if self.channel.id() != ws::ChannelId::NONE {
                    self._config_req = self
                        .channel
                        .request()
                        .body(api::GetConfigRequest)
                        .on_packet(ctx.link().callback(Msg::ConfigLoaded))
                        .send();
                }

                Ok(true)
            }
            Msg::AppBroadcast(result) => {
                let event = result?.decode_event()?;

                if let api::AppEventKind::ConfigChanged { config } = event.kind {
                    let new_tz = Self::tz_from_config(&config);

                    if Some(&new_tz) != self.tz.as_ref() {
                        self.tz = Some(new_tz);
                        return Ok(true);
                    }
                }

                Ok(false)
            }
            Msg::ConfigLoaded(result) => {
                let config = result
                    .context(Message::LoadingConfig)?
                    .decode()
                    .context(Message::LoadingConfig)?
                    .config;

                let new_tz = Self::tz_from_config(&config);

                if Some(&new_tz) != self.tz.as_ref() {
                    self.tz = Some(new_tz);
                    return Ok(true);
                }

                Ok(false)
            }
            Msg::WsError(e) => Err(e.into()),
            Msg::Navigate(route) => {
                ctx.props().on_navigate.emit(route);
                Ok(false)
            }
        }
    }

    fn tz_from_config(config: &api::Config) -> TimeZone {
        if !config.timezone.is_empty()
            && let Ok(tz) = JiffTimeZone::get(&config.timezone)
        {
            return TimeZone::from_jiff(tz);
        }

        TimeZone::from_jiff(JiffTimeZone::system())
    }

    fn view_page(&self, ctx: &Context<Self>) -> Html {
        let on_navigate = ctx.link().callback(Msg::Navigate);

        let onerror = ctx.props().onerror.clone();

        match &ctx.props().route {
            Route::Dashboard(query) => {
                html! { <Dashboard {onerror} {on_navigate} page={query.page} /> }
            }
            Route::Queue => html! { <Queue {onerror} {on_navigate} /> },
            Route::Shows(query) => html! {
                <ShowList {onerror} {on_navigate} page={query.page} filter={query.filter.clone()} />
            },
            Route::ShowDetail(show_id, query) => {
                let show_id = *show_id;
                let initial_season = query.season;

                html! {
                    <ShowDetail {onerror} {show_id} {initial_season} {on_navigate} />
                }
            }
            Route::Movies(query) => html! {
                <MoviesList {onerror} {on_navigate} page={query.page} filter={query.filter.clone()} />
            },
            Route::MovieDetail(movie_id) => {
                let movie_id = *movie_id;
                html! { <MovieDetail {onerror} {movie_id} {on_navigate} /> }
            }
            Route::Search(query) => html! {
                <Search {onerror} {on_navigate} kind={query.kind} filter={query.filter.clone()} />
            },
            Route::Settings => html! { <Settings {onerror} /> },
        }
    }
}

#[derive(Properties, PartialEq)]
struct ToolbarProps {
    on_navigate: Callback<Route>,
}

#[function_component]
fn Toolbar(props: &ToolbarProps) -> Html {
    let menu_open = use_state(|| false);

    let on_menu_toggle = {
        let menu_open = menu_open.clone();
        Callback::from(move |_| menu_open.set(!*menu_open))
    };

    let on_nav = {
        |route: Route| {
            props.on_navigate.reform({
                let menu_open = menu_open.clone();

                move |_| {
                    menu_open.set(false);
                    route.clone()
                }
            })
        }
    };

    html! {
        <div class="toolbar">
            <div class="toolbar-brand">
                <span class="site-title clickable" onclick={on_nav(Route::Dashboard(DashboardQuery::default()))}>{"Track"}</span>
            </div>

            <div class="toolbar-toggle" onclick={on_menu_toggle} title="Navigation">
                <span class={classes!("icon", if *menu_open { "ellipsis-horizontal" } else { "bars-3" })} />
            </div>

            <div class={classes!("toolbar-dropdown", (!*menu_open).then_some("hide-mobile"))}>
                <button onclick={on_nav(Route::Dashboard(DashboardQuery::default()))} class="toolbar-item" title="Dashboard">
                    <span class="icon rectangle-stack" />
                    <span>{"Dashboard"}</span>
                </button>
                <button onclick={on_nav(Route::Shows(PagedQuery::default()))} class="toolbar-item" title="Shows">
                    <span class="icon tv" />
                    <span>{"Shows"}</span>
                </button>
                <button onclick={on_nav(Route::Movies(PagedQuery::default()))} class="toolbar-item" title="Movies">
                    <span class="icon film" />
                    <span>{"Movies"}</span>
                </button>
                <button onclick={on_nav(Route::Search(SearchQuery::default()))} class="toolbar-item" title="Search">
                    <span class="icon magnifying-glass" />
                    <span class="hide-desktop">{"Search"}</span>
                </button>
                <button onclick={on_nav(Route::Queue)} class="toolbar-item" title="Queue">
                    <span class="icon queue-list" />
                    <span class="hide-desktop">{"Queue"}</span>
                </button>
                <button onclick={on_nav(Route::Settings)} class="toolbar-item" title="Settings">
                    <span class="icon cog-6-tooth" />
                    <span class="hide-desktop">{"Settings"}</span>
                </button>
            </div>
        </div>
    }
}
