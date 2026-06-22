use web_sys::{Event, MouseEvent};
use yew::prelude::*;

use super::{AirDateFiltersEditor, LanguagePicker, Modal, ReleaseFiltersEditor};

#[derive(Properties, PartialEq)]
pub(crate) struct Props {
    pub(crate) title: AttrValue,
    pub(crate) language: api::Locale,
    pub(crate) has_images: bool,
    pub(crate) on_language_change: Callback<api::Locale>,
    pub(crate) on_edit_graphics: Callback<()>,
    pub(crate) on_edit_remotes: Callback<()>,
    pub(crate) on_close: Callback<()>,
    pub(crate) has_remotes: bool,
    pub(crate) last_synced: Option<AttrValue>,
    pub(crate) syncing: bool,
    pub(crate) on_sync: Callback<()>,
    pub(crate) auto_sync: bool,
    pub(crate) on_auto_sync_change: Callback<bool>,
    #[prop_or_default]
    pub(crate) include_specials: Option<bool>,
    #[prop_or_default]
    pub(crate) on_include_specials_change: Option<Callback<Option<bool>>>,
    #[prop_or_default]
    pub(crate) release_filters: Option<Vec<api::ReleaseFilter>>,
    #[prop_or_default]
    pub(crate) default_release_filters: Vec<api::ReleaseFilter>,
    #[prop_or_default]
    pub(crate) on_release_filters_change: Option<Callback<Option<Vec<api::ReleaseFilter>>>>,
    #[prop_or_default]
    pub(crate) air_date_filters: Option<Vec<api::AirDateFilter>>,
    #[prop_or_default]
    pub(crate) default_air_date_filters: Vec<api::AirDateFilter>,
    #[prop_or_default]
    pub(crate) on_air_date_filters_change: Option<Callback<Option<Vec<api::AirDateFilter>>>>,
}

#[function_component]
pub(crate) fn MediaSettingsModal(props: &Props) -> Html {
    let on_edit_graphics = props.on_edit_graphics.reform(|_: MouseEvent| ());
    let on_edit_remotes = props.on_edit_remotes.reform(|_: MouseEvent| ());
    let on_sync = props.on_sync.reform(|_: MouseEvent| ());

    let auto_sync = props.auto_sync;
    let on_auto_sync = props
        .on_auto_sync_change
        .reform(move |_: MouseEvent| !auto_sync);

    let specials = props.on_include_specials_change.as_ref().map(|cb| {
        let include_specials = props.include_specials;
        let value = match include_specials {
            None => "default",
            Some(true) => "include",
            Some(false) => "skip",
        };

        let on_change = cb.reform(|e: Event| {
            let select: web_sys::HtmlSelectElement = e.target_unchecked_into();
            match select.value().as_str() {
                "include" => Some(true),
                "skip" => Some(false),
                _ => None,
            }
        });

        html! {
            <div class="field">
                <label>{"Specials when syncing"}</label>
                <select class="input-select" onchange={on_change} {value}>
                    <option value="default" selected={include_specials.is_none()}>{"Default"}</option>
                    <option value="include" selected={include_specials == Some(true)}>{"Include"}</option>
                    <option value="skip" selected={include_specials == Some(false)}>{"Skip"}</option>
                </select>
            </div>
        }
    });

    let release = props.on_release_filters_change.as_ref().map(|cb| {
        let is_custom = props.release_filters.is_some();

        let on_mode = {
            let cb = cb.clone();
            let default = props.default_release_filters.clone();
            Callback::from(move |e: Event| {
                let select: web_sys::HtmlSelectElement = e.target_unchecked_into();
                match select.value().as_str() {
                    "custom" => cb.emit(Some(default.clone())),
                    _ => cb.emit(None),
                }
            })
        };

        let editor = props.release_filters.as_ref().map(|filters| {
            let on_change = cb.reform(|f: Vec<api::ReleaseFilter>| Some(f));
            html! {
                <ReleaseFiltersEditor filters={filters.clone()} on_change={on_change} />
            }
        });

        html! {
            <div class="field">
                <label>{"Release Date"}</label>

                <select class="input-select" onchange={on_mode}>
                    <option value="default" selected={!is_custom}>{"Default"}</option>
                    <option value="custom" selected={is_custom}>{"Customize"}</option>
                </select>

                {editor}
            </div>
        }
    });

    let air_dates = props.on_air_date_filters_change.as_ref().map(|cb| {
        let is_custom = props.air_date_filters.is_some();

        let on_mode = {
            let cb = cb.clone();
            let default = props.default_air_date_filters.clone();
            Callback::from(move |e: Event| {
                let select: web_sys::HtmlSelectElement = e.target_unchecked_into();
                match select.value().as_str() {
                    "custom" => cb.emit(Some(default.clone())),
                    _ => cb.emit(None),
                }
            })
        };

        let editor = props.air_date_filters.as_ref().map(|filters| {
            let on_change = cb.reform(|f: Vec<api::AirDateFilter>| Some(f));
            html! {
                <AirDateFiltersEditor filters={filters.clone()} on_change={on_change} />
            }
        });

        html! {
            <div class="field">
                <label>{"Air Date"}</label>

                <select class="input-select" onchange={on_mode}>
                    <option value="default" selected={!is_custom}>{"Default"}</option>
                    <option value="custom" selected={is_custom}>{"Customize"}</option>
                </select>

                {editor}
            </div>
        }
    });

    html! {
        <Modal title={props.title.clone()} on_close={props.on_close.reform(|_| ())}>
            <div class="form">
                <div class="field">
                    <label>{"Language"}</label>

                    <LanguagePicker
                        current={props.language}
                        placeholder="Default"
                        on_change={props.on_language_change.clone()}
                    />
                </div>

                <div class="field">
                    <label>{"Automatic sync"}</label>
                    <span class={classes!("input-checkbox", auto_sync.then_some("checked"))} id="auto-sync-enabled" onclick={on_auto_sync}>
                        <span class="mark" />
                        {if auto_sync { "Enabled" } else { "Disabled" }}
                    </span>
                </div>

                {specials}

                {release}

                {air_dates}

                <div class="field">
                    <label>{"Sync"}</label>
                    <div class="input-group">
                        if let Some(ref ts) = props.last_synced {
                            <div class="input-text fill" title="Last synced at">{ts}</div>
                        } else {
                            <div class="input-text fill text-muted">{"Never synced"}</div>
                        }

                        if props.has_remotes {
                            <button class="btn" onclick={on_sync} title="Sync now">
                                <span class={classes!("icon", "arrow-path", props.syncing.then_some("spin"))} />
                            </button>
                        }
                    </div>
                </div>

                <div class="field">
                    if props.has_images {
                        <button class="btn" onclick={on_edit_graphics}>
                            <span class="icon photo" />
                            <span>{"Graphics"}</span>
                        </button>

                        <span class="hint">{"Choose the poster, backdrop, banner, and other artwork."}</span>
                    } else {
                        <span class="hint">{"No graphics available. Sync to fetch artwork."}</span>
                    }
                </div>

                <div class="field">
                    <button class="btn" onclick={on_edit_remotes}>
                        <span class="icon identification" />
                        <span>{"Remotes"}</span>
                    </button>

                    <span class="hint">{"Edit the TMDB, TVDB, and other remote identifiers used to sync."}</span>
                </div>
            </div>
        </Modal>
    }
}
