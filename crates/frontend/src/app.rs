use musli_web::web03::prelude::*;
use yew::prelude::*;

use crate::Dashboard;
use crate::MovieDetail;
use crate::MoviesList;
use crate::Queue;
use crate::Search;
use crate::SeriesDetail;
use crate::SeriesList;
use crate::Settings;
use crate::WatchNext;
use crate::error::Error;
use crate::router::Route;
use crate::setup_channel::SetupChannel;

pub(super) struct App {
    channel: ws::Channel,
    ws: ws::Service,
    _setup: SetupChannel,
    _broadcast: ws::Listener,
}

pub(super) enum Msg {
    Channel(Result<ws::Channel, ws::Error>),
    AppBroadcast(Result<ws::Packet<api::AppBroadcast>, ws::Error>),
    WsError(ws::Error),
    Navigate(Route),
}

#[derive(Properties, PartialEq)]
pub(super) struct Props {
    pub(super) route: Route,
    pub(super) onerror: Callback<Error>,
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
            _setup,
            _broadcast,
        }
    }

    fn update(&mut self, ctx: &Context<Self>, msg: Self::Message) -> bool {
        match msg {
            Msg::Channel(result) => {
                match result {
                    Ok(ch) => self.channel = ch,
                    Err(e) => ctx.props().onerror.emit(e.into()),
                }
                true
            }
            Msg::AppBroadcast(_) => false,
            Msg::WsError(e) => {
                ctx.props().onerror.emit(e.into());
                false
            }
            Msg::Navigate(route) => {
                ctx.props().on_navigate.emit(route);
                false
            }
        }
    }

    fn view(&self, ctx: &Context<Self>) -> Html {
        let link = ctx.link();
        let on_nav = |route: Route| link.callback(move |_| Msg::Navigate(route.clone()));

        html! {
            <ContextProvider<ws::Handle> context={self.ws.handle()}>
            <div class="app">
                <div class="toolbar row-fill">
                    <div class="row start">
                        <span class="app-title">{"OnTV"}</span>
                    </div>
                    <div class="row end">
                        <button onclick={on_nav(Route::Dashboard)} class="btn" title="Dashboard">
                            <span class="icon-inline"><span class="icon rectangle-stack" /></span>
                            <span class="hide-mobile">{"Dashboard"}</span>
                        </button>
                        <button onclick={on_nav(Route::Queue)} class="btn" title="Queue">
                            <span class="icon-inline"><span class="icon queue-list" /></span>
                            <span class="hide-mobile">{"Queue"}</span>
                        </button>
                        <button onclick={on_nav(Route::WatchNext)} class="btn" title="Watch Next">
                            <span class="icon-inline"><span class="icon play" /></span>
                            <span class="hide-mobile">{"Watch Next"}</span>
                        </button>
                        <button onclick={on_nav(Route::Series)} class="btn" title="Series">
                            <span class="icon-inline"><span class="icon tv" /></span>
                            <span class="hide-mobile">{"Series"}</span>
                        </button>
                        <button onclick={on_nav(Route::Movies)} class="btn" title="Movies">
                            <span class="icon-inline"><span class="icon film" /></span>
                            <span class="hide-mobile">{"Movies"}</span>
                        </button>
                        <button onclick={on_nav(Route::Search)} class="btn" title="Search">
                            <span class="icon-inline"><span class="icon magnifying-glass" /></span>
                            <span class="hide-mobile">{"Search"}</span>
                        </button>
                        <button onclick={on_nav(Route::Settings)} class="btn" title="Settings">
                            <span class="icon-inline"><span class="icon cog-6-tooth" /></span>
                            <span class="hide-mobile">{"Settings"}</span>
                        </button>
                    </div>
                </div>
                <div class="app-body">
                    { self.view_page(ctx) }
                </div>
            </div>
            </ContextProvider<ws::Handle>>
        }
    }
}

impl App {
    fn view_page(&self, ctx: &Context<Self>) -> Html {
        let onerror = ctx.props().onerror.clone();
        let on_navigate = ctx.link().callback(Msg::Navigate);
        match &ctx.props().route {
            Route::Dashboard => html! { <Dashboard {onerror} {on_navigate} /> },
            Route::Queue => html! { <Queue {onerror} {on_navigate} /> },
            Route::WatchNext => html! { <WatchNext {onerror} {on_navigate} /> },
            Route::Series => html! {
                <SeriesList {onerror} {on_navigate} />
            },
            Route::SeriesDetail(series_id) => {
                let series_id = *series_id;
                html! {
                    <SeriesDetail {series_id} {onerror} {on_navigate} />
                }
            }
            Route::Movies => html! { <MoviesList {onerror} {on_navigate} /> },
            Route::MovieDetail(movie_id) => {
                let movie_id = *movie_id;
                html! { <MovieDetail {movie_id} {onerror} {on_navigate} /> }
            }
            Route::Search => html! { <Search {onerror} {on_navigate} /> },
            Route::Settings => html! { <Settings {onerror} /> },
        }
    }
}
