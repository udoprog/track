use musli_web::web03::prelude::*;
use wasm_bindgen::JsCast as _;
use yew::prelude::*;

use crate::SetupChannel;
use crate::background::Background;
use crate::error::{CustomContext, Error, Message};
use crate::router::{MediaSelection, Route, Router, SearchQuery, ShowDetailQuery};
use crate::ui::{Image, MediaKindToggle, SEARCH};

pub(crate) struct Search {
    channel: ws::Channel,
    background: Background,
    router: Router,
    query: String,
    selection: MediaSelection,
    results: Vec<api::SearchResult>,
    page: usize,
    total: usize,
    loading: bool,
    /// Set once a "load more" page comes back empty, so the UI can say there are
    /// no further results rather than keep offering to load more (remote totals
    /// can over-report).
    end: bool,
    _setup: SetupChannel,
    _broadcast: ws::Listener,
    _search_req: ws::Request,
    _track_req: ws::Request,
}

pub(crate) enum Msg {
    Channel(Result<ws::Channel, ws::Error>),
    AppBroadcast(Result<ws::Packet<api::AppBroadcast>, ws::Error>),
    QueryInput(String),
    SelectionChanged(MediaSelection),
    Submit,
    LoadMore,
    SearchDone(Result<ws::Packet<api::Search>, ws::Error>),
    TrackShow(Option<String>, api::Remote),
    TrackMovie(Option<String>, api::Remote),
    TrackShowDone(Result<ws::Packet<api::TrackShow>, ws::Error>),
    TrackMovieDone(Result<ws::Packet<api::TrackMovie>, ws::Error>),
    Navigate(Route),
}

#[derive(Properties, PartialEq)]
pub(crate) struct Props {
    pub(crate) onerror: Callback<Error>,
    pub(crate) selection: MediaSelection,
    pub(crate) filter: String,
}

impl Component for Search {
    type Message = Msg;
    type Properties = Props;

    fn create(ctx: &Context<Self>) -> Self {
        let (ws, _) = ctx
            .link()
            .context::<ws::Handle>(Callback::noop())
            .expect("Expected ws::Handle in context");

        let _setup = SetupChannel::new(ws.clone(), ctx.link().callback(Msg::Channel));
        let _broadcast = ws.on_broadcast(ctx.link().callback(Msg::AppBroadcast));

        let (background, _) = ctx
            .link()
            .context::<Background>(Callback::noop())
            .expect("Expected background handle in context");

        let (router, _) = ctx
            .link()
            .context::<Router>(Callback::noop())
            .expect("Expected router in context");

        Self {
            channel: ws::Channel::default(),
            background,
            router,
            query: ctx.props().filter.clone(),
            selection: ctx.props().selection,
            results: Vec::new(),
            page: 0,
            total: 0,
            loading: false,
            end: false,
            _setup,
            _broadcast,
            _search_req: ws::Request::default(),
            _track_req: ws::Request::default(),
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

    fn rendered(&mut self, _ctx: &Context<Self>, first_render: bool) {
        if first_render {
            self.background.title(Some("Search Remotes".to_string()));
        }
    }

    fn destroy(&mut self, _ctx: &Context<Self>) {
        self.background.title(None);
    }

    fn changed(&mut self, ctx: &Context<Self>, old_props: &Self::Properties) -> bool {
        let props = ctx.props();

        if props.selection != old_props.selection || props.filter != old_props.filter {
            self.selection = props.selection;
            self.query = props.filter.clone();
            self.results.clear();
            self.page = 0;
            self.total = 0;
            self.end = false;
            self.send_search(ctx, self.page);
        }

        true
    }

    fn view(&self, ctx: &Context<Self>) -> Html {
        let link = ctx.link();

        let on_input = link.callback(|e: InputEvent| {
            let input = e
                .target()
                .and_then(|t| t.dyn_into::<web_sys::HtmlInputElement>().ok());
            Msg::QueryInput(input.map(|i| i.value()).unwrap_or_default())
        });

        let on_keydown = link.batch_callback(|e: KeyboardEvent| {
            if e.key() == "Enter" {
                Some(Msg::Submit)
            } else {
                None
            }
        });

        let on_submit = link.callback(|_| Msg::Submit);

        html! {
            <>
                <h1>{"Search Remotes"}</h1>

                <input-controls>
                    <div class="input-group">
                        <input class="input-text fill" type="text" placeholder={SEARCH} autofocus=true value={self.query.clone()} oninput={on_input} onkeydown={on_keydown} />

                        <button class="desktop-has-text" onclick={on_submit}>
                            <span class="icon magnifying-glass" />
                            <span class="desktop-only">{"Search Remotes"}</span>
                        </button>
                    </div>

                    <controls>
                        <div class="input-group">
                            <MediaKindToggle
                                selection={self.selection}
                                on_change={link.callback(Msg::SelectionChanged)}
                            />
                        </div>
                    </controls>
                </input-controls>

                { self.view_results(ctx) }
            </>
        }
    }
}

impl Search {
    fn try_update(&mut self, ctx: &Context<Self>, msg: Msg) -> Result<bool, Error> {
        match msg {
            Msg::Channel(result) => {
                self.channel = result?;
                self.page = 0;
                Ok(self.send_search(ctx, self.page))
            }
            Msg::AppBroadcast(packet) => {
                let event = packet?.decode_event()?;
                if event.channel == self.channel.id() {
                    return Ok(false);
                }
                match event.kind {
                    api::AppEventKind::ShowCreated { show } => {
                        for entry in &show.remotes {
                            for r in &mut self.results {
                                if let api::SearchResult::Show(s) = r
                                    && s.remote == entry.remote
                                {
                                    s.already_tracked = Some(show.id);
                                }
                            }
                        }
                        Ok(true)
                    }
                    api::AppEventKind::MovieCreated { movie } => {
                        for entry in &movie.remotes {
                            for r in &mut self.results {
                                if let api::SearchResult::Movie(m) = r
                                    && m.remote == entry.remote
                                {
                                    m.already_tracked = Some(movie.id);
                                }
                            }
                        }
                        Ok(true)
                    }
                    _ => Ok(false),
                }
            }
            Msg::QueryInput(q) => {
                self.query = q;
                Ok(false)
            }
            Msg::SelectionChanged(selection) => {
                // Drive the search through the URL; `changed` runs the search.
                self.router.push(Route::Search(SearchQuery {
                    selection,
                    filter: self.query.clone(),
                }));
                Ok(false)
            }
            Msg::Submit => {
                self.router.push(Route::Search(SearchQuery {
                    selection: self.selection,
                    filter: self.query.clone(),
                }));
                Ok(false)
            }
            Msg::LoadMore => {
                self.page += 1;
                Ok(self.send_search(ctx, self.page))
            }
            Msg::SearchDone(result) => {
                self.loading = false;

                let resp = result
                    .context(Message::Searching)?
                    .decode()
                    .context(Message::Searching)?;

                if self.page == 0 {
                    self.results.clear();
                }

                self.end = resp.results.is_empty();
                self.results.extend(resp.results);
                self.total = resp.total;
                Ok(true)
            }
            Msg::TrackShow(slug, remote) => {
                self._track_req = self
                    .channel
                    .request()
                    .body(api::TrackShowRequest { slug, remote })
                    .on_packet(ctx.link().callback(Msg::TrackShowDone))
                    .send();
                Ok(false)
            }
            Msg::TrackMovie(slug, remote) => {
                self._track_req = self
                    .channel
                    .request()
                    .body(api::TrackMovieRequest { slug, remote })
                    .on_packet(ctx.link().callback(Msg::TrackMovieDone))
                    .send();
                Ok(false)
            }
            Msg::TrackShowDone(result) => {
                let show = result
                    .context(Message::TrackingShow)?
                    .decode()
                    .context(Message::TrackingShow)?;
                self.router
                    .push(Route::ShowDetail(show.id, ShowDetailQuery::default()));
                Ok(false)
            }
            Msg::TrackMovieDone(result) => {
                let movie = result
                    .context(Message::TrackingMovie)?
                    .decode()
                    .context(Message::TrackingMovie)?;
                self.router.push(Route::MovieDetail(movie.id));
                Ok(false)
            }
            Msg::Navigate(route) => {
                self.router.push(route);
                Ok(false)
            }
        }
    }

    fn send_search(&mut self, ctx: &Context<Self>, page: usize) -> bool {
        let no_kind = !self.selection.shows && !self.selection.movies;

        if self.query.is_empty() || no_kind || self.channel.id() == ws::ChannelId::NONE {
            self.loading = false;
            return true;
        }

        self.loading = true;

        self._search_req = self
            .channel
            .request()
            .body(api::SearchRequest {
                query: self.query.clone(),
                page,
                shows: self.selection.shows,
                movies: self.selection.movies,
            })
            .on_packet(ctx.link().callback(Msg::SearchDone))
            .send();

        true
    }

    fn view_results(&self, ctx: &Context<Self>) -> Html {
        let on_more = ctx.link().callback(|e: MouseEvent| {
            e.prevent_default();
            Msg::LoadMore
        });

        let loaded = self.results.len();

        html! {
            <>
                { for self.results.iter().map(|r| match r {
                    api::SearchResult::Show(show) => self.view_show_result(ctx, show),
                    api::SearchResult::Movie(movie) => self.view_movie_result(ctx, movie),
                }) }

                if self.loading {
                    <div class="row center">
                        <span class="item-inline-more"><span class="icon arrow-path spin" /></span>
                    </div>
                } else if self.end {
                    <div class="row center">
                        <span class="item-inline-more">{"No more results."}</span>
                    </div>
                } else if loaded < self.total {
                    <a class="row center clickable" onclick={on_more}>
                        <span class="item-inline-more">
                            <span class="icon ellipsis-horizontal" />
                        </span>
                    </a>
                }
            </>
        }
    }

    fn view_show_result(&self, ctx: &Context<Self>, r: &api::SearchShow) -> Html {
        let slug = r.slug.clone();
        let remote = r.remote.clone();
        let show_id = r.already_tracked;

        let on_nav = show_id.map(|id| {
            ctx.link()
                .callback(move |_| Msg::Navigate(Route::ShowDetail(id, ShowDetailQuery::default())))
        });

        let on_track = ctx
            .link()
            .callback(move |_| Msg::TrackShow(slug.clone(), remote.clone()));

        html! {
            <div key={r.remote.to_string()} class="desktop-row mobile-column">
                <Image class="poster poster-side top desktop-only" src={r.poster.clone()} placeholder=true />
                <Image class="banner mobile-only" src={r.banner.clone()} placeholder=true />

                <div class="column top fill">
                    <div class="row-split">
                        <a class="item-inline-lg" href={r.remote.show_url(r.slug.as_deref())} target="_blank" rel="noopener noreferrer" title={format!("Open on {}", r.remote.source())}>
                            <span class={classes!("logo", r.remote.source().as_id())} />
                        </a>

                        <h2 class={classes!(on_nav.is_some().then_some("clickable"))} onclick={on_nav.clone()}>
                            <div class="item-inline" title="Movie">
                                <div class="icon tv" />
                            </div>

                            <span class="item-title">{r.title.as_deref().unwrap_or("Untitled Movie")}</span>
                        </h2>

                        <div class="row">
                            if let Some(on_nav) = on_nav {
                                <button class="desktop-has-text" onclick={on_nav} title="Already tracked">
                                    <span class="icon check" />
                                    <span class="desktop-only">{"Tracked"}</span>
                                </button>
                            } else {
                                <button class="desktop-has-text" onclick={on_track} title="Track show">
                                    <span class="icon plus" />
                                    <span class="desktop-only">{"Track"}</span>
                                </button>
                            }
                        </div>
                    </div>

                    <div class="row">
                        if let Some(date) = r.first_air_date {
                            <span class="text-muted">{date.year().to_string()}</span>
                        }
                    </div>

                    if let Some(ref overview) = r.overview {
                        <p class="overview text-muted">{overview}</p>
                    }
                </div>
            </div>
        }
    }

    fn view_movie_result(&self, ctx: &Context<Self>, r: &api::SearchMovie) -> Html {
        let remote = r.remote.clone();
        let show_id = r.already_tracked;

        let on_nav = show_id.map(|id| {
            ctx.link()
                .callback(move |_| Msg::Navigate(Route::MovieDetail(id)))
        });

        let on_track = ctx
            .link()
            .callback(move |_| Msg::TrackMovie(None, remote.clone()));

        html! {
            <div key={r.remote.to_string()} class="desktop-row mobile-column align-top">
                <Image class="poster poster-side top desktop-only" src={r.poster.clone()} placeholder=true />
                <Image class="banner mobile-only" src={r.banner.clone()} placeholder=true />

                <div class="column fill">
                    <div class="row-split">
                        <a class="item-inline-lg" href={r.remote.movie_url()} target="_blank" rel="noopener noreferrer" title={format!("Open on {}", r.remote.source())}>
                            <span class={classes!("logo", r.remote.source().as_id())} />
                        </a>

                        <h2 class={classes!(on_nav.is_some().then_some("clickable"))} onclick={on_nav.clone()}>
                            <div class="item-inline" title="Movie">
                                <div class="icon film" />
                            </div>

                            <span class="item-title">{r.title.as_deref().unwrap_or("Untitled Movie")}</span>
                        </h2>

                        <div class="row">
                            if let Some(on_nav) = on_nav {
                                <button class="desktop-has-text" onclick={on_nav} title="Already tracked">
                                    <span class="icon check" />
                                    <span class="desktop-only">{"Tracked"}</span>
                                </button>
                            } else {
                                <button class="desktop-has-text" onclick={on_track} title="Track movie">
                                    <span class="icon plus" />
                                    <span class="desktop-only">{"Track"}</span>
                                </button>
                            }
                        </div>
                    </div>

                    <div class="row">
                        if let Some(date) = r.release_date {
                            <span class="text-muted">{date.year().to_string()}</span>
                        }
                    </div>

                    if let Some(ref overview) = r.overview {
                        <p class="overview text-muted">{overview}</p>
                    }
                </div>
            </div>
        }
    }
}
