const STORAGE_KEY: &str = "theme";

/// Apply the theme to the document root. `System` removes the attribute so
/// `prefers-color-scheme` decides. The choice is also kept in local storage so
/// `index.html` can apply it before the application has loaded.
pub(crate) fn apply(theme: api::ThemeType) {
    let Some(window) = web_sys::window() else {
        return;
    };

    if let Some(root) = window.document().and_then(|d| d.document_element()) {
        _ = match theme {
            api::ThemeType::System => root.remove_attribute("data-theme"),
            theme => root.set_attribute("data-theme", &theme.to_string()),
        };
    }

    if let Ok(Some(storage)) = window.local_storage() {
        _ = storage.set_item(STORAGE_KEY, &theme.to_string());
    }
}
