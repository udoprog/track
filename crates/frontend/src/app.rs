use std::rc::Rc;

use api::{TimeInfo, TimeZone, Timestamp};
use gloo::events::EventListener;
use gloo::timers::callback::Interval;
use musli_web::web03::prelude::*;
use yew::prelude::*;

use crate::background::{Background, BackgroundState};
use crate::error::{CustomContext, Error, Message, RcError};
use crate::page::{
    Dashboard, MediaList, MovieDetail, PersonDetail, PersonList, Queue, Search, Settings,
    ShowDetail,
};
use crate::router::{
    DashboardQuery, MediaQuery, PersonQuery, QueueQuery, Route, Router, RouterState, SearchQuery,
};
use crate::setup_channel::SetupChannel;
use crate::ui::{ErrorBox, Outline, OutlineControl, OutlineEntry, TopLanguages};

pub(super) struct App {
    channel: ws::Channel,
    ws: ws::Service,
    /// Connection state of the websocket, surfaced in the toolbar.
    ws_state: ws::State,
    time: TimeInfo,
    /// Normalized site title (config `page_title`, defaulting to `"Track"`).
    /// Drives both the toolbar and the tab-title fallback.
    site_title: AttrValue,
    top_languages: TopLanguages,
    error: Option<RcError>,
    /// Entries currently shown in the outline, pushed in by a consumer through
    /// [`OutlineControl`] and forwarded to [`Outline`]. `None` hides it.
    outline_entries: Rc<[OutlineEntry]>,
    /// Control handed to consumers via context.
    outline_control: OutlineControl,
    router_state: RouterState,
    router: Router,
    background_state: BackgroundState,
    background: Background,
    _setup: SetupChannel,
    _state_listener: ws::StateListener,
    _broadcast: ws::Listener,
    _config_req: ws::Request,
    _top_languages_req: ws::Request,
    _tick_minute_interval: Interval,
    _history_listener: EventListener,
    onclearerror: Callback<()>,
}

pub(super) enum Msg {
    Channel(Result<ws::Channel, ws::Error>),
    WsState(ws::State),
    AppBroadcast(Result<ws::Packet<api::AppBroadcast>, ws::Error>),
    TickTime,
    ConfigLoaded(Result<ws::Packet<api::GetConfig>, ws::Error>),
    TopLanguagesLoaded(Result<ws::Packet<api::GetTopLanguages>, ws::Error>),
    /// A consumer set (or cleared) the outline contents.
    SetOutline(Rc<[OutlineEntry]>),
    WsError(ws::Error),
    Navigate(Route),
    Replace(Route),
    PopState,
    SetBackground(String),
    SetTitle(Option<String>),
    Error(Error),
    ClearError,
}

impl Component for App {
    type Message = Msg;
    type Properties = ();

    fn create(ctx: &Context<Self>) -> Self {
        let link = ctx.link();

        let ws = ws::connect(ws::Connect::location("/ws"))
            .close_before_unload()
            .on_error(link.callback(Msg::WsError))
            .build();

        let _setup = SetupChannel::new(ws.handle().clone(), link.callback(Msg::Channel));

        let (ws_state, _state_listener) = ws.handle().on_state_change(link.callback(Msg::WsState));

        let _broadcast = ws
            .handle()
            .clone()
            .on_broadcast(link.callback(Msg::AppBroadcast));

        let outline_control = OutlineControl::new(link.callback(Msg::SetOutline));

        let _tick_minute_interval = Interval::new(10_000, {
            let link = link.clone();
            move || link.send_message(Msg::TickTime)
        });

        let background_state = BackgroundState::new();

        let router_state = RouterState::new().expect("Setting up router");
        let _history_listener = router_state.on_change(link.callback(|()| Msg::PopState));

        let onerror = link.callback(Msg::Error);
        let onclearerror = link.callback(|()| Msg::ClearError);
        let on_navigate = link.callback(Msg::Navigate);
        let on_replace = link.callback(Msg::Replace);
        let on_background = link.callback(Msg::SetBackground);
        let on_title = link.callback(Msg::SetTitle);

        let router = Router::new(on_navigate, on_replace);

        let background = Background::new(on_background, on_title, onerror);

        Self {
            channel: ws::Channel::default(),
            ws,
            ws_state,
            time: TimeInfo::new(TimeZone::system(), Timestamp::now()),
            site_title: AttrValue::from("Track"),
            top_languages: TopLanguages::default(),
            error: None,
            outline_entries: Rc::from([]),
            outline_control,
            router_state,
            router,
            _history_listener,
            background_state,
            background,
            _setup,
            _state_listener,
            _broadcast,
            _config_req: ws::Request::default(),
            _top_languages_req: ws::Request::default(),
            _tick_minute_interval,
            onclearerror,
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
        html! {
            <>
                <div class="background">
                    if let Some(url) = self.background_state.url() {
                        <div
                            key={url.to_owned()}
                            class="background-image"
                            style={format!("background-image: url('{url}')")}
                        />
                    }
                </div>

                <ContextProvider<ws::Handle> context={self.ws.handle()}>
                <ContextProvider<TimeInfo> context={self.time.clone()}>
                <ContextProvider<TopLanguages> context={self.top_languages.clone()}>
                <ContextProvider<Router> context={self.router.clone()}>
                <ContextProvider<Background> context={self.background.clone()}>
                <ContextProvider<OutlineControl> context={self.outline_control.clone()}>
                    <div id="application">
                        if let Some(ref error) = self.error {
                            <div id="error">
                                <ErrorBox error={error.clone()} onclearerror={self.onclearerror.clone()} />
                            </div>
                        }

                        <Toolbar site_title={self.site_title.clone()} connected={self.ws_state.is_open()} section={Section::of(&self.router_state.route)} />

                        <div id="content">
                            <div id="page">
                                { self.view_page(ctx) }
                            </div>

                            <Outline entries={self.outline_entries.clone()} />
                        </div>
                    </div>
                </ContextProvider<OutlineControl>>
                </ContextProvider<Background>>
                </ContextProvider<Router>>
                </ContextProvider<TopLanguages>>
                </ContextProvider<TimeInfo>>
                </ContextProvider<ws::Handle>>
            </>
        }
    }
}

impl App {
    fn try_update(&mut self, ctx: &Context<Self>, msg: Msg) -> Result<bool, Error> {
        let link = ctx.link();

        match msg {
            Msg::Channel(result) => {
                self.channel = result?;

                if self.channel.id() != ws::ChannelId::NONE {
                    self._config_req = self
                        .channel
                        .request()
                        .body(api::GetConfigRequest)
                        .on_packet(link.callback(Msg::ConfigLoaded))
                        .send();

                    self._top_languages_req = self
                        .channel
                        .request()
                        .body(api::GetTopLanguagesRequest)
                        .on_packet(link.callback(Msg::TopLanguagesLoaded))
                        .send();
                }

                Ok(true)
            }
            Msg::WsState(state) => {
                if self.ws_state == state {
                    return Ok(false);
                }

                self.ws_state = state;
                Ok(true)
            }
            Msg::AppBroadcast(result) => {
                let event = result?.decode_event()?;

                match event.kind {
                    api::AppEventKind::ConfigChanged { config } => {
                        crate::theme::apply(config.theme);
                        let mut render = self.apply_config_title(&config);

                        let tz = Self::tz_from_config(&config);
                        if *self.time.tz() != tz {
                            self.time = TimeInfo::new(tz, self.time.now());
                            render = true;
                        }

                        return Ok(render);
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

                crate::theme::apply(config.theme);
                let mut render = self.apply_config_title(&config);

                let new_tz = Self::tz_from_config(&config);
                if *self.time.tz() != new_tz {
                    self.time = TimeInfo::new(new_tz, self.time.now());
                    render = true;
                }

                Ok(render)
            }
            Msg::WsError(e) => Err(e.into()),
            Msg::Navigate(route) => {
                if let Err(e) = self.router_state.navigate(&route) {
                    self.error = Some(RcError::from(e));
                }

                Ok(true)
            }
            Msg::Replace(route) => {
                if let Err(e) = self.router_state.replace(&route) {
                    self.error = Some(RcError::from(e));
                }

                Ok(true)
            }
            Msg::PopState => {
                if let Err(e) = self.router_state.on_pop() {
                    self.error = Some(RcError::from(e));
                }

                Ok(true)
            }
            Msg::SetBackground(background) => {
                if let Err(e) = self.background_state.set_background(background) {
                    self.error = Some(RcError::from(e));
                }

                Ok(true)
            }
            Msg::SetTitle(title) => {
                self.background_state.set_title(title);
                Ok(false)
            }
            Msg::Error(e) => {
                self.error = Some(RcError::from(e));
                Ok(true)
            }
            Msg::ClearError => {
                self.error = None;
                Ok(true)
            }
        }
    }

    /// Apply the config's title to the tab-title fallback and the toolbar.
    /// Returns whether the toolbar title changed (and a re-render is needed).
    fn apply_config_title(&mut self, config: &api::Config) -> bool {
        let title = Self::title_from_config(config);
        self.background_state.set_default_title(&title);

        if self.site_title != title {
            self.site_title = title.into();
            return true;
        }

        false
    }

    /// The effective site title: the configured `page_title`, or `"Track"` when
    /// it is empty/whitespace-only.
    fn title_from_config(config: &api::Config) -> String {
        let title = config.page_title.trim();

        if title.is_empty() {
            "Track".to_owned()
        } else {
            title.to_owned()
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

    fn view_page(&self, _: &Context<Self>) -> Html {
        match self.router_state.route {
            Route::Dashboard(ref q) => {
                html! { <Dashboard page={q.page} week={q.week} week_start={q.week_start} range={q.range} view={q.view} selection={q.selection} /> }
            }
            Route::Queue(ref q) => html! {
                <Queue focus={q.focus} page={q.page} />
            },
            Route::Media(ref q) => html! {
                <MediaList page={q.page} filter={q.filter.clone()} sort={q.sort} desc={q.desc} tracked={q.tracked} selection={q.selection} />
            },
            Route::ShowDetail(show_id, ref q) => {
                html! {
                    <ShowDetail {show_id} season={q.season} orphaned={q.orphaned} />
                }
            }
            Route::MovieDetail(movie_id) => {
                html! { <MovieDetail {movie_id} /> }
            }
            Route::People(ref q) => html! {
                <PersonList page={q.page} filter={q.filter.clone()} sort={q.sort} desc={q.desc} />
            },
            Route::PersonDetail(person_id) => {
                html! { <PersonDetail {person_id} /> }
            }
            Route::Search(ref q) => html! {
                <Search selection={q.selection} filter={q.filter.clone()} />
            },
            Route::Settings => html! { <Settings /> },
        }
    }
}

/// The toolbar entry a route belongs to, which is highlighted.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Section {
    Dashboard,
    Media,
    People,
    Search,
    Queue,
    Settings,
}

impl Section {
    fn of(route: &Route) -> Self {
        match route {
            Route::Dashboard(..) => Section::Dashboard,
            Route::Media(..) | Route::ShowDetail(..) | Route::MovieDetail(..) => Section::Media,
            Route::People(..) | Route::PersonDetail(..) => Section::People,
            Route::Search(..) => Section::Search,
            Route::Queue(..) => Section::Queue,
            Route::Settings => Section::Settings,
        }
    }
}

#[derive(Properties, PartialEq)]
struct ToolbarProps {
    site_title: AttrValue,
    /// Whether the websocket is currently connected.
    connected: bool,
    section: Section,
}

#[function_component]
fn Toolbar(props: &ToolbarProps) -> Html {
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

    let (connection_icon, connection_style, connection_title) = if props.connected {
        ("signal", "success", "Connected")
    } else {
        ("signal-slash", "danger", "Disconnected")
    };

    html! {
        <div id="toolbar" class="toolbar toolbar-padding">
            <div class="row text-gap">
                <span class="site-title clickable" onclick={on_nav(Route::Dashboard(DashboardQuery::default()))}>{ props.site_title.clone() }</span>
            </div>

            <button class="toolbar-toggle" onclick={on_menu_toggle} title="Navigation" aria-expanded={menu_open.to_string()}>
                <span class={classes!("icon", if *menu_open { "x-mark" } else { "bars-3" })} />
            </button>

            <div class={classes!("toolbar-dropdown", (!*menu_open).then_some("desktop-only"))}>
                <div class="toolbar-item mobile-has-text" title={connection_title}>
                    <span class={classes!("icon", connection_style, connection_icon)} />
                    <span class="mobile-only">{connection_title}</span>
                </div>

                <button class={classes!("toolbar-item", "has-text", (props.section == Section::Dashboard).then_some("active"))} aria-current={(props.section == Section::Dashboard).then_some("page")} onclick={on_nav(Route::Dashboard(DashboardQuery::default()))} title="Dashboard">
                    <span class="icon rectangle-stack" />
                    <span>{"Dashboard"}</span>
                </button>

                <button class={classes!("toolbar-item", "has-text", (props.section == Section::Media).then_some("active"))} aria-current={(props.section == Section::Media).then_some("page")} onclick={on_nav(Route::Media(MediaQuery::default()))} title="Media">
                    <span class="icon film" />
                    <span>{"Media"}</span>
                </button>

                <button class={classes!("toolbar-item", "has-text", (props.section == Section::People).then_some("active"))} aria-current={(props.section == Section::People).then_some("page")} onclick={on_nav(Route::People(PersonQuery::default()))} title="People">
                    <span class="icon users" />
                    <span>{"People"}</span>
                </button>

                <button class={classes!("toolbar-item", "has-text", (props.section == Section::Search).then_some("active"))} aria-current={(props.section == Section::Search).then_some("page")} onclick={on_nav(Route::Search(SearchQuery::default()))} title="Search">
                    <span class="icon magnifying-glass" />
                    <span>{"Search"}</span>
                </button>

                <button class={classes!("toolbar-item", "has-text", (props.section == Section::Queue).then_some("active"))} aria-current={(props.section == Section::Queue).then_some("page")} onclick={on_nav(Route::Queue(QueueQuery::default()))} title="Queue">
                    <span class="icon queue-list" />
                    <span>{"Queue"}</span>
                </button>

                <button class={classes!("toolbar-item", "has-text", (props.section == Section::Settings).then_some("active"))} aria-current={(props.section == Section::Settings).then_some("page")} onclick={on_nav(Route::Settings)} title="Settings">
                    <span class="icon cog-6-tooth" />
                    <span>{"Settings"}</span>
                </button>
            </div>
        </div>
    }
}
