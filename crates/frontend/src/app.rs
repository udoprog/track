use std::rc::Rc;

use api::{TimeInfo, TimeZone, Timestamp};
use gloo::timers::callback::Interval;
use musli_web::web03::prelude::*;
use yew::prelude::*;

use crate::background::Background;
use crate::error::{CustomContext, Error, Message, RcError};
use crate::page::{Dashboard, MediaList, MovieDetail, Queue, Search, Settings, ShowDetail};
use crate::router::{DashboardQuery, MediaQuery, QueueQuery, Route, Router, SearchQuery};
use crate::setup_channel::SetupChannel;
use crate::ui::{ErrorBox, Outline, OutlineControl, OutlineEntry, TopLanguages};

pub(super) struct App {
    channel: ws::Channel,
    ws: ws::Service,
    time: TimeInfo,
    top_languages: TopLanguages,
    /// Scroll container the outline reflects and drives; passed to [`Outline`].
    page: NodeRef,
    /// Entries currently shown in the outline, pushed in by a consumer through
    /// [`OutlineControl`] and forwarded to [`Outline`]. `None` hides it.
    outline_entries: Option<Rc<[OutlineEntry]>>,
    /// Control handed to consumers via context.
    outline_control: OutlineControl,
    _setup: SetupChannel,
    _broadcast: ws::Listener,
    _config_req: ws::Request,
    _top_languages_req: ws::Request,
    _tick_minute_interval: Interval,
}

pub(super) enum Msg {
    Channel(Result<ws::Channel, ws::Error>),
    AppBroadcast(Result<ws::Packet<api::AppBroadcast>, ws::Error>),
    TickTime,
    ConfigLoaded(Result<ws::Packet<api::GetConfig>, ws::Error>),
    TopLanguagesLoaded(Result<ws::Packet<api::GetTopLanguages>, ws::Error>),
    /// A consumer set (or cleared) the outline contents.
    SetOutline(Option<Rc<[OutlineEntry]>>),
    WsError(ws::Error),
}

#[derive(Properties, PartialEq)]
pub(super) struct Props {
    pub(super) error: Option<RcError>,
    pub(super) onerror: Callback<Error>,
    pub(super) onclearerror: Callback<()>,
    pub(super) route: Route,
    pub(super) on_navigate: Callback<Route>,
    pub(super) on_replace: Callback<Route>,
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

        let outline_control = OutlineControl::new(ctx.link().callback(Msg::SetOutline));

        let link = ctx.link().clone();
        let _tick_minute_interval = Interval::new(10_000, move || link.send_message(Msg::TickTime));

        Self {
            channel: ws::Channel::default(),
            ws,
            time: TimeInfo::new(TimeZone::system(), Timestamp::now()),
            top_languages: TopLanguages::default(),
            page: NodeRef::default(),
            outline_entries: None,
            outline_control,
            _setup,
            _broadcast,
            _config_req: ws::Request::default(),
            _top_languages_req: ws::Request::default(),
            _tick_minute_interval,
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
        let router = Router::new(
            ctx.props().on_navigate.clone(),
            ctx.props().on_replace.clone(),
        );

        let background = Background::new(
            ctx.props().on_background.clone(),
            ctx.props().on_title.clone(),
        );

        html! {
            <ContextProvider<ws::Handle> context={self.ws.handle()}>
            <ContextProvider<TimeInfo> context={self.time.clone()}>
            <ContextProvider<TopLanguages> context={self.top_languages.clone()}>
            <ContextProvider<Router> context={router}>
            <ContextProvider<Background> context={background}>
            <ContextProvider<OutlineControl> context={self.outline_control.clone()}>
                <div id="application">
                    if let Some(error) = &ctx.props().error {
                        <div id="error">
                            <ErrorBox error={error.clone()} onclearerror={ctx.props().onclearerror.clone()} />
                        </div>
                    }

                    <Toolbar />

                    <div id="content">
                        <div id="page" ref={self.page.clone()}>
                            { self.view_page(ctx) }
                        </div>

                        <Outline
                            page={self.page.clone()}
                            entries={self.outline_entries.clone()}
                            onerror={ctx.props().onerror.clone()}
                        />
                    </div>
                </div>
            </ContextProvider<OutlineControl>>
            </ContextProvider<Background>>
            </ContextProvider<Router>>
            </ContextProvider<TopLanguages>>
            </ContextProvider<TimeInfo>>
            </ContextProvider<ws::Handle>>
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

                    self._top_languages_req = self
                        .channel
                        .request()
                        .body(api::GetTopLanguagesRequest)
                        .on_packet(ctx.link().callback(Msg::TopLanguagesLoaded))
                        .send();
                }

                Ok(true)
            }
            Msg::AppBroadcast(result) => {
                let event = result?.decode_event()?;

                match event.kind {
                    api::AppEventKind::ConfigChanged { config } => {
                        let tz = Self::tz_from_config(&config);

                        if *self.time.tz() != tz {
                            self.time = TimeInfo::new(tz, self.time.now());
                            return Ok(true);
                        }
                    }
                    api::AppEventKind::TopLanguagesChanged { top_languages } => {
                        let next = TopLanguages(top_languages);

                        if next != self.top_languages {
                            self.top_languages = next;
                            return Ok(true);
                        }
                    }
                    _ => {}
                }

                Ok(false)
            }
            Msg::TickTime => {
                self.time = TimeInfo::new(self.time.tz().clone(), Timestamp::now());
                Ok(true)
            }
            Msg::TopLanguagesLoaded(result) => {
                let top_languages = result?.decode()?.top_languages;
                let next = TopLanguages(top_languages);

                if next != self.top_languages {
                    self.top_languages = next;
                    return Ok(true);
                }

                Ok(false)
            }
            Msg::SetOutline(entries) => {
                if self.outline_entries == entries {
                    return Ok(false);
                }

                self.outline_entries = entries;
                Ok(true)
            }
            Msg::ConfigLoaded(result) => {
                let config = result
                    .context(Message::LoadingConfig)?
                    .decode()
                    .context(Message::LoadingConfig)?
                    .config;

                let new_tz = Self::tz_from_config(&config);

                if *self.time.tz() != new_tz {
                    self.time = TimeInfo::new(new_tz, self.time.now());
                    return Ok(true);
                }

                Ok(false)
            }
            Msg::WsError(e) => Err(e.into()),
        }
    }

    fn tz_from_config(config: &api::Config) -> TimeZone {
        if !config.timezone.is_empty()
            && let Some(tz) = TimeZone::get(&config.timezone)
        {
            return tz;
        }

        TimeZone::system()
    }

    fn view_page(&self, ctx: &Context<Self>) -> Html {
        let onerror = ctx.props().onerror.clone();

        match ctx.props().route {
            Route::Dashboard(ref q) => {
                html! { <Dashboard {onerror} page={q.page} /> }
            }
            Route::Queue(ref q) => html! {
                <Queue {onerror} focus={q.focus} page={q.page} />
            },
            Route::Media(ref q) => html! {
                <MediaList
                    {onerror}
                    page={q.page}
                    filter={q.filter.clone()}
                    sort={q.sort}
                    desc={q.desc}
                    tracked={q.tracked}
                    selection={q.selection}
                />
            },
            Route::ShowDetail(show_id, ref q) => {
                let season = q.season.unwrap_or(api::SeasonNumber::FIRST);

                html! {
                    <ShowDetail {onerror} {show_id} {season} />
                }
            }
            Route::MovieDetail(movie_id) => {
                html! { <MovieDetail {onerror} {movie_id} /> }
            }
            Route::Search(ref q) => html! {
                <Search {onerror} selection={q.selection} filter={q.filter.clone()} />
            },
            Route::Settings => html! { <Settings {onerror} /> },
        }
    }
}

#[function_component]
fn Toolbar() -> Html {
    let menu_open = use_state(|| false);

    let router = use_context::<Router>().expect("Expected router in context");

    let on_menu_toggle = {
        let menu_open = menu_open.clone();
        Callback::from(move |_| menu_open.set(!*menu_open))
    };

    let on_nav = {
        |route: Route| {
            let router = router.clone();
            let menu_open = menu_open.clone();

            Callback::from(move |_| {
                menu_open.set(false);
                router.push(route.clone());
            })
        }
    };

    html! {
        <div class="toolbar toolbar-padding">
            <div class="row text-gap">
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
                <button onclick={on_nav(Route::Media(MediaQuery::default()))} class="toolbar-item" title="Media">
                    <span class="icon film" />
                    <span>{"Media"}</span>
                </button>
                <button onclick={on_nav(Route::Search(SearchQuery::default()))} class="toolbar-item" title="Search Remotes">
                    <span class="icon magnifying-glass" />
                    <span class="hide-desktop">{"Search Remotes"}</span>
                </button>
                <button onclick={on_nav(Route::Queue(QueueQuery::default()))} class="toolbar-item" title="Queue">
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
