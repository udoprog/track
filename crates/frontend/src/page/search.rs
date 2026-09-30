use musli_web::web03::prelude::*;
use wasm_bindgen::JsCast as _;
use yew::prelude::*;

use crate::SetupChannel;
use crate::background::Background;
use crate::error::{CustomContext, Error, Message};
use crate::router::{MediaSelection, Route, Router, SearchQuery, ShowDetailQuery};
use crate::ui::{Button, Image, Link, MediaKindToggle, SEARCH, Skeleton, Variant};

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
    input: NodeRef,
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
}

#[derive(Properties, PartialEq)]
pub(crate) struct Props {
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
            input: NodeRef::default(),
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
                self.background.error(e);
                false
            }
        }
    }

    fn rendered(&mut self, _ctx: &Context<Self>, first_render: bool) {
        if first_render {
            self.background.title(Some("Search".to_string()));

            if let Some(input) = self.input.cast::<web_sys::HtmlElement>() {
                _ = input.focus();
            }
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
                <h1 class="visually-hidden">{"Search"}</h1>

                <div class="list-controls">
                    <div class="search-field">
                        <span class="icon magnifying-glass" aria-hidden="true" />
                        <input type="text" placeholder={SEARCH} aria-label="Search for shows and movies" ref={self.input.clone()} value={self.query.clone()} oninput={on_input} onkeydown={on_keydown} />
                        <Button icon="arrow-right" title="Search" variant={Variant::Primary} onclick={on_submit} />
                    </div>

                    <div class="chips">
                        <MediaKindToggle
                            selection={self.selection}
                            on_change={link.callback(Msg::SelectionChanged)}
                        />
                    </div>
                </div>

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
                if self.channel.id() != ws::ChannelId::NONE {
                    self._track_req = self
                        .channel
                        .request()
                        .body(api::TrackShowRequest { slug, remote })
                        .on_packet(ctx.link().callback(Msg::TrackShowDone))
                        .send();
                }

                Ok(false)
            }
            Msg::TrackMovie(slug, remote) => {
                if self.channel.id() != ws::ChannelId::NONE {
                    self._track_req = self
                        .channel
                        .request()
                        .body(api::TrackMovieRequest { slug, remote })
                        .on_packet(ctx.link().callback(Msg::TrackMovieDone))
                        .send();
                }

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
        if self.query.is_empty() {
            return html! {
                <p class="text-muted">{"Search TMDB and TVDB for shows and movies to track."}</p>
            };
        }

        if self.results.is_empty() {
            return if self.loading {
                html! {
                    <div class="search-results" aria-busy="true">
                        { for (0..3).map(|_| html! { <Skeleton class="search-skeleton" /> }) }
                    </div>
                }
            } else {
                html! {
                    <p class="text-muted">{format!("No results for \u{201c}{}\u{201d}.", self.query)}</p>
                }
            };
        }

        let on_more = ctx.link().callback(|e: MouseEvent| {
            e.prevent_default();
            Msg::LoadMore
        });

        let rows = merge(&self.results);

        html! {
            <>
                <div class="page-controls">
                    <span class="text-muted">{results_count(self.total)}</span>
                </div>

                <div class="search-results">
                    { for rows.iter().map(|row| self.view_row(ctx, row)) }
                </div>

                if self.loading {
                    <div class="row center">
                        <span class="item-inline-more"><span class="icon arrow-path spin" aria-hidden="true" /></span>
                    </div>
                } else if self.end {
                    <p class="text-muted center">{"No more results."}</p>
                } else if self.results.len() < self.total {
                    <div class="row center">
                        <Button icon="ellipsis-horizontal" title="Load more results" label="More results" onclick={on_more} />
                    </div>
                }
            </>
        }
    }

    /// One result: a small poster, its kind, title and year, two lines of
    /// overview and the sources it came from, with Track or Tracked beside it.
    fn view_row(&self, ctx: &Context<Self>, row: &Row<'_>) -> Html {
        let (kind_icon, kind_label) = if row.movie {
            ("film", "Movie")
        } else {
            ("tv", "Show")
        };

        let title = row.title.unwrap_or(if row.movie {
            "Untitled Movie"
        } else {
            "Untitled Show"
        });

        let action = match (&row.tracked, &row.track) {
            (Some(to), _) => html! {
                <Link to={to.clone()} class="button has-text" title={format!("Open the tracked {}", kind_label.to_lowercase())}>
                    <span class="icon check" aria-hidden="true" />
                    <span>{"Tracked"}</span>
                </Link>
            },
            (None, Some(track)) => {
                let track = track.clone();
                let onclick = ctx.link().callback(move |_| match track.clone() {
                    Track::Show(slug, remote) => Msg::TrackShow(slug, remote),
                    Track::Movie(remote) => Msg::TrackMovie(None, remote),
                });

                html! {
                    <Button icon="plus" label="Track" title={format!("Track {}", kind_label.to_lowercase())} {onclick} />
                }
            }
            (None, None) => html! {},
        };

        html! {
            <div key={row.key.clone()} class="search-result">
                if let Some(to) = &row.tracked {
                    <Link to={to.clone()} class="search-poster" decorative=true>
                        <Image src={row.poster.cloned()} placeholder=true />
                    </Link>
                } else {
                    <span class="search-poster">
                        <Image src={row.poster.cloned()} placeholder=true />
                    </span>
                }

                <div class="search-body">
                    <div class="search-heading">
                        <span class={classes!("icon", "sm", kind_icon)} role="img" aria-label={kind_label} title={kind_label} />

                        if let Some(to) = &row.tracked {
                            <Link to={to.clone()} class="search-title"><>{title}</></Link>
                        } else {
                            <span class="search-title">{title}</span>
                        }

                        if let Some(year) = row.year {
                            <span class="search-year">{year.to_string()}</span>
                        }
                    </div>

                    if let Some(overview) = row.overview {
                        <p class="search-overview">{overview}</p>
                    }

                    <div class="search-sources">
                        { for row.sources.iter().map(|(source, url)| html! {
                            if let Some(url) = url {
                                <a class="search-source" href={url.clone()} target="_blank" rel="noopener noreferrer" title={format!("Open on {source}")}>
                                    <span class={classes!("logo", source.as_id())} aria-hidden="true" />
                                </a>
                            } else {
                                <span class="search-source" title={source.to_string()}>
                                    <span class={classes!("logo", source.as_id())} aria-hidden="true" />
                                </span>
                            }
                        }) }
                    </div>
                </div>

                <div class="search-action">{action}</div>
            </div>
        }
    }
}

/// How to track a result that isn't tracked yet.
#[derive(Clone)]
enum Track {
    Show(Option<String>, api::Remote),
    Movie(api::Remote),
}

/// A search result as shown: results from several sources that lead to the
/// same tracked show or movie are one row carrying every source.
struct Row<'a> {
    key: String,
    movie: bool,
    title: Option<&'a str>,
    poster: Option<&'a api::Image>,
    year: Option<i16>,
    overview: Option<&'a str>,
    tracked: Option<Route>,
    track: Option<Track>,
    sources: Vec<(api::RemoteSource, Option<String>)>,
}

fn merge(results: &[api::SearchResult]) -> Vec<Row<'_>> {
    let mut rows: Vec<Row<'_>> = Vec::new();

    for result in results {
        let row = match result {
            api::SearchResult::Show(r) => Row {
                key: r.remote.to_string(),
                movie: false,
                title: r.title.as_deref(),
                poster: r.poster.as_ref(),
                year: r.first_air_date.map(|d| d.year()),
                overview: r.overview.as_deref(),
                tracked: r
                    .already_tracked
                    .map(|id| Route::ShowDetail(id, ShowDetailQuery::default())),
                track: Some(Track::Show(r.slug.clone(), r.remote.clone())),
                sources: vec![(*r.remote.source(), r.remote.show_url(r.slug.as_deref()))],
            },
            api::SearchResult::Movie(r) => Row {
                key: r.remote.to_string(),
                movie: true,
                title: r.title.as_deref(),
                poster: r.poster.as_ref(),
                year: r.release_date.map(|d| d.year()),
                overview: r.overview.as_deref(),
                tracked: r.already_tracked.map(Route::MovieDetail),
                track: Some(Track::Movie(r.remote.clone())),
                sources: vec![(*r.remote.source(), r.remote.movie_url())],
            },
        };

        if let Some(to) = &row.tracked
            && let Some(existing) = rows.iter_mut().find(|r| r.tracked.as_ref() == Some(to))
        {
            existing.sources.extend(row.sources);
            continue;
        }

        rows.push(row);
    }

    rows
}

fn results_count(total: usize) -> String {
    match total {
        1 => "1 result".to_owned(),
        n => format!("{n} results"),
    }
}
