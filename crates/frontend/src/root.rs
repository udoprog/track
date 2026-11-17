use auth::yew023::AuthProvider;
use phrasebook::Phrases;
use sockel::{Glyph, IconSet, OverlayHost};
use yew::prelude::*;

use crate::app::App;
use crate::l10n;

/// Signs the user in before the app, and its websocket, start.
#[function_component]
pub(super) fn Root() -> Html {
    let phrases = use_memo((), |()| l10n::initial());
    let icons = use_memo((), |()| icon_set());

    html! {
        <ContextProvider<Phrases> context={(*phrases).clone()}>
            <ContextProvider<IconSet> context={(*icons).clone()}>
                <OverlayHost>
                    <AuthProvider>
                        <App />
                    </AuthProvider>
                </OverlayHost>
            </ContextProvider<IconSet>>
        </ContextProvider<Phrases>>
    }
}

/// sockel's glyphs drawn with the app's heroicons.
fn icon_set() -> IconSet {
    const GLYPHS: &[(Glyph, &str)] = &[
        (Glyph::Close, "x-mark"),
        (Glyph::ChevronDown, "chevron-down"),
        (Glyph::ChevronUp, "chevron-up"),
        (Glyph::ChevronRight, "chevron-right"),
        (Glyph::Check, "check"),
        (Glyph::Plus, "plus"),
        (Glyph::Minus, "minus"),
        (Glyph::Spinner, "arrow-path"),
        (Glyph::Grip, "bars-2"),
        (Glyph::Ellipsis, "ellipsis-horizontal"),
        (Glyph::Search, "magnifying-glass"),
        (Glyph::Eye, "eye"),
        (Glyph::EyeOff, "eye-slash"),
        (Glyph::Copy, "clipboard"),
    ];

    GLYPHS
        .iter()
        .fold(IconSet::default(), |set, &(glyph, name)| {
            set.with(
                glyph,
                html! { <span class={classes!("icon", "glyph", name)} /> },
            )
        })
}
