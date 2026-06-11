use core::fmt;

use gloo::events::EventListener;
use wasm_bindgen::JsValue;
use yew::prelude::*;

use crate::error::{CustomContext, Error, Message};

#[derive(Default, Debug, Clone, PartialEq)]
pub(super) struct DashboardQuery {
    pub(super) page: usize,
}

impl DashboardQuery {
    fn to_query_string(&self) -> String {
        let mut s = form_urlencoded::Serializer::new(String::new());

        if self.page > 0 {
            s.append_pair("page", &self.page.to_string());
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
                _ => continue,
            }
        }

        this
    }
}

#[derive(Default, Debug, Clone, PartialEq)]
pub(super) struct PagedQuery {
    pub(super) page: usize,
    pub(super) filter: String,
}

impl PagedQuery {
    fn to_query_string(&self) -> String {
        let mut s = form_urlencoded::Serializer::new(String::new());

        if !self.filter.is_empty() {
            s.append_pair("filter", &self.filter);
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
pub(super) struct SeriesDetailQuery {
    pub(super) season: Option<api::SeasonNumber>,
}

impl SeriesDetailQuery {
    fn to_query_string(&self) -> String {
        let mut s = form_urlencoded::Serializer::new(String::new());

        if let Some(season) = self.season {
            let ordinal = season.ordinal().to_string();
            s.append_pair("season", &ordinal);
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
                        .map(api::SeasonNumber::from_ordinal);
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
    Queue,
    Series(PagedQuery),
    SeriesDetail(api::SeriesId, SeriesDetailQuery),
    Movies(PagedQuery),
    MovieDetail(api::MovieId),
    Search,
    Settings,
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
            Route::Queue => f.write_str("/queue"),
            Route::Series(q) => {
                let qs = q.to_query_string();

                if qs.is_empty() {
                    f.write_str("/series")
                } else {
                    write!(f, "/series?{qs}")
                }
            }
            Route::SeriesDetail(id, q) => {
                let qs = q.to_query_string();

                if qs.is_empty() {
                    write!(f, "/series/{id}")
                } else {
                    write!(f, "/series/{id}?{qs}")
                }
            }
            Route::Movies(q) => {
                let qs = q.to_query_string();

                if qs.is_empty() {
                    f.write_str("/movies")
                } else {
                    write!(f, "/movies?{qs}")
                }
            }
            Route::MovieDetail(id) => write!(f, "/movies/{id}"),
            Route::Search => f.write_str("/search"),
            Route::Settings => f.write_str("/settings"),
        }
    }
}

impl Route {
    fn from_location(path: &str, search: &str) -> Self {
        let mut parts = path.split('/').filter(|s| !s.is_empty());
        let search = search.strip_prefix('?').unwrap_or(search);

        match parts.next() {
            Some("queue") => Route::Queue,
            Some("series") => match parts.next() {
                Some(id) => id
                    .parse()
                    .map(|id| Route::SeriesDetail(id, SeriesDetailQuery::from_search(search)))
                    .unwrap_or(Route::Series(PagedQuery::default())),
                None => Route::Series(PagedQuery::from_search(search)),
            },
            Some("movies") => match parts.next() {
                Some(id) => id
                    .parse()
                    .map(Route::MovieDetail)
                    .unwrap_or(Route::Movies(PagedQuery::default())),
                None => Route::Movies(PagedQuery::from_search(search)),
            },
            Some("search") => Route::Search,
            Some("settings") => Route::Settings,
            _ => Route::Dashboard(DashboardQuery::from_search(search)),
        }
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

    pub(super) fn on_pop(&mut self) -> Result<(), Error> {
        let location = self.window.location();
        let path = location.pathname().context(Message::ReadingPathname)?;
        let search = location.search().unwrap_or_default();
        self.route = Route::from_location(&path, &search);
        Ok(())
    }
}
