use core::fmt;

use gloo::events::EventListener;
use wasm_bindgen::JsValue;
use yew::prelude::*;

use crate::error::{CustomContext, Error, Message};

/// Which of the dashboard's tabbed views is shown. Stored in the URL;
/// [`DashboardView::Upcoming`] is the default and is omitted from the query.
#[derive(Default, Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum DashboardView {
    #[default]
    WatchNext,
    Upcoming,
    Schedule,
}

impl DashboardView {
    fn as_str(self) -> &'static str {
        match self {
            DashboardView::WatchNext => "next",
            DashboardView::Upcoming => "upcoming",
            DashboardView::Schedule => "schedule",
        }
    }

    fn from_param(s: &str) -> Option<Self> {
        match s {
            "next" => Some(DashboardView::WatchNext),
            "upcoming" => Some(DashboardView::Upcoming),
            "schedule" => Some(DashboardView::Schedule),
            _ => None,
        }
    }
}

#[derive(Default, Debug, Clone, PartialEq)]
pub(super) struct DashboardQuery {
    pub(super) page: usize,
    /// Offset of the schedule's visible window from the current week, in weeks.
    pub(super) week: i32,
    /// Mobile-only: reveal the past days of the current week.
    pub(super) week_start: bool,
    /// Offset of the upcoming-days strip's start from today, in days.
    pub(super) range: i32,
    /// Which tabbed view is shown.
    pub(super) view: DashboardView,
    /// Which media kinds the tabs show.
    pub(super) selection: MediaSelection,
}

impl DashboardQuery {
    fn to_query_string(&self) -> String {
        let mut s = form_urlencoded::Serializer::new(String::new());

        if self.page > 0 {
            s.append_pair("page", &self.page.to_string());
        }

        if self.week != 0 {
            s.append_pair("week", &self.week.to_string());
        }

        if self.week_start {
            s.append_pair("week_start", "1");
        }

        if self.range != 0 {
            s.append_pair("range", &self.range.to_string());
        }

        if self.view != DashboardView::default() {
            s.append_pair("view", self.view.as_str());
        }

        // Exclusionary: default is both shown, so only serialize deselected kinds.
        if !self.selection.shows {
            s.append_pair("hide", "shows");
        }

        if !self.selection.movies {
            s.append_pair("hide", "movies");
        }

        s.finish()
    }

    fn from_search(search: &str) -> Self {
        let mut this = Self::default();

        for (key, value) in form_urlencoded::parse(search.as_bytes()) {
            match key.as_ref() {
                "page" => {
                    this.page = value.parse::<usize>().unwrap_or(0);
                }
                "week" => {
                    this.week = value.parse::<i32>().unwrap_or(0);
                }
                "week_start" => {
                    this.week_start = value == "1" || value == "true";
                }
                "range" => {
                    this.range = value.parse::<i32>().unwrap_or(0);
                }
                "view" => {
                    this.view = DashboardView::from_param(&value).unwrap_or_default();
                }
                "hide" => match value.as_ref() {
                    "shows" => this.selection.shows = false,
                    "movies" => this.selection.movies = false,
                    _ => {}
                },
                _ => continue,
            }
        }

        this
    }
}

/// Field a media list is ordered by.
#[derive(Default, Debug, Clone, Copy, PartialEq)]
pub(super) enum SortField {
    #[default]
    Title,
    Release,
    Watched,
}

impl SortField {
    fn as_str(self) -> &'static str {
        match self {
            SortField::Title => "title",
            SortField::Release => "release",
            SortField::Watched => "watched",
        }
    }

    fn parse(value: &str) -> Option<Self> {
        match value {
            "title" => Some(SortField::Title),
            "release" => Some(SortField::Release),
            "watched" => Some(SortField::Watched),
            _ => None,
        }
    }
}

/// Tracked-state filter applied to a media list.
#[derive(Default, Debug, Clone, Copy, PartialEq)]
pub(super) enum TrackedFilter {
    All,
    #[default]
    Tracked,
    Untracked,
}

impl TrackedFilter {
    fn as_str(self) -> &'static str {
        match self {
            TrackedFilter::All => "all",
            TrackedFilter::Tracked => "tracked",
            TrackedFilter::Untracked => "untracked",
        }
    }

    fn parse(value: &str) -> Option<Self> {
        match value {
            "all" => Some(TrackedFilter::All),
            "tracked" => Some(TrackedFilter::Tracked),
            "untracked" => Some(TrackedFilter::Untracked),
            _ => None,
        }
    }

    /// Next state when cycling the toggle.
    pub(super) fn next(self) -> Self {
        match self {
            TrackedFilter::All => TrackedFilter::Tracked,
            TrackedFilter::Tracked => TrackedFilter::Untracked,
            TrackedFilter::Untracked => TrackedFilter::All,
        }
    }
}

/// Which media kinds the list shows. Two independent toggles, defaulting to
/// both enabled; serialized exclusionarily via a repeatable `hide` key.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(super) struct MediaSelection {
    pub(super) shows: bool,
    pub(super) movies: bool,
}

impl Default for MediaSelection {
    fn default() -> Self {
        Self {
            shows: true,
            movies: true,
        }
    }
}

impl MediaSelection {
    /// Whether the given item kind is currently shown.
    pub(super) fn contains(self, kind: api::MediaKind) -> bool {
        match kind {
            api::MediaKind::Shows => self.shows,
            api::MediaKind::Movies => self.movies,
        }
    }

    /// Selection showing only the given kind.
    fn only(kind: api::MediaKind) -> Self {
        Self {
            shows: matches!(kind, api::MediaKind::Shows),
            movies: matches!(kind, api::MediaKind::Movies),
        }
    }
}

#[derive(Default, Debug, Clone, PartialEq)]
pub(super) struct MediaQuery {
    pub(super) page: usize,
    pub(super) filter: String,
    pub(super) sort: SortField,
    pub(super) desc: bool,
    pub(super) tracked: TrackedFilter,
    pub(super) selection: MediaSelection,
}

impl MediaQuery {
    fn to_query_string(&self) -> String {
        let mut s = form_urlencoded::Serializer::new(String::new());

        if !self.filter.is_empty() {
            s.append_pair("filter", &self.filter);
        }

        if self.sort != SortField::default() {
            s.append_pair("sort", self.sort.as_str());
        }

        if self.desc {
            s.append_pair("dir", "desc");
        }

        if self.tracked != TrackedFilter::default() {
            s.append_pair("tracked", self.tracked.as_str());
        }

        // Exclusionary: default is both shown, so only serialize deselected kinds.
        if !self.selection.shows {
            s.append_pair("hide", "shows");
        }

        if !self.selection.movies {
            s.append_pair("hide", "movies");
        }

        if self.page > 0 {
            s.append_pair("page", &self.page.to_string());
        }

        s.finish()
    }

    fn from_search(search: &str) -> Self {
        let mut this = Self::default();

        for (key, value) in form_urlencoded::parse(search.as_bytes()) {
            match key.as_ref() {
                "filter" => {
                    this.filter = value.into_owned();
                }
                "sort" => {
                    if let Some(sort) = SortField::parse(value.as_ref()) {
                        this.sort = sort;
                    }
                }
                "dir" => {
                    this.desc = value.as_ref() == "desc";
                }
                "tracked" => {
                    if let Some(tracked) = TrackedFilter::parse(value.as_ref()) {
                        this.tracked = tracked;
                    }
                }
                "hide" => match value.as_ref() {
                    "shows" => this.selection.shows = false,
                    "movies" => this.selection.movies = false,
                    _ => {}
                },
                "page" => {
                    this.page = value.parse::<usize>().unwrap_or(0);
                }
                _ => continue,
            }
        }

        this
    }
}

#[derive(Default, Debug, Clone, PartialEq)]
pub(super) struct SearchQuery {
    pub(super) selection: MediaSelection,
    pub(super) filter: String,
}

impl SearchQuery {
    fn to_query_string(&self) -> String {
        let mut s = form_urlencoded::Serializer::new(String::new());

        // Exclusionary: search spans both kinds by default, so only serialize
        // deselected kinds.
        if !self.selection.shows {
            s.append_pair("hide", "shows");
        }

        if !self.selection.movies {
            s.append_pair("hide", "movies");
        }

        if !self.filter.is_empty() {
            s.append_pair("filter", &self.filter);
        }

        s.finish()
    }

    fn from_search(search: &str) -> Self {
        let mut this = Self::default();

        for (key, value) in form_urlencoded::parse(search.as_bytes()) {
            match key.as_ref() {
                "hide" => match value.as_ref() {
                    "shows" => this.selection.shows = false,
                    "movies" => this.selection.movies = false,
                    _ => {}
                },
                "filter" => {
                    this.filter = value.into_owned();
                }
                _ => continue,
            }
        }

        this
    }
}

/// How the people browse view is sorted.
#[derive(Default, Debug, Clone, Copy, PartialEq)]
pub(super) enum PersonSort {
    Name,
    #[default]
    Credits,
}

impl PersonSort {
    /// Whether this sort runs descending unless asked otherwise: names A to
    /// Z, credits most first.
    pub(super) fn default_desc(self) -> bool {
        matches!(self, PersonSort::Credits)
    }

    fn as_str(self) -> &'static str {
        match self {
            PersonSort::Name => "name",
            PersonSort::Credits => "credits",
        }
    }

    fn parse(s: &str) -> Option<Self> {
        match s {
            "name" => Some(PersonSort::Name),
            "credits" => Some(PersonSort::Credits),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub(super) struct PersonQuery {
    pub(super) page: usize,
    pub(super) filter: String,
    pub(super) sort: PersonSort,
    pub(super) desc: bool,
}

impl Default for PersonQuery {
    fn default() -> Self {
        Self {
            page: 0,
            filter: String::new(),
            sort: PersonSort::default(),
            desc: PersonSort::default().default_desc(),
        }
    }
}

impl PersonQuery {
    fn to_query_string(&self) -> String {
        let mut s = form_urlencoded::Serializer::new(String::new());

        if !self.filter.is_empty() {
            s.append_pair("filter", &self.filter);
        }

        if self.sort != PersonSort::default() {
            s.append_pair("sort", self.sort.as_str());
        }

        if self.desc != self.sort.default_desc() {
            s.append_pair("dir", if self.desc { "desc" } else { "asc" });
        }

        if self.page > 0 {
            s.append_pair("page", &self.page.to_string());
        }

        s.finish()
    }

    fn from_search(search: &str) -> Self {
        let mut this = Self::default();
        let mut desc = None;

        for (key, value) in form_urlencoded::parse(search.as_bytes()) {
            match key.as_ref() {
                "filter" => this.filter = value.into_owned(),
                "sort" => {
                    if let Some(sort) = PersonSort::parse(value.as_ref()) {
                        this.sort = sort;
                    }
                }
                "dir" => desc = Some(value.as_ref() == "desc"),
                "page" => this.page = value.parse::<usize>().unwrap_or(0),
                _ => continue,
            }
        }

        this.desc = desc.unwrap_or(this.sort.default_desc());
        this
    }
}

/// Which tasks the queue timeline shows.
#[derive(Debug, Default, Clone, Copy, PartialEq)]
pub(super) enum QueueFilter {
    #[default]
    All,
    /// Running and pending tasks.
    Upcoming,
    /// Tasks that completed successfully.
    Done,
    Failed,
}

impl QueueFilter {
    fn as_str(self) -> &'static str {
        match self {
            QueueFilter::All => "all",
            QueueFilter::Upcoming => "upcoming",
            QueueFilter::Done => "done",
            QueueFilter::Failed => "failed",
        }
    }

    fn parse(value: &str) -> Self {
        match value {
            "upcoming" => QueueFilter::Upcoming,
            "done" => QueueFilter::Done,
            "failed" => QueueFilter::Failed,
            _ => QueueFilter::All,
        }
    }
}

#[derive(Default, Debug, Clone, PartialEq)]
pub(super) struct QueueQuery {
    pub(super) filter: QueueFilter,
    /// The page shown, or `None` to follow the running task.
    pub(super) page: Option<usize>,
}

impl QueueQuery {
    fn to_query_string(&self) -> String {
        let mut s = form_urlencoded::Serializer::new(String::new());

        if self.filter != QueueFilter::All {
            s.append_pair("filter", self.filter.as_str());
        }

        if let Some(page) = self.page {
            s.append_pair("page", &page.to_string());
        }

        s.finish()
    }

    fn from_search(search: &str) -> Self {
        let mut this = Self::default();

        for (key, value) in form_urlencoded::parse(search.as_bytes()) {
            match key.as_ref() {
                "filter" => {
                    this.filter = QueueFilter::parse(value.as_ref());
                }
                "page" => {
                    this.page = value.parse::<usize>().ok();
                }
                _ => continue,
            }
        }

        this
    }
}

#[derive(Default, Debug, Clone, PartialEq)]
pub(super) struct ShowDetailQuery {
    pub(super) season: api::SeasonNumber,
    /// Whether the show details are currently displaying orphaned episodes.
    pub(super) orphaned: bool,
    /// Episode to scroll to, emitted as the URL fragment (`#S01E05`). Write-only:
    /// it is never parsed back from the location, since the fragment is read
    /// directly by the detail page (see [`Router::hash`]).
    pub(super) episode: Option<api::Code>,
}

impl ShowDetailQuery {
    fn to_query_string(&self) -> String {
        let mut s = form_urlencoded::Serializer::new(String::new());

        if self.season != api::SeasonNumber::FIRST {
            let ordinal = self.season.ordinal().to_string();
            s.append_pair("season", &ordinal);
        }

        if self.orphaned {
            s.append_pair("orphaned", "true");
        }

        s.finish()
    }

    fn from_search(search: &str) -> Self {
        let mut this = Self::default();

        for (key, value) in form_urlencoded::parse(search.as_bytes()) {
            match key.as_ref() {
                "season" => {
                    this.season = value
                        .parse::<u32>()
                        .ok()
                        .map(api::SeasonNumber::from_ordinal)
                        .unwrap_or_default();
                }
                "orphaned" => {
                    this.orphaned = value.as_ref() == "true";
                }
                _ => continue,
            }
        }

        this
    }
}

#[derive(Debug, Clone, PartialEq)]
pub(super) enum Route {
    Dashboard(DashboardQuery),
    Queue(QueueQuery),
    Media(MediaQuery),
    ShowDetail(api::ShowId, ShowDetailQuery),
    MovieDetail(api::MovieId),
    People(PersonQuery),
    PersonDetail(api::PersonId),
    Search(SearchQuery),
    Settings,
    Users,
    Account,
}

impl Default for Route {
    #[inline]
    fn default() -> Self {
        Route::Dashboard(DashboardQuery::default())
    }
}

impl fmt::Display for Route {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Route::Dashboard(q) => {
                let qs = q.to_query_string();

                if qs.is_empty() {
                    f.write_str("/")
                } else {
                    write!(f, "/?{qs}")
                }
            }
            Route::Queue(q) => {
                let qs = q.to_query_string();

                if qs.is_empty() {
                    f.write_str("/queue")
                } else {
                    write!(f, "/queue?{qs}")
                }
            }
            Route::Media(q) => {
                let qs = q.to_query_string();

                if qs.is_empty() {
                    f.write_str("/media")
                } else {
                    write!(f, "/media?{qs}")
                }
            }
            Route::ShowDetail(id, q) => {
                let qs = q.to_query_string();

                if qs.is_empty() {
                    write!(f, "/shows/{id}")?;
                } else {
                    write!(f, "/shows/{id}?{qs}")?;
                }

                if let Some(episode) = q.episode {
                    write!(f, "#{episode}")?;
                }

                Ok(())
            }
            Route::MovieDetail(id) => write!(f, "/movies/{id}"),
            Route::People(q) => {
                let qs = q.to_query_string();

                if qs.is_empty() {
                    f.write_str("/people")
                } else {
                    write!(f, "/people?{qs}")
                }
            }
            Route::PersonDetail(id) => write!(f, "/people/{id}"),
            Route::Search(q) => {
                let qs = q.to_query_string();

                if qs.is_empty() {
                    f.write_str("/search")
                } else {
                    write!(f, "/search?{qs}")
                }
            }
            Route::Settings => f.write_str("/settings"),
            Route::Users => f.write_str("/users"),
            Route::Account => f.write_str("/account"),
        }
    }
}

impl Route {
    fn from_location(path: &str, search: &str) -> Self {
        let mut parts = path.split('/').filter(|s| !s.is_empty());
        let search = search.strip_prefix('?').unwrap_or(search);

        match parts.next() {
            Some("queue") => Route::Queue(QueueQuery::from_search(search)),
            Some("media") => Route::Media(MediaQuery::from_search(search)),
            // Detail routes keep their /shows/{id} and /movies/{id} URLs; the
            // bare list paths redirect to the unified /media view, pre-filtered.
            Some("shows") => match parts.next() {
                Some(id) => id
                    .parse()
                    .map(|id| Route::ShowDetail(id, ShowDetailQuery::from_search(search)))
                    .unwrap_or_else(|_| Route::Media(MediaQuery::default())),
                None => Route::Media(MediaQuery {
                    selection: MediaSelection::only(api::MediaKind::Shows),
                    ..MediaQuery::from_search(search)
                }),
            },
            Some("movies") => match parts.next() {
                Some(id) => id
                    .parse()
                    .map(Route::MovieDetail)
                    .unwrap_or_else(|_| Route::Media(MediaQuery::default())),
                None => Route::Media(MediaQuery {
                    selection: MediaSelection::only(api::MediaKind::Movies),
                    ..MediaQuery::from_search(search)
                }),
            },
            Some("people") => match parts.next() {
                Some(id) => id
                    .parse()
                    .map(Route::PersonDetail)
                    .unwrap_or_else(|_| Route::People(PersonQuery::default())),
                None => Route::People(PersonQuery::from_search(search)),
            },
            Some("search") => Route::Search(SearchQuery::from_search(search)),
            Some("settings") => Route::Settings,
            Some("users") => Route::Users,
            Some("account") => Route::Account,
            _ => Route::Dashboard(DashboardQuery::from_search(search)),
        }
    }
}

/// Thin handle over the browser window, resolved once. Every method degrades to
/// a no-op (or `None`) when the window, document, or target element is
/// unavailable, so call sites never have to deal with the fallible `web_sys`
/// access chain.
#[derive(Clone, PartialEq)]
struct Dom {
    window: Option<web_sys::Window>,
}

impl Dom {
    fn new() -> Self {
        Self {
            window: web_sys::window(),
        }
    }

    /// The current URL fragment without its leading `#`, or `None` when there is
    /// no (non-empty) fragment.
    fn hash(&self) -> Option<String> {
        let hash = self.window.as_ref()?.location().hash().ok()?;
        let hash = hash.trim_start_matches('#');
        (!hash.is_empty()).then(|| hash.to_owned())
    }

    /// Scroll the element with the given id into view, returning whether it was
    /// found and scrolled.
    fn scroll_to_id(&self, id: &str) -> bool {
        let Some(window) = &self.window else {
            return false;
        };

        let Some(element) = window.document().and_then(|d| d.get_element_by_id(id)) else {
            return false;
        };

        element.scroll_into_view();
        true
    }
}

/// Context handed to descendant components so they can navigate without
/// threading callbacks through props. Backed by callbacks into
/// [`crate::root::Root`], which owns the [`RouterState`].
#[derive(Clone, PartialEq)]
pub(super) struct Router {
    navigate: Callback<Route>,
    replace: Callback<Route>,
    dom: Dom,
}

impl Router {
    pub(super) fn new(navigate: Callback<Route>, replace: Callback<Route>) -> Self {
        Self {
            navigate,
            replace,
            dom: Dom::new(),
        }
    }

    /// The current URL fragment (without the leading `#`), if any. Used to honor
    /// deep links to in-page anchors whose content loads asynchronously.
    pub(super) fn hash(&self) -> Option<String> {
        self.dom.hash()
    }

    /// Scroll the element with the given id into view, returning whether it was
    /// found. No-op when the element is not (yet) present.
    pub(super) fn scroll_to_id(&self, id: &str) -> bool {
        self.dom.scroll_to_id(id)
    }

    /// Navigate to `route`, pushing a new browser history entry.
    pub(super) fn push(&self, route: Route) {
        self.navigate.emit(route);
    }

    /// Navigate to `route` by replacing the current history entry. Used for URL
    /// corrections (e.g. clamping an out-of-range page) that should not leave a
    /// phantom entry for the back button to return to.
    pub(super) fn replace(&self, route: Route) {
        self.replace.emit(route);
    }
}

pub(super) struct RouterState {
    window: web_sys::Window,
    history: web_sys::History,
    pub(super) route: Route,
}

impl RouterState {
    pub(super) fn new() -> Result<Self, Error> {
        let window = web_sys::window().context(Message::MissingWindow)?;
        let history = window.history().context(Message::MissingHistory)?;
        let location = window.location();
        let path = location.pathname().context(Message::ReadingPathname)?;
        let search = location.search().unwrap_or_default();
        let route = Route::from_location(&path, &search);

        Ok(Self {
            window,
            history,
            route,
        })
    }

    pub(super) fn on_change(&self, callback: Callback<()>) -> EventListener {
        EventListener::new(&self.window, "popstate", move |_| callback.emit(()))
    }

    pub(super) fn navigate(&mut self, route: &Route) -> Result<(), Error> {
        let url = route.to_string();
        self.history
            .push_state_with_url(&JsValue::NULL, "", Some(&url))
            .context(Message::PushState)?;
        self.route = route.clone();
        Ok(())
    }

    /// Replace the current history entry rather than pushing a new one. Used for
    /// URL corrections (e.g. clamping an out-of-range page) that should not leave
    /// a phantom entry for the back button to return to.
    pub(super) fn replace(&mut self, route: &Route) -> Result<(), Error> {
        let url = route.to_string();
        self.history
            .replace_state_with_url(&JsValue::NULL, "", Some(&url))
            .context(Message::ReplaceState)?;
        self.route = route.clone();
        Ok(())
    }

    pub(super) fn on_pop(&mut self) -> Result<(), Error> {
        let location = self.window.location();
        let path = location.pathname().context(Message::ReadingPathname)?;
        let search = location.search().unwrap_or_default();
        self.route = Route::from_location(&path, &search);
        Ok(())
    }
}
