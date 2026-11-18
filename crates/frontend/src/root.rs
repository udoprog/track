use auth::yew023::AuthProvider;
use phrasebook::Phrases;
use sockel::{Glyph, IconSet, OverlayHost};
use yew::prelude::*;

use crate::app::App;
use crate::class;
use crate::l10n;

const STYLESHEET: &str = include_str!(concat!(env!("OUT_DIR"), "/style.css"));

/// Signs the user in before the app, and its websocket, start.
#[function_component]
pub(super) fn Root() -> Html {
    let phrases = use_memo((), |()| l10n::initial());
    let icons = use_memo((), |()| icon_set());

    html! {
        <>
            <sockel::Styles css={STYLESHEET} />
            <ContextProvider<Phrases> context={(*phrases).clone()}>
                <ContextProvider<IconSet> context={(*icons).clone()}>
                    <OverlayHost>
                        <AuthProvider>
                            <App />
                        </AuthProvider>
                    </OverlayHost>
                </ContextProvider<IconSet>>
            </ContextProvider<Phrases>>
        </>
    }
}

/// sockel's glyphs drawn with the app's heroicons.
fn icon_set() -> IconSet {
    const GLYPHS: &[(Glyph, &str)] = &[
        (Glyph::Close, class::ICON_X_MARK),
        (Glyph::ChevronDown, class::ICON_CHEVRON_DOWN),
        (Glyph::ChevronUp, class::ICON_CHEVRON_UP),
        (Glyph::ChevronRight, class::ICON_CHEVRON_RIGHT),
        (Glyph::Check, class::ICON_CHECK),
        (Glyph::Plus, class::ICON_PLUS),
        (Glyph::Minus, class::ICON_MINUS),
        (Glyph::Spinner, class::ICON_ARROW_PATH),
        (Glyph::Grip, class::ICON_BARS_2),
        (Glyph::Ellipsis, class::ICON_ELLIPSIS_HORIZONTAL),
        (Glyph::Search, class::ICON_MAGNIFYING_GLASS),
        (Glyph::Eye, class::ICON_EYE),
        (Glyph::EyeOff, class::ICON_EYE_SLASH),
        (Glyph::Copy, class::ICON_CLIPBOARD),
    ];

    GLYPHS
        .iter()
        .fold(IconSet::default(), |set, &(glyph, name)| {
            set.with(
                glyph,
                html! { <span class={classes!(class::ICON, class::GLYPH, name)} /> },
            )
        })
}
