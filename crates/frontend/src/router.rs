use core::fmt;

use gloo::events::EventListener;
use wasm_bindgen::JsValue;
use yew::prelude::*;

use crate::error::{CustomContext, Error, Message};

#[derive(Default, Debug, Clone, PartialEq)]
pub(super) struct SeriesQuery {
    pub(super) season: Option<api::SeasonNumber>,
}

impl SeriesQuery {
    fn to_query_string(&self) -> String {
        match self.season {
            Some(s) => format!("season={}", s.to_i64()),
            None => String::new(),
        }
    }

    fn from_search(search: &str) -> Self {
        let query = search.trim_start_matches('?');
        let mut season = None;

        for (k, v) in query.split('&').filter_map(|pair| {
            let mut it = pair.splitn(2, '=');
            Some((it.next()?, it.next().unwrap_or("")))
        }) {
            if k == "season" {
                season = v.parse::<i64>().ok().map(api::SeasonNumber::from_i64);
            }
        }

        Self { season }
    }
}

#[derive(Default, Debug, Clone, PartialEq)]
pub(super) enum Route {
    #[default]
    Dashboard,
    Queue,
    WatchNext,
    Series,
    SeriesDetail(api::SeriesId, SeriesQuery),
    Movies,
    MovieDetail(api::MovieId),
    Search,
    Settings,
}

impl fmt::Display for Route {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Route::Dashboard => f.write_str("/"),
            Route::Queue => f.write_str("/queue"),
            Route::WatchNext => f.write_str("/watch-next"),
            Route::Series => f.write_str("/series"),
            Route::SeriesDetail(id, q) => {
                let qs = q.to_query_string();
                if qs.is_empty() {
                    write!(f, "/series/{id}")
                } else {
                    write!(f, "/series/{id}?{qs}")
                }
            }
            Route::Movies => f.write_str("/movies"),
            Route::MovieDetail(id) => write!(f, "/movies/{id}"),
            Route::Search => f.write_str("/search"),
            Route::Settings => f.write_str("/settings"),
        }
    }
}

impl Route {
    fn from_location(path: &str, search: &str) -> Self {
        let mut parts = path.split('/').filter(|s| !s.is_empty());
        match parts.next() {
            Some("queue") => Route::Queue,
            Some("watch-next") => Route::WatchNext,
            Some("series") => match parts.next() {
                Some(id) => u64::from_str_radix(id, 16)
                    .map(|n| {
                        Route::SeriesDetail(api::SeriesId::new(n), SeriesQuery::from_search(search))
                    })
                    .unwrap_or(Route::Series),
                None => Route::Series,
            },
            Some("movies") => match parts.next() {
                Some(id) => u64::from_str_radix(id, 16)
                    .map(|n| Route::MovieDetail(api::MovieId::new(n)))
                    .unwrap_or(Route::Movies),
                None => Route::Movies,
            },
            Some("search") => Route::Search,
            Some("settings") => Route::Settings,
            _ => Route::Dashboard,
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
