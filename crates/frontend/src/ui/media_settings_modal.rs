use api::IncludeSpecials;
use web_sys::{Event, MouseEvent};
use yew::prelude::*;

use super::{
    AIR_DATE_KINDS, AIR_DATE_SOURCES, FiltersEditor, LanguagePicker, Modal, RELEASE_KINDS,
    RELEASE_SOURCES,
};

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
    pub(crate) include_specials: IncludeSpecials,
    #[prop_or_default]
    pub(crate) on_include_specials_change: Callback<IncludeSpecials>,
    #[prop_or_default]
    pub(crate) release_filters: Option<Vec<api::FilterRule>>,
    #[prop_or_default]
    pub(crate) default_release_filters: Vec<api::FilterRule>,
    #[prop_or_default]
    pub(crate) on_release_filters_change: Option<Callback<Option<Vec<api::FilterRule>>>>,
    #[prop_or_default]
    pub(crate) air_date_filters: Option<Vec<api::FilterRule>>,
    #[prop_or_default]
    pub(crate) default_air_date_filters: Vec<api::FilterRule>,
    #[prop_or_default]
    pub(crate) on_air_date_filters_change: Option<Callback<Option<Vec<api::FilterRule>>>>,
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

    let include_specials = props.include_specials;

    let special_on_change = |value: IncludeSpecials| {
        props
            .on_include_specials_change
            .reform(move |_| value.cycle())
    };

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
            let on_change = cb.reform(|f: Vec<api::FilterRule>| Some(f));
            html! {
                <FiltersEditor rules={filters.clone()} on_change={on_change} kinds={RELEASE_KINDS} sources={RELEASE_SOURCES} />
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
        let default = props.default_air_date_filters.clone();
        let on_mode = cb.reform(move |_| (!is_custom).then(|| default.clone()));

        let editor = props.air_date_filters.as_ref().map(|filters| {
            let on_change = cb.reform(|f: Vec<api::FilterRule>| Some(f));

            html! {
                <FiltersEditor rules={filters.clone()} on_change={on_change} kinds={AIR_DATE_KINDS} sources={AIR_DATE_SOURCES} />
            }
        });

        html! {
            <>
                <div class="input-group">
                    <span class="input-label has-text">{"Air Date"}</span>

                    <button class="input-checkbox has-text fill" onclick={on_mode}>
                        {if is_custom { "Custom" } else { "Use global default" }}
                    </button>
                </div>

                {editor}
            </>
        }
    });

    html! {
        <Modal icon="cog-6-tooth" title={props.title.clone()} on_close={props.on_close.reform(|_| ())}>
            <div class="form">
                <div class="field">
                    <label>{"Language"}</label>

                    <LanguagePicker
                        current={props.language}
                        placeholder="Default"
                        on_change={props.on_language_change.clone()}
                    />
                </div>

                <div class="input-group">
                    <div class="input-label has-text">{"Automatic Sync"}</div>
                    <div class={classes!("input-checkbox", "has-text", "fill", auto_sync.then_some("checked"))} id="auto-sync-enabled" onclick={on_auto_sync}>
                        <span class="mark" />
                        {if auto_sync { "Enabled" } else { "Disabled" }}
                    </div>
                </div>

                <div class="input-group">
                    <span class="input-label has-text">{"Include Specials"}</span>
                    <div class="input-checkbox has-text fill" onclick={special_on_change(include_specials)}>
                        {include_specials.as_label()}
                    </div>
                </div>

                {release}

                {air_dates}

                <div class="input-group">
                    <span class="input-label has-text">{"Last Sync"}</span>

                    if let Some(ref ts) = props.last_synced {
                        <div class="input-text has-text fill" title="Last synced at">
                            <span>{ts}</span>
                        </div>
                    } else {
                        <div class="input-text has-text fill text-muted">
                            <span>{"Never synced"}</span>
                        </div>
                    }

                    if props.has_remotes {
                        <button onclick={on_sync} title="Sync now">
                            <span class={classes!("icon", "arrow-path", props.syncing.then_some("spin"))} />
                        </button>
                    }
                </div>

                <div class="field">
                    if props.has_images {
                        <button class="has-text" onclick={on_edit_graphics}>
                            <span class="icon photo" />
                            <span>{"Graphics"}</span>
                        </button>

                        <span class="hint">{"Choose the poster, backdrop, banner, and other artwork."}</span>
                    } else {
                        <span class="hint">{"No graphics available. Sync to fetch artwork."}</span>
                    }
                </div>

                <div class="field">
                    <button class="has-text" onclick={on_edit_remotes}>
                        <span class="icon identification" />
                        <span>{"Remotes"}</span>
                    </button>

                    <span class="hint">{"Edit the TMDB, TVDB, and other remote identifiers used to sync."}</span>
                </div>
            </div>
        </Modal>
    }
}
