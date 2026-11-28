use musli_web::web03::prelude::*;
use musli_web::yew023::ContextExt;
use wasm_bindgen::JsCast as _;
use yew::prelude::*;

use crate::background::Background;
use crate::class;
use crate::error::{CustomContext, Error, Message};
use crate::help;
use crate::router::{MediaSelection, Route, Router, SearchQuery, ShowDetailQuery};
use sockel::Skeleton;

use crate::ui::{Button, Help, Image, Link, MediaKindToggle, SEARCH, Variant};

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
    _broadcast: ws::Listener,
    _search_req: ws::Request,
    _track_req: ws::Request,
}

pub(crate) enum Msg {
    Channel(ws::ChannelEvent),
    AppBroadcast(Result<ws::Packet<api::AppBroadcast>, ws::Error>),
    QueryInput(String),
    SelectionChanged(MediaSelection),
    Submit,
    LoadMore,
    SearchDone(Result<ws::Packet<api::Search>, ws::Error>),
    TrackShow(api::TrackShowRequest),
    TrackMovie(api::TrackMovieRequest),
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
        let ws = ctx.handle();

        let channel = ws
            .channel()
            .on_change(ctx.link().callback(Msg::Channel))
            .open();
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
            channel,
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
                <h1 class={sockel::class::VISUALLY_HIDDEN}>{"Search"}</h1>

                <div class={class::LIST_CONTROLS}>
                    <div class={class::SEARCH_FIELD}>
                        <span class={classes!(class::ICON, class::ICON_MAGNIFYING_GLASS)} aria-hidden="true" />
                        <input type="text" placeholder={SEARCH} aria-label="Search for shows and movies" ref={self.input.clone()} value={self.query.clone()} oninput={on_input} onkeydown={on_keydown} />
                        <Button icon={class::ICON_ARROW_RIGHT} title="Search" variant={Variant::Primary} onclick={on_submit} />
                    </div>

                    <div class={classes!(class::CHIPS, sockel::class::CLUSTER)}>
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

/// Mark the show results with one of `show`'s remotes as tracked as it. With
/// `known`, only when a result is already tracked as `show`.
fn mark_show(results: &mut [api::SearchResult], show: &api::Show, known: bool) -> bool {
    let entries = results.iter_mut().filter_map(|r| match r {
        api::SearchResult::Show(s) => Some((&s.remote, &mut s.already_tracked)),
        api::SearchResult::Movie(_) => None,
    });

    mark_tracked(entries.collect(), &show.remotes, show.id, known)
}

/// [`mark_show`] for movies.
fn mark_movie(results: &mut [api::SearchResult], movie: &api::Movie, known: bool) -> bool {
    let entries = results.iter_mut().filter_map(|r| match r {
        api::SearchResult::Movie(m) => Some((&m.remote, &mut m.already_tracked)),
        api::SearchResult::Show(_) => None,
    });

    mark_tracked(entries.collect(), &movie.remotes, movie.id, known)
}

/// Set each entry whose remote is one of `remotes` as tracked as `id`, and
/// say whether any changed.
fn mark_tracked<I>(
    mut entries: Vec<(&api::Remote, &mut Option<I>)>,
    remotes: &[api::RemoteEntry],
    id: I,
    known: bool,
) -> bool
where
    I: Copy + PartialEq,
{
    if known && !entries.iter().any(|(_, t)| **t == Some(id)) {
        return false;
    }

    let mut changed = false;

    for (remote, tracked) in &mut entries {
        if **tracked != Some(id) && remotes.iter().any(|e| e.remote == **remote) {
            **tracked = Some(id);
            changed = true;
        }
    }

    changed
}

impl Search {
    fn try_update(&mut self, ctx: &Context<Self>, msg: Msg) -> Result<bool, Error> {
        match msg {
            Msg::Channel(event) => {
                if let ws::ChannelEvent::Failed(error) = event {
                    return Err(error.into());
                }
                self.page = 0;
                Ok(self.send_search(ctx, self.page))
            }
            Msg::AppBroadcast(packet) => {
                let packet = packet?;
                let event = packet.decode_event()?;
                if packet.is_from(self.channel.id()) {
                    return Ok(false);
                }
                // A created show is the viewer's own (or new); a changed one
                // carries nobody's tracking, but its sync may have found more
                // remotes (a TMDB show's TheTVDB id) of a show already Tracked.
                let (show, known) = match event.kind {
                    api::AppEventKind::ShowCreated { show } => (show, false),
                    api::AppEventKind::ShowChanged { show } => (show, true),
                    api::AppEventKind::MovieCreated { movie } => {
                        return Ok(mark_movie(&mut self.results, &movie, false));
                    }
                    api::AppEventKind::MovieChanged { movie } => {
                        return Ok(mark_movie(&mut self.results, &movie, true));
                    }
                    _ => return Ok(false),
                };

                Ok(mark_show(&mut self.results, &show, known))
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
            Msg::TrackShow(req) => {
                if self.channel.is_open() {
                    self._track_req = self
                        .channel
                        .request()
                        .body(req)
                        .on_packet(ctx.link().callback(Msg::TrackShowDone))
                        .send();
                }

                Ok(false)
            }
            Msg::TrackMovie(req) => {
                if self.channel.is_open() {
                    self._track_req = self
                        .channel
                        .request()
                        .body(req)
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

        if self.query.is_empty() || no_kind || !self.channel.is_open() {
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
                <p class={class::TEXT_MUTED}>
                    {"Search TMDB and TVDB for shows and movies to "}
                    // Wraps with the last word instead of alone on a line.
                    <span style="white-space: nowrap">
                        {"track."}
                        <Help section={help::FINDING} />
                    </span>
                </p>
            };
        }

        if self.results.is_empty() {
            return if self.loading {
                html! {
                    <div aria-busy="true">
                        { for (0..3).map(|_| html! { <Skeleton class={class::SEARCH_SKELETON} /> }) }
                    </div>
                }
            } else {
                html! {
                    <p class={class::TEXT_MUTED}>{format!("No results for \u{201c}{}\u{201d}.", self.query)}</p>
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
                <div class={class::PAGE_CONTROLS}>
                    <span class={class::TEXT_MUTED}>{results_count(self.total)}</span>
                </div>

                <div>
                    { for rows.iter().map(|row| self.view_row(ctx, row)) }
                </div>

                if self.loading {
                    <div class={classes!(sockel::class::CLUSTER, sockel::class::NOWRAP, sockel::class::LOOSE, sockel::class::CENTER)}>
                        <span class={class::ITEM_INLINE_MORE}><span class={classes!(class::ICON, class::ICON_ARROW_PATH, class::SPIN)} aria-hidden="true" /></span>
                    </div>
                } else if self.end {
                    <p class={classes!(class::TEXT_MUTED, class::CENTER)}>{"No more results."}</p>
                } else if self.results.len() < self.total {
                    <div class={classes!(sockel::class::CLUSTER, sockel::class::NOWRAP, sockel::class::LOOSE, sockel::class::CENTER)}>
                        <Button icon={class::ICON_ELLIPSIS_HORIZONTAL} title="Load more results" label="More results" onclick={on_more} />
                    </div>
                }
            </>
        }
    }

    /// One result: a small poster, its kind, title and year, two lines of
    /// overview and the sources it came from, with Track or Tracked beside it.
    fn view_row(&self, ctx: &Context<Self>, row: &Row<'_>) -> Html {
        let (kind_icon, kind_label) = if row.movie {
            (class::ICON_FILM, "Movie")
        } else {
            (class::ICON_TV, "Show")
        };

        let title = row.title.unwrap_or(if row.movie {
            "Untitled Movie"
        } else {
            "Untitled Show"
        });

        let action = match (&row.tracked, &row.track) {
            (Some(to), _) => html! {
                <Button to={to.clone()} icon={class::ICON_CHECK} label="Tracked" title={format!("Open the tracked {}", kind_label.to_lowercase())} />
            },
            (None, Some(track)) => {
                let track = track.clone();
                let onclick = ctx.link().callback(move |_| match track.clone() {
                    Track::Show(req) => Msg::TrackShow(req),
                    Track::Movie(req) => Msg::TrackMovie(req),
                });

                html! {
                    <Button icon={class::ICON_PLUS} label="Track" title={format!("Track {}", kind_label.to_lowercase())} {onclick} />
                }
            }
            (None, None) => html! {},
        };

        html! {
            <div key={row.key.clone()} class={class::SEARCH_RESULT}>
                if let Some(to) = &row.tracked {
                    <Link to={to.clone()} class={class::SEARCH_POSTER} decorative=true>
                        <Image src={row.poster.cloned()} placeholder=true />
                    </Link>
                } else {
                    <span class={class::SEARCH_POSTER}>
                        <Image src={row.poster.cloned()} placeholder=true />
                    </span>
                }

                <div class={classes!(class::SEARCH_BODY, sockel::class::STACK, sockel::class::SNUG)}>
                    <div class={class::SEARCH_HEADING}>
                        <span class={classes!(class::ICON, class::SM, kind_icon)} role="img" aria-label={kind_label} title={kind_label} />

                        if let Some(to) = &row.tracked {
                            <Link to={to.clone()} class={classes!(class::SEARCH_TITLE, class::TITLE, class::SM, sockel::class::TRUNCATE)}><>{title}</></Link>
                        } else {
                            <span class={classes!(class::SEARCH_TITLE, class::TITLE, class::SM, sockel::class::TRUNCATE)}>{title}</span>
                        }

                        if let Some(year) = row.year {
                            <span class={classes!(class::SEARCH_YEAR, sockel::class::NUMERIC)}>{year.to_string()}</span>
                        }
                    </div>

                    if let Some(overview) = row.overview {
                        <p class={class::SEARCH_OVERVIEW}>{overview}</p>
                    }

                    <div class={classes!(sockel::class::CLUSTER, sockel::class::NOWRAP, sockel::class::TIGHT)}>
                        { for row.sources.iter().map(|(source, url)| html! {
                            if let Some(url) = url {
                                <a class={class::SEARCH_SOURCE} href={url.clone()} target="_blank" rel="noopener noreferrer" title={format!("Open on {source}")}>
                                    <span class={classes!(class::LOGO, class::logo(source.as_id()))} aria-hidden="true" />
                                </a>
                            } else {
                                <span class={class::SEARCH_SOURCE} title={source.to_string()}>
                                    <span class={classes!(class::LOGO, class::logo(source.as_id()))} aria-hidden="true" />
                                </span>
                            }
                        }) }
                    </div>
                </div>

                <div class={class::SEARCH_ACTION}>{action}</div>
            </div>
        }
    }
}

/// How to track a result that isn't tracked yet.
#[derive(Clone)]
enum Track {
    Show(api::TrackShowRequest),
    Movie(api::TrackMovieRequest),
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
                track: Some(Track::Show(api::TrackShowRequest {
                    slug: r.slug.clone(),
                    remote: r.remote.clone(),
                    title: r.title.clone(),
                    overview: r.overview.clone(),
                })),
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
                track: Some(Track::Movie(api::TrackMovieRequest {
                    slug: None,
                    remote: r.remote.clone(),
                    title: r.title.clone(),
                    overview: r.overview.clone(),
                })),
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

#[cfg(test)]
mod tests {
    use super::mark_tracked;

    fn entry(remote: api::Remote) -> api::RemoteEntry {
        api::RemoteEntry {
            id: api::RemoteId::new(1),
            slug: None,
            remote,
            enabled: true,
            priority: 0,
            sync_kinds: None,
            cache: None,
        }
    }

    /// A show tracked from its TMDB result whose sync found its TheTVDB id
    /// marks the TheTVDB result Tracked too.
    #[test]
    fn changed_show_marks_its_other_remotes_of_a_tracked_result() {
        let show = api::ShowId::new(7);
        let tmdb = api::Remote::tmdb(1396);
        let tvdb = api::Remote::tvdb(81189);
        let remotes = [entry(tmdb.clone()), entry(tvdb.clone())];

        let mut a = Some(show);
        let mut b = None;
        let changed = mark_tracked(vec![(&tmdb, &mut a), (&tvdb, &mut b)], &remotes, show, true);

        assert!(changed);
        assert_eq!(b, Some(show));
    }

    /// A changed show nobody here has Tracked leaves its results alone, since
    /// the event says nothing of the viewer's tracking.
    #[test]
    fn changed_show_leaves_untracked_results() {
        let show = api::ShowId::new(7);
        let tmdb = api::Remote::tmdb(1396);
        let tvdb = api::Remote::tvdb(81189);
        let remotes = [entry(tmdb.clone()), entry(tvdb.clone())];

        let mut a = None;
        let mut b = None;
        let changed = mark_tracked(vec![(&tmdb, &mut a), (&tvdb, &mut b)], &remotes, show, true);

        assert!(!changed);
        assert_eq!((a, b), (None, None));
    }
}
