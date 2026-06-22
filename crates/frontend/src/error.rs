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

/// One variant per distinct operation, so a failure's displayed message points
/// at exactly what was being attempted. Variants carry the relevant parameters
/// when the failing handler already has them in scope, to aid troubleshooting.
#[derive(Debug, Clone, Display)]
pub(crate) enum Message {
    #[display("WebSocket Error")]
    WebSocketError,
    #[display("Missing window")]
    MissingWindow,
    #[display("Missing history API")]
    MissingHistory,
    #[display("Reading location pathname")]
    ReadingPathname,
    #[display("Pushing browser history state")]
    PushState,
    #[display("Replacing browser history state")]
    ReplaceState,
    #[display("Loading pending")]
    LoadingPending,
    #[display("Loading schedule")]
    LoadingSchedule,
    #[display("Loading config")]
    LoadingConfig,
    #[display("Loading show")]
    LoadingShow,
    #[display("Loading seasons")]
    LoadingSeasons,
    #[display("Loading episodes")]
    LoadingEpisodes,
    #[display("Loading translations")]
    LoadingTranslations,
    #[display("Loading season images")]
    LoadingSeasonImages,
    #[display("Marking watched")]
    MarkingWatched,
    #[display("Moving watch entry")]
    MovingWatched,
    #[display("Skipping episode")]
    SkippingEpisode,
    #[display("Loading movies")]
    LoadingMovies,
    #[display("Loading watch history")]
    LoadingWatched,
    #[display("Saving config")]
    SavingConfig,
    #[display("Removing watch")]
    RemovingWatched,
    #[display("Removing movie")]
    RemovingMovie,
    #[display("Untracking show")]
    UntrackingShow,
    #[display("Updating movie tracking")]
    UntrackingMovie,
    #[display("Removing show")]
    RemovingShow,
    #[display("Syncing show")]
    SyncingShow,
    #[display("Syncing movie")]
    SyncingMovie,
    #[display("Syncing all media")]
    SyncingAll,
    #[display("Toggling remote source")]
    SettingRemoteEnabled,
    #[display("Setting remote sync kinds")]
    SettingRemoteSyncKinds,
    #[display("Reordering remotes")]
    ReorderingRemotes,
    #[display("Editing remotes")]
    EditingRemotes,
    #[display("Adding to up next")]
    AddingPending,
    #[display("Removing from up next")]
    RemovingPending,
    #[display("Selecting image")]
    SelectingImage,
    #[display("Clearing image")]
    ClearingImage,
    #[display("Setting language to {_0:?}")]
    SettingLanguage(api::Locale),
    #[display("Setting release dates")]
    SettingReleaseFilters,
    #[display("Setting specials handling to {_0:?}")]
    SettingIncludeSpecials(Option<bool>),
    #[display("Setting air dates")]
    SettingAirDateFilters,
    #[display("Setting automatic sync to {_0}")]
    SettingAutoSync(bool),
    #[display("Loading tasks")]
    LoadingTasks,
    #[display("Searching")]
    Searching,
    #[display("Tracking show")]
    TrackingShow,
    #[display("Tracking movie")]
    TrackingMovie,
    #[display("Setting outline style")]
    SetOutlineStyle,
    #[display("Capturing outline pointer")]
    CapturingOutlinePointer,
    #[display("Releasing outline pointer")]
    ReleasingOutlinePointer,
    #[display("Reading viewport size")]
    ReadingViewport,
    #[display("Positioning the time menu")]
    PositioningMenu,
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
