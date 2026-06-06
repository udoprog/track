use core::fmt;

use gloo::events::EventListener;
use wasm_bindgen::JsValue;
use yew::prelude::*;

use crate::error::{CustomContext, Error, Message};

#[derive(Debug, Clone, PartialEq)]
pub(super) enum Route {
    Dashboard,
    Series,
    SeriesDetail(api::SeriesId),
    Movies,
    MovieDetail(api::MovieId),
    Settings,
}

impl Default for Route {
    fn default() -> Self {
        Route::Dashboard
    }
}

impl fmt::Display for Route {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Route::Dashboard => f.write_str("/"),
            Route::Series => f.write_str("/series"),
            Route::SeriesDetail(id) => write!(f, "/series/{id}"),
            Route::Movies => f.write_str("/movies"),
            Route::MovieDetail(id) => write!(f, "/movies/{id}"),
            Route::Settings => f.write_str("/settings"),
        }
    }
}

impl Route {
    fn from_path(path: &str) -> Self {
        let mut parts = path.split('/').filter(|s| !s.is_empty());
        match parts.next() {
            Some("series") => match parts.next() {
                Some(id) => u64::from_str_radix(id, 16)
                    .map(|n| Route::SeriesDetail(api::SeriesId::new(n)))
                    .unwrap_or(Route::Series),
                None => Route::Series,
            },
            Some("movies") => match parts.next() {
                Some(id) => u64::from_str_radix(id, 16)
                    .map(|n| Route::MovieDetail(api::MovieId::new(n)))
                    .unwrap_or(Route::Movies),
                None => Route::Movies,
            },
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
        let path = window
            .location()
            .pathname()
            .context(Message::ReadingPathname)?;
        let route = Route::from_path(&path);

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
        let path = self
            .window
            .location()
            .pathname()
            .context(Message::ReadingPathname)?;
        self.route = Route::from_path(&path);
        Ok(())
    }
}
