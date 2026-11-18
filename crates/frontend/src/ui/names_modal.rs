use yew::prelude::*;

use crate::class;
use crate::flags;
use crate::help;
use crate::ui::{Help, Modal};

#[derive(Properties, PartialEq)]
pub(crate) struct Props {
    /// The show's other names.
    pub(crate) names: Vec<api::AltName>,
    /// Each season with names, by its own name, with the names of the seasons
    /// of other numberings it covers.
    pub(crate) seasons: Vec<(String, Vec<api::SeasonAltNames>)>,
    pub(crate) on_close: Callback<()>,
}

/// A show's and its seasons' other names, with their languages and sources.
#[function_component]
pub(crate) fn NamesModal(props: &Props) -> Html {
    html! {
        <Modal icon={class::ICON_TAG} title="Other names" testid="names-modal" on_close={props.on_close.clone()}>
            <p class={classes!(class::NAMES_INTRO, class::TEXT_MUTED)}>
                {"Other titles this show and its seasons go by, with each one's language and where it comes from."}
                <Help section={help::NAMES} />
            </p>

            <div class={classes!(class::TRANSLATIONS, sockel::class::STACK, sockel::class::LOOSE)}>
                if !props.names.is_empty() {
                    <section>
                        <h3>{"Show"}</h3>
                        { view_names(&props.names) }
                    </section>
                }

                { for props.seasons.iter().map(|(season, groups)| html! {
                    <section>
                        <h3>{season.clone()}</h3>

                        { for groups.iter().map(|group| {
                            let label = api::xem_system_label(&group.target.system);

                            html! {
                                <>
                                    <h4 class={class::NAMES_TARGET} title={format!("{label} season {}", group.target.season)}>
                                        <span class={classes!(class::LOGO, class::logo(&group.target.system))} aria-hidden="true" />
                                        <span>{format!("{label} Season {}", group.target.season)}</span>
                                    </h4>

                                    { view_names(&group.names) }
                                </>
                            }
                        }) }
                    </section>
                }) }
            </div>
        </Modal>
    }
}

fn view_names(names: &[api::AltName]) -> Html {
    html! {
        <div class={classes!(class::TRANSLATION_ROWS, class::BESIDE)}>
            { for names.iter().map(view_name) }
        </div>
    }
}

/// A name with its language, which reads as unknown when its source gives no
/// code or one that names no language, and where it comes from.
fn view_name(n: &api::AltName) -> Html {
    let origin = n.origin.label();
    let language = n.language.and_then(|l| l.to_iso());

    let title = match &n.code {
        Some(code) => format!("{origin} language code: {code}"),
        None => format!("{origin} gives no language"),
    };

    html! {
        <div class={classes!(class::TRANSLATION_ROW, "alt-name")}>
            <span class={classes!(class::TRANSLATION_LANGUAGE, language.is_none().then_some(class::UNKNOWN))} {title}>
                if let Some(flag) = language.and_then(|l| l.flag).and_then(flags::flag_class) {
                    <span class={classes!(class::ITEM_INLINE, class::FLAG, flag)} aria-hidden="true" />
                }

                <span class="alt-name-language">{language.map_or("Unknown language", |l| l.name)}</span>
            </span>

            <span class="alt-name-text">
                {n.name.clone()}
                <span class={classes!(class::ALT_NAME_ORIGIN, class::LOGO, class::logo(n.origin.as_id()))} role="img" title={format!("From {origin}")} aria-label={format!("From {origin}")} />
            </span>
        </div>
    }
}
