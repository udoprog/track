use yew::prelude::*;

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
        <Modal icon="tag" title="Other names" class="names-modal" on_close={props.on_close.clone()}>
            <p class="names-intro text-muted">
                {"Other titles this show and its seasons go by, with each one's language and where it comes from."}
                <Help section={help::NAMES} />
            </p>

            <div class="translations">
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
                                    <h4 class="names-target" title={format!("{label} season {}", group.target.season)}>
                                        <span class={classes!("logo", group.target.system.clone())} aria-hidden="true" />
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
        <div class="translation-rows beside">
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
        <div class="translation-row alt-name">
            <span class={classes!("translation-language", language.is_none().then_some("unknown"))} {title}>
                if let Some(flag) = language.and_then(|l| l.flag).filter(|flag| flags::has_flag(flag)) {
                    <span class={classes!("item-inline", "flag", flag)} aria-hidden="true" />
                }

                <span class="alt-name-language">{language.map_or("Unknown language", |l| l.name)}</span>
            </span>

            <span class="alt-name-text">
                {n.name.clone()}
                <span class={classes!("alt-name-origin", "logo", n.origin.as_id())} role="img" title={format!("From {origin}")} aria-label={format!("From {origin}")} />
            </span>
        </div>
    }
}
