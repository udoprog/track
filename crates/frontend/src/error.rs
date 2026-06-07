use core::fmt;
use core::iter;
use core::ops::Deref;

use std::rc::Rc;

use derive_more::Display;
use musli_web::web;
use wasm_bindgen::JsValue;
use yew::html::ImplicitClone;

#[derive(Clone)]
pub(crate) struct RcError {
    error: Rc<Error>,
}

impl ImplicitClone for RcError {}

impl From<Error> for RcError {
    #[inline]
    fn from(error: Error) -> Self {
        Self {
            error: Rc::new(error),
        }
    }
}

impl PartialEq for RcError {
    #[inline]
    fn eq(&self, other: &Self) -> bool {
        Rc::ptr_eq(&self.error, &other.error)
    }
}

impl Deref for RcError {
    type Target = Error;

    #[inline]
    fn deref(&self) -> &Self::Target {
        &self.error
    }
}

pub(crate) struct Error {
    pub(crate) message: Message,
    source: Option<Box<Source>>,
}

#[derive(Debug, Clone, Copy, Display)]
pub(crate) enum Message {
    #[display("WebSocket Error")]
    WebSocketError,
    #[display("missing window")]
    MissingWindow,
    #[display("missing history API")]
    MissingHistory,
    #[display("reading location pathname")]
    ReadingPathname,
    #[display("pushing browser history state")]
    PushState,
    #[display("loading pending")]
    LoadingPending,
    #[display("loading schedule")]
    LoadingSchedule,
    #[display("loading config")]
    LoadingConfig,
    #[display("loading series")]
    LoadingSeries,
    #[display("loading seasons")]
    LoadingSeasons,
    #[display("loading episodes")]
    LoadingEpisodes,
    #[display("marking watched")]
    MarkingWatched,
    #[display("loading movies")]
    LoadingMovies,
    #[display("loading watch history")]
    LoadingWatched,
    #[display("saving config")]
    SavingConfig,
    #[display("removing watch")]
    RemovingWatched,
    #[display("removing movie")]
    RemovingMovie,
    #[display("untracking series")]
    UntrackingSeries,
    #[display("updating movie tracking")]
    UntrackingMovie,
    #[display("removing series")]
    RemovingSeries,
    #[display("syncing series")]
    SyncingSeries,
    #[display("setting sync source")]
    SettingSyncSource,
    #[display("loading tasks")]
    LoadingTasks,
    #[display("searching")]
    Searching,
    #[display("tracking series")]
    TrackingSeries,
    #[display("tracking movie")]
    TrackingMovie,
}

pub(crate) trait CustomContext<T> {
    fn context(self, message: Message) -> Result<T, Error>;
}

impl<T, E> CustomContext<T> for Result<T, E>
where
    Source: From<E>,
{
    #[inline]
    fn context(self, message: Message) -> Result<T, Error> {
        match self {
            Ok(t) => Ok(t),
            Err(e) => Err(Error {
                message,
                source: Some(Box::new(Source::from(e))),
            }),
        }
    }
}

impl<T> CustomContext<T> for Option<T> {
    #[inline]
    fn context(self, message: Message) -> Result<T, Error> {
        match self {
            Some(value) => Ok(value),
            None => Err(Error {
                message,
                source: None,
            }),
        }
    }
}

impl Error {
    pub(crate) fn sources(&self) -> impl Iterator<Item = &dyn core::error::Error> {
        iter::successors(Some(self as &dyn core::error::Error), |e| e.source())
    }
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.message.fmt(f)
    }
}

impl fmt::Debug for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Error")
            .field("message", &self.message.to_string())
            .field("source", &self.source)
            .finish()
    }
}

impl From<web::Error> for Error {
    fn from(e: web::Error) -> Self {
        Error {
            message: Message::WebSocketError,
            source: Some(Box::new(Source::Web(e))),
        }
    }
}

#[derive(Debug)]
enum Source {
    Web(web::Error),
    #[allow(unused)]
    JsValue(JsValue),
}

impl From<web::Error> for Source {
    fn from(e: web::Error) -> Self {
        Source::Web(e)
    }
}

impl From<JsValue> for Source {
    fn from(e: JsValue) -> Self {
        Source::JsValue(e)
    }
}

impl core::error::Error for Error {
    fn source(&self) -> Option<&(dyn core::error::Error + 'static)> {
        match self.source.as_ref()?.as_ref() {
            Source::Web(e) => Some(e),
            Source::JsValue(..) => None,
        }
    }
}
