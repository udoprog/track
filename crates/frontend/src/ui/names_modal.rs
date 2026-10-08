use yew::prelude::*;

use crate::ui::Modal;

#[derive(Properties, PartialEq)]
pub(crate) struct Props {
    /// The show's other names.
    pub(crate) names: Vec<api::AltName>,
    /// Each season with names, by its own name, with the names of the seasons
    /// of other numberings it covers.
    pub(crate) seasons: Vec<(String, Vec<api::SeasonAltNames>)>,
    pub(crate) on_close: Callback<()>,
}

/// XEM's other names for a show and its seasons, with their languages.
#[function_component]
pub(crate) fn NamesModal(props: &Props) -> Html {
    html! {
        <Modal icon="tag" title="Other names" class="names-modal" on_close={props.on_close.clone()}>
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
        <div class="translation-rows">
            { for names.iter().map(|n| html! {
                <div class="translation-row alt-name">
                    <span class="translation-language" title="Language">
                        {n.language.as_deref().map(str::to_uppercase).unwrap_or_default()}
                    </span>
                    <span class="alt-name-text">{n.name.clone()}</span>
                </div>
            }) }
        </div>
    }
}
