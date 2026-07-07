use std::rc::Rc;

use web_sys::{Document, Storage};
use yew::prelude::*;

use crate::error::{CustomContext, Error, Message};

const STORAGE_KEY: &str = "background";

/// Persists the selected page background in local storage. Created alongside the
/// router (see [`crate::root::Root`]) so the saved background is applied from the
/// very first render.
pub(super) struct BackgroundState {
    document: Option<Document>,
    storage: Option<Storage>,
    value: Option<String>,
    /// Fallback tab title used when no page sets its own. Driven by config.
    default_title: String,
    /// The title set by the current page, if any.
    current_title: Option<String>,
}

impl BackgroundState {
    pub(super) fn new() -> Self {
        let document = web_sys::window().and_then(|w| w.document());
        let storage = web_sys::window().and_then(|w| w.local_storage().ok().flatten());

        let value = storage
            .as_ref()
            .and_then(|s| s.get_item(STORAGE_KEY).ok().flatten())
            .filter(|s| !s.is_empty());

        Self {
            document,
            storage,
            value,
            default_title: "Track".to_owned(),
            current_title: None,
        }
    }

    /// Update the stored background, persisting it to local storage. Returns
    /// whether the value changed (and therefore a re-render is needed).
    pub(super) fn set_background(&mut self, value: String) -> Result<bool, Error> {
        if self.value.as_ref() == Some(&value) {
            return Ok(false);
        }

        if let Some(storage) = &self.storage {
            storage
                .set_item(STORAGE_KEY, &value)
                .context(Message::SetStorageItem)?;
        }

        self.value = Some(value);
        Ok(true)
    }

    pub(super) fn set_title(&mut self, title: Option<String>) {
        self.current_title = title;
        self.apply();
    }

    /// Set the fallback tab title used when no page sets its own. Expects an
    /// already-normalized value (see `App::title_from_config`).
    pub(super) fn set_default_title(&mut self, title: &str) {
        self.default_title = title.to_owned();
        self.apply();
    }

    fn apply(&self) {
        let Some(ref document) = self.document else {
            return;
        };

        document.set_title(self.current_title.as_deref().unwrap_or(&self.default_title));
    }

    /// The current background image URL, if any.
    pub(super) fn url(&self) -> Option<&str> {
        self.value.as_deref()
    }
}

struct Inner {
    background: Callback<String>,
    title: Callback<Option<String>>,
    error: Callback<Error>,
}

/// Context handed to descendant components so they can set the page background.
/// Backed by a callback into [`crate::root::Root`].
#[derive(Clone)]
pub(super) struct Background {
    inner: Rc<Inner>,
}

impl PartialEq for Background {
    #[inline]
    fn eq(&self, other: &Self) -> bool {
        Rc::ptr_eq(&self.inner, &other.inner)
    }
}

impl Background {
    pub(super) fn new(
        background: Callback<String>,
        title: Callback<Option<String>>,
        error: Callback<Error>,
    ) -> Self {
        Self {
            inner: Rc::new(Inner {
                background,
                title,
                error,
            }),
        }
    }

    /// Set the page background to the given (proxied) image URL, or clear it
    /// with `None`.
    pub(super) fn background(&self, url: Option<String>) {
        if let Some(url) = url {
            self.inner.background.emit(url);
        }
    }

    /// Set the title.
    pub(super) fn title(&self, title: Option<String>) {
        self.inner.title.emit(title);
    }

    /// Emit an error.
    pub(super) fn error(&self, error: Error) {
        self.inner.error.emit(error);
    }
}
