//! Keyboard focus for surfaces that hold it while open (modals and popovers):
//! move it in on open, keep Tab inside, and hand it back on close.

use gloo::timers::callback::Timeout;
use wasm_bindgen::JsCast;
use web_sys::{Element, HtmlElement, KeyboardEvent};

const FOCUSABLE: &str = "a[href], button:not([disabled]), input:not([disabled]), \
    select:not([disabled]), textarea:not([disabled]), [tabindex]:not([tabindex='-1'])";

/// The element that has focus now, to give it back later with [`restore`].
pub(crate) fn active() -> Option<HtmlElement> {
    web_sys::window()?
        .document()?
        .active_element()?
        .dyn_into()
        .ok()
}

/// Give focus back to `element` if it is still on the page.
pub(crate) fn restore(element: Option<HtmlElement>) {
    if let Some(element) = element
        && element.is_connected()
    {
        _ = element.focus();
    }
}

/// Everything inside `root` that Tab can reach, in document order.
fn focusables(root: &Element) -> Vec<HtmlElement> {
    let Ok(list) = root.query_selector_all(FOCUSABLE) else {
        return Vec::new();
    };

    (0..list.length())
        .filter_map(|i| list.item(i)?.dyn_into::<HtmlElement>().ok())
        .filter(|e| e.offset_parent().is_some())
        .filter(|e| e.closest("[inert]").ok().flatten().is_none())
        .collect()
}

/// Focus the first control inside `preferred`, else the first inside `root`,
/// else `root` itself.
///
/// Deferred until the current event is done: a surface opened by Enter would
/// otherwise receive the rest of that key press and activate its first control.
pub(crate) fn focus_first(root: &HtmlElement, preferred: Option<&Element>) {
    let root = root.clone();
    let preferred = preferred.cloned();

    Timeout::new(0, move || {
        if !root.is_connected() {
            return;
        }

        let first = preferred
            .as_ref()
            .and_then(|p| focusables(p).into_iter().next())
            .or_else(|| focusables(&root).into_iter().next());

        match first {
            Some(first) => _ = first.focus(),
            None => _ = root.focus(),
        }
    })
    .forget();
}

/// Keep Tab and Shift+Tab cycling inside `root`.
pub(crate) fn trap_tab(e: &KeyboardEvent, root: &HtmlElement) {
    if e.key() != "Tab" {
        return;
    }

    let items = focusables(root);

    let (Some(first), Some(last)) = (items.first(), items.last()) else {
        e.prevent_default();
        return;
    };

    let current = active();
    let inside = current.as_ref().is_some_and(|c| root.contains(Some(c)));

    if e.shift_key() {
        if !inside || current.as_ref() == Some(first) {
            e.prevent_default();
            _ = last.focus();
        }
    } else if !inside || current.as_ref() == Some(last) {
        e.prevent_default();
        _ = first.focus();
    }
}

/// Move between the `menuitem`s of a `role="menu"` list inside `root` with
/// the arrow keys, Home and End. Returns whether the key was handled.
pub(crate) fn move_in_menu(e: &KeyboardEvent, root: &HtmlElement) -> bool {
    let key = e.key();

    if !matches!(key.as_str(), "ArrowDown" | "ArrowUp" | "Home" | "End") {
        return false;
    }

    let Ok(Some(menu)) = root.query_selector("[role='menu']") else {
        return false;
    };

    let items = focusables(&menu);

    if items.is_empty() {
        return false;
    }

    let current = active().and_then(|a| items.iter().position(|i| *i == a));
    let last = items.len() - 1;

    let next = match (key.as_str(), current) {
        ("Home", _) | ("ArrowDown", None) => 0,
        ("ArrowDown", Some(i)) => (i + 1) % items.len(),
        ("ArrowUp", Some(i)) => i.checked_sub(1).unwrap_or(last),
        _ => last,
    };

    e.prevent_default();
    _ = items[next].focus();
    true
}
