use web_sys::Storage;
use yew::prelude::*;

const STORAGE_KEY: &str = "background";

/// Persists the selected page background in local storage. Created alongside the
/// router (see [`crate::root::Root`]) so the saved background is applied from the
/// very first render.
pub(super) struct BackgroundState {
    storage: Option<Storage>,
    value: Option<String>,
}

impl BackgroundState {
    pub(super) fn new() -> Self {
        let storage = web_sys::window().and_then(|w| w.local_storage().ok().flatten());

        let value = storage
            .as_ref()
            .and_then(|s| s.get_item(STORAGE_KEY).ok().flatten())
            .filter(|s| !s.is_empty());

        Self { storage, value }
    }

    /// Update the stored background, persisting it to local storage. Returns
    /// whether the value changed (and therefore a re-render is needed).
    pub(super) fn set(&mut self, value: String) -> bool {
        if self.value.as_ref() == Some(&value) {
            return false;
        }

        if let Some(storage) = &self.storage {
            let _ = storage.set_item(STORAGE_KEY, &value);
        }

        self.value = Some(value);
        true
    }

    /// The inline `--background` custom property for the current value, ready to
    /// drop onto the root element's `style` attribute.
    pub(super) fn style(&self) -> String {
        match &self.value {
            Some(url) => format!("--background: url('{url}')"),
            None => String::new(),
        }
    }
}

/// Context handed to descendant components so they can set the page background.
/// Backed by a callback into [`crate::root::Root`].
#[derive(Clone, PartialEq)]
pub(super) struct Background {
    set: Callback<String>,
}

impl Background {
    pub(super) fn new(set: Callback<String>) -> Self {
        Self { set }
    }

    /// Set the page background to the given (proxied) image URL, or clear it
    /// with `None`.
    pub(super) fn set(&self, url: Option<String>) {
        if let Some(url) = url {
            self.set.emit(url);
        }
    }
}
