use yew::prelude::*;

use plinth::{Reorder, ReorderHandle, ReorderItem};

use super::Button;

#[derive(Properties, PartialEq)]
pub(crate) struct Props {
    pub(crate) prefs: Vec<api::NumberingPref>,
    pub(crate) on_change: Callback<Vec<api::NumberingPref>>,
}

/// What a numbering is, beside its name.
fn description(system: &str) -> Option<&'static str> {
    match system {
        "tvdb" => Some("aired order"),
        "scene" => Some("release-group numbering"),
        "anidb" => Some("one season per AniDB entry"),
        "rage" => Some("defunct"),
        _ => None,
    }
}

/// The numberings episodes show besides their own, in order, each shown or
/// hidden; rows move by dragging.
#[function_component]
pub(crate) fn NumberingsEditor(props: &Props) -> Html {
    let prefs = api::numbering_order(&props.prefs);

    let on_move = {
        let prefs = prefs.clone();

        props.on_change.reform(move |(from, to): (usize, usize)| {
            let mut prefs = prefs.clone();
            let pref = prefs.remove(from);
            prefs.insert(to, pref);
            prefs
        })
    };

    html! {
        <Reorder class={classes!("form", "numberings")} {on_move} quick_keys=true titles=true>
            {
                for prefs.iter().enumerate().map(|(index, pref)| {
                    let label = api::xem_system_label(&pref.system);
                    let shown = pref.shown;
                    // A hidden numbering's row is quieter.
                    let dim = classes!((!shown).then_some("dim"));

                    let on_toggle = {
                        let prefs = prefs.clone();
                        props.on_change.reform(move |_: MouseEvent| {
                            let mut prefs = prefs.clone();
                            prefs[index].shown = !shown;
                            prefs
                        })
                    };

                    html_nested! {
                        <ReorderItem id={pref.system.clone()} name={label.to_owned()} class={classes!("input-group", "plinth-cluster", "plinth-nowrap", (!shown).then_some("numbering-hidden"))}>
                            <ReorderHandle class={dim.clone()} />

                            <span class={classes!("input-label", "has-text", dim.clone())}>
                                <span class={classes!("logo", pref.system.clone())} data-system={pref.system.clone()} />
                            </span>

                            <span class={classes!("input-label", "has-text", "grow", "plinth-cluster", "plinth-tight", dim.clone())}>
                                <span>{label}</span>

                                if let Some(description) = description(&pref.system) {
                                    <span class="text-muted">{description}</span>
                                }
                            </span>

                            <Button
                                icon={if shown { "check" } else { "x-mark" }}
                                label={if shown { "Shown" } else { "Hidden" }}
                                title={format!("{} {label} numbers on episodes", if shown { "Showing" } else { "Hiding" })}
                                pressed={Some(shown)}
                                onclick={on_toggle}
                            />
                        </ReorderItem>
                    }
                })
            }
        </Reorder>
    }
}
