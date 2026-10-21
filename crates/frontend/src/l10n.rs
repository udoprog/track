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
        .catalog(&L10N)
        .catalog(&auth::yew023::L10N)
        .build()
}

/// A handle open at the browser's preferred language, or English.
pub(crate) fn initial() -> Phrases {
    phrasebook::browser::initial(&phrasebook())
}

#[cfg(test)]
mod tests {
    use phrasebook::check;

    use super::*;

    #[test]
    fn the_catalog_is_sound() {
        check::all(&L10N, &[&auth::yew023::L10N]).unwrap();
    }

    #[test]
    fn the_catalogs_are_in_the_phrasebook() {
        let book = phrasebook();
        assert!(book.has_catalog(&L10N));
        assert!(book.has_catalog(&auth::yew023::L10N));
    }

    #[test]
    fn a_preference_matches_on_the_primary_subtag() {
        let book = phrasebook();
        assert_eq!(book.negotiate(["en-GB"]), Lang::EN_US);
        assert_eq!(book.negotiate(["sv-SE"]), Lang::EN_US);
    }
}
