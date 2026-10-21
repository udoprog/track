//! The phrasebook the interface's text is looked up in: track's catalog, then
//! watchword's for the sign-in, account and user-management components.

use phrasebook::{Catalog, Lang, Phrasebook, Phrases, Source};

static L10N: Catalog = Catalog::new(
    "track",
    &[Source::new(
        Lang::EN_US,
        include_str!("../locales/en-US.ftl"),
    )],
);

/// Every shipped language, over track's catalog and then watchword's,
/// falling back to English.
fn phrasebook() -> Phrasebook {
    Phrasebook::builder()
        .language(Lang::EN_US, "English")
        .fallback(Lang::EN_US)
        .catalog(&L10N)
        .catalog(&auth::yew023::L10N)
        .build()
}

/// A handle open at the browser's preferred language, or English.
pub(crate) fn initial() -> Phrases {
    let book = phrasebook();
    let lang = phrasebook::browser::initial_lang(&book, phrasebook::browser::DEFAULT_KEY);
    book.phrases(lang)
}

#[cfg(test)]
mod tests {
    use phrasebook::check;

    use super::*;

    #[test]
    fn the_catalog_is_sound() {
        check::catalog(&L10N).unwrap();
    }

    #[test]
    fn watchwords_messages_resolve() {
        let phrases = phrasebook().phrases(Lang::EN_US);
        assert_eq!(
            phrases.try_text("watchword-sign-in").as_deref(),
            Some("Sign in")
        );
    }

    #[test]
    fn a_preference_matches_on_the_primary_subtag() {
        let book = phrasebook();
        assert_eq!(book.negotiate(["en-GB"]), Lang::EN_US);
        assert_eq!(book.negotiate(["sv-SE"]), Lang::EN_US);
    }
}
