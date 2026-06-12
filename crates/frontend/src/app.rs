use api::TimeZone;
use jiff::tz::TimeZone as JiffTimeZone;
use musli_web::web03::prelude::*;
use yew::prelude::*;

use crate::error::{CustomContext, Error, Message, RcError};
use crate::router::{DashboardQuery, PagedQuery, Route};
use crate::setup_channel::SetupChannel;
use crate::ui::LoadingPage;
use crate::{
    Dashboard, MovieDetail, MoviesList, Queue, Search, SeriesDetail, SeriesList, Settings,
};

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
                <LoadingPage />
            };
        };

        let link = ctx.link();
        let on_nav = link.callback(Msg::Navigate);

        html! {
            <ContextProvider<TimeZone> context={tz.clone()}>
                <ContextProvider<ws::Handle> context={self.ws.handle()}>
                    <div class="app">
                        <Toolbar on_navigate={on_nav} />

                        { self.view_page(ctx) }
                    </div>
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

        let error = ctx.props().error.clone();
        let onerror = ctx.props().onerror.clone();

        match &ctx.props().route {
            Route::Dashboard(query) => {
                html! { <Dashboard {error} {onerror} {on_navigate} page={query.page} /> }
            }
            Route::Queue => html! { <Queue {error} {onerror} {on_navigate} /> },
            Route::Series(query) => html! {
                <SeriesList {error} {onerror} {on_navigate} page={query.page} filter={query.filter.clone()} />
            },
            Route::SeriesDetail(series_id, query) => {
                let series_id = *series_id;
                let initial_season = query.season;

                html! {
                    <SeriesDetail {error} {onerror} {series_id} {initial_season} {on_navigate} />
                }
            }
            Route::Movies(query) => html! {
                <MoviesList {error} {onerror} {on_navigate} page={query.page} filter={query.filter.clone()} />
            },
            Route::MovieDetail(movie_id) => {
                let movie_id = *movie_id;
                html! { <MovieDetail {error} {onerror} {movie_id} {on_navigate} /> }
            }
            Route::Search => html! { <Search {error} {onerror} {on_navigate} /> },
            Route::Settings => html! { <Settings {error} {onerror} /> },
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
            <div class="toolbar-inner desktop-row-fill mobile-column">
                <div class="row-fill">
                    <div class="row">
                        <span class="site-title clickable" onclick={on_nav(Route::Dashboard(DashboardQuery::default()))}>{"Track"}</span>
                    </div>

                    <div class="row end hide-desktop">
                        <button class="btn" onclick={on_menu_toggle} title="Settings">
                            <span class="item-inline"><span class={classes!("icon", if *menu_open { "ellipsis-horizontal" } else { "bars-3" })} /></span>
                        </button>
                    </div>
                </div>

                <div class={classes!("desktop-row", "mobile-column", "end", (!*menu_open).then_some("hide-mobile"))}>
                    <button onclick={on_nav(Route::Dashboard(DashboardQuery::default()))} class="btn" title="Dashboard">
                        <span class="item-inline"><span class="icon rectangle-stack" /></span>
                        <span>{"Dashboard"}</span>
                    </button>
                    <button onclick={on_nav(Route::Queue)} class="btn" title="Queue">
                        <span class="item-inline"><span class="icon queue-list" /></span>
                        <span>{"Queue"}</span>
                    </button>
                    <button onclick={on_nav(Route::Series(PagedQuery::default()))} class="btn" title="Series">
                        <span class="item-inline"><span class="icon tv" /></span>
                        <span>{"Series"}</span>
                    </button>
                    <button onclick={on_nav(Route::Movies(PagedQuery::default()))} class="btn" title="Movies">
                        <span class="item-inline"><span class="icon film" /></span>
                        <span>{"Movies"}</span>
                    </button>
                    <button onclick={on_nav(Route::Search)} class="btn" title="Search">
                        <span class="item-inline"><span class="icon magnifying-glass" /></span>
                        <span>{"Search"}</span>
                    </button>
                    <button onclick={on_nav(Route::Settings)} class="btn" title="Settings">
                        <span class="item-inline"><span class="icon cog-6-tooth" /></span>
                        <span>{"Settings"}</span>
                    </button>
                </div>
            </div>
        </div>
    }
}
