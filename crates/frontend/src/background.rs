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

    pub(super) fn set_title(&self, title: Option<String>) {
        let Some(ref document) = self.document else {
            return;
        };

        if let Some(title) = title {
            document.set_title(&title);
        } else {
            document.set_title("Track");
        }
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
