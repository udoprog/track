use musli_web::web03::prelude::*;
use yew::prelude::*;

use crate::SetupChannel;
use crate::background::Background;
use crate::error::{CustomContext, Error, Message};
use crate::ui::{
    AIR_DATE_KINDS, AIR_DATE_SOURCES, Button, DurationInput, FiltersEditor, FormRow,
    LanguagePicker, RELEASE_KINDS, RELEASE_SOURCES, SecretInput, Skeleton, SyncKindsEditor,
    SyncLanguagesEditor,
};

fn tz_is_valid(name: &str) -> bool {
    name.is_empty() || jiff_tzdb::get(name).is_some()
}

pub(crate) struct Settings {
    channel: ws::Channel,
    background: Background,
    config: api::Config,
    /// Whether the real config has loaded; until then fields render as skeletons
    /// rather than flashing default values.
    loaded: bool,
    _setup: SetupChannel,
    _broadcast: ws::Listener,
    _config_req: ws::Request,
    _save_req: ws::Request,
}

pub(crate) enum Msg {
    Channel(Result<ws::Channel, ws::Error>),
    AppBroadcast(Result<ws::Packet<api::AppBroadcast>, ws::Error>),
    ConfigLoaded(Result<ws::Packet<api::GetConfig>, ws::Error>),
    ThemeChanged(api::ThemeType),
    TvdbKeyChanged(String),
    TvdbPinChanged(String),
    TmdbKeyChanged(String),
    PageTitleChanged(String),
    TimezoneChanged(String),
    LanguageChanged(api::Locale),
    SyncLanguagesChanged(Vec<api::Locale>),
    DashboardPageChanged(String),
    DashboardLookaheadChanged(api::Duration),
    AutoSyncEnabledToggle,
    AutoSyncIntervalChanged(String),
    IncludeSpecialsToggle,
    ReleaseFiltersChanged(api::FilterRules),
    AirDateFiltersChanged(api::FilterRules),
    SyncKindsChanged(Vec<api::SourceSyncKinds>),
    SaveDone(Result<ws::Packet<api::SetConfig>, ws::Error>),
}

impl Component for Settings {
    type Message = Msg;
    type Properties = ();

    fn create(ctx: &Context<Self>) -> Self {
        let (ws, _) = ctx
            .link()
            .context::<ws::Handle>(Callback::noop())
            .expect("Expected ws::Handle in context");

        let _setup = SetupChannel::new(ws.clone(), ctx.link().callback(Msg::Channel));
        let _broadcast = ws.on_broadcast(ctx.link().callback(Msg::AppBroadcast));

        let (background, _) = ctx
            .link()
            .context::<Background>(Callback::noop())
            .expect("Expected background handle in context");

        Self {
            channel: ws::Channel::default(),
            background,
            config: api::Config::default(),
            loaded: false,
            _setup,
            _broadcast,
            _config_req: ws::Request::default(),
            _save_req: ws::Request::default(),
        }
    }

    fn update(&mut self, ctx: &Context<Self>, msg: Self::Message) -> bool {
        match self.try_update(ctx, msg) {
            Ok(render) => render,
            Err(e) => {
                self.background.error(e);
                false
            }
        }
    }

    fn rendered(&mut self, _ctx: &Context<Self>, first_render: bool) {
        if first_render {
            self.background.title(Some("Settings".to_string()));
        }
    }

    fn destroy(&mut self, _ctx: &Context<Self>) {
        self.background.title(None);
    }

    fn view(&self, ctx: &Context<Self>) -> Html {
        let link = ctx.link();

        let on_theme = link.callback(|e: Event| {
            let select: web_sys::HtmlSelectElement = e.target_unchecked_into();
            let theme = match select.value().as_str() {
                "light" => api::ThemeType::Light,
                "system" => api::ThemeType::System,
                _ => api::ThemeType::Dark,
            };
            Msg::ThemeChanged(theme)
        });

        // Text and number fields commit on `change` (blur/Enter) rather than
        // `input`, so a setting is persisted once the user finishes editing it
        // instead of on every keystroke.
        let on_dashboard_page = link.callback(|e: Event| {
            let input: web_sys::HtmlInputElement = e.target_unchecked_into();
            Msg::DashboardPageChanged(input.value())
        });

        let on_page_title = link.callback(|e: Event| {
            let input: web_sys::HtmlInputElement = e.target_unchecked_into();
            Msg::PageTitleChanged(input.value())
        });

        // Tab takes the best matching zone, as a shell completes a name; the
        // browser's own suggestion list only takes the arrow keys.
        let on_timezone_key = link.batch_callback(|e: KeyboardEvent| {
            if e.key() != "Tab" || e.shift_key() {
                return None;
            }

            let input: web_sys::HtmlInputElement = e.target_unchecked_into();
            let typed = input.value();
            let zone = complete_timezone(&typed).filter(|zone| *zone != typed)?;

            // Stay in the field so the completion is seen; Tab again moves on.
            e.prevent_default();
            input.set_value(zone);
            Some(Msg::TimezoneChanged(zone.to_owned()))
        });

        let on_timezone = link.callback(|e: Event| {
            let input: web_sys::HtmlInputElement = e.target_unchecked_into();
            Msg::TimezoneChanged(input.value())
        });

        let on_auto_sync_toggle = link.callback(|_| Msg::AutoSyncEnabledToggle);
        let on_include_specials_change = link.callback(|_| Msg::IncludeSpecialsToggle);

        let on_auto_sync_interval = link.callback(|e: Event| {
            let input: web_sys::HtmlInputElement = e.target_unchecked_into();
            Msg::AutoSyncIntervalChanged(input.value())
        });

        let theme_val = self.config.theme.to_string();

        let tz_valid = tz_is_valid(&self.config.timezone);
        let auto_sync = self.config.auto_sync_enabled;
        let specials = self.config.include_specials;

        html! {
            <>
                <h1>{"Settings"}</h1>

                <div class="settings">
                    <section>
                        <h2>{"Appearance"}</h2>

                        <div class="form-rows">
                            <FormRow label="Theme">
                                { self.field_slot("", html! {
                                    <select class="input-select" data-test="theme" title="Theme" onchange={on_theme} value={theme_val}>
                                        <option value="dark" selected={self.config.theme == api::ThemeType::Dark}>{"Dark"}</option>
                                        <option value="light" selected={self.config.theme == api::ThemeType::Light}>{"Light"}</option>
                                        <option value="system" selected={self.config.theme == api::ThemeType::System}>{"System"}</option>
                                    </select>
                                }) }
                            </FormRow>

                            <FormRow label="Page title">
                                { self.field_slot("", html! {
                                    <input class="input-text fill" type="text" title="Page title" placeholder="Track" value={self.config.page_title.clone()} onchange={on_page_title} autocomplete="off" />
                                }) }
                            </FormRow>

                            <FormRow label="Time zone" hint="Leave empty to use the browser's time zone.">
                                { self.field_slot("", html! {
                                    <input class="input-text fill" type="text" title="Time zone" placeholder="Browser time zone" value={self.config.timezone.clone()} onchange={on_timezone} onkeydown={on_timezone_key} list="tz-datalist" autocomplete="off" />
                                }) }

                                <datalist id="tz-datalist">
                                    { for jiff_tzdb::available().map(|name| html! {
                                        <option value={name} />
                                    }) }
                                </datalist>

                                if !tz_valid {
                                    <span class="field-error" role="alert">{"Unknown time zone"}</span>
                                }
                            </FormRow>

                            <FormRow label="Up next per page" hint="How many cards What's Next shows on a page.">
                                { self.field_slot("", html! {
                                    <input
                                        type="number"
                                        class="input-number"
                                        title="Up next per page"
                                        min="1"
                                        max="100"
                                        value={self.config.dashboard_page.to_string()}
                                        onchange={on_dashboard_page}
                                    />
                                }) }
                            </FormRow>

                            <FormRow label="Up next look-ahead" hint="How far ahead What's Next shows episodes that have not aired yet.">
                                { self.field_slot("", html! {
                                    <div class="input-group">
                                        <DurationInput
                                            value={self.config.dashboard_lookahead}
                                            on_change={link.callback(Msg::DashboardLookaheadChanged)}
                                        />
                                    </div>
                                }) }
                            </FormRow>
                        </div>
                    </section>

                    <section>
                        <h2>{"Language"}</h2>

                        <div class="form-rows">
                            <FormRow label="Language" hint="The default language for shows and movies.">
                                { self.field_slot("", html! {
                                    <LanguagePicker
                                        current={self.config.language}
                                        placeholder="Default"
                                        on_change={link.callback(Msg::LanguageChanged)}
                                    />
                                }) }
                            </FormRow>

                            <FormRow label="Sync languages" hint="Translations to fetch; titles can be searched and filtered in these languages.">
                                { self.field_slot("tall", html! {
                                    <SyncLanguagesEditor
                                        languages={self.config.sync_languages.clone()}
                                        on_change={link.callback(Msg::SyncLanguagesChanged)}
                                    />
                                }) }
                            </FormRow>
                        </div>
                    </section>

                    <section>
                        <h2>{"Sync"}</h2>

                        <div class="form-rows">
                            <FormRow label="Automatic sync">
                                { self.field_slot("", html! {
                                    <Button class={classes!("input-checkbox", "has-text", auto_sync.then_some("checked"))} role="switch" checked={Some(auto_sync)} title="Automatic sync" onclick={on_auto_sync_toggle}>
                                        <span class="mark" />
                                        <span>{if auto_sync { "Enabled" } else { "Disabled" }}</span>
                                    </Button>
                                }) }
                            </FormRow>

                            <FormRow label="Sync every">
                                { self.field_slot("", html! {
                                    <div class="input-group">
                                        <input
                                            type="number"
                                            class="input-number"
                                            title="Sync every"
                                            min="1"
                                            max="168"
                                            value={self.config.auto_sync_interval_hours.to_string()}
                                            onchange={on_auto_sync_interval}
                                        />

                                        <span class="input-label has-text">{"hours"}</span>
                                    </div>
                                }) }
                            </FormRow>

                            <FormRow label="Specials in What's Next">
                                { self.field_slot("", html! {
                                    <Button class={classes!("input-checkbox", "has-text", specials.then_some("checked"))} role="switch" checked={Some(specials)} title="Specials for Watch Next" onclick={on_include_specials_change}>
                                        <span class="mark" />
                                        <span>{if specials { "Included" } else { "Skipped" }}</span>
                                    </Button>
                                }) }
                            </FormRow>

                            <FormRow label="Sync sources" hint="What each source contributes by default, in priority order (top wins). Base covers titles, overviews and episodes; air dates merge in this order; graphics come from every source. Shows and movies can override this per remote.">
                                { self.field_slot("tall", html! {
                                    <SyncKindsEditor
                                        kinds={self.config.sync_kinds.clone()}
                                        on_change={link.callback(Msg::SyncKindsChanged)}
                                    />
                                }) }
                            </FormRow>

                            <FormRow label="Release dates" hint="Which release dates count. A date matching any rule is considered, in the rules' order.">
                                { self.field_slot("tall", html! {
                                    <FiltersEditor
                                        rules={self.config.release_filters.clone()}
                                        on_change={link.callback(Msg::ReleaseFiltersChanged)}
                                        kinds={RELEASE_KINDS}
                                        sources={RELEASE_SOURCES}
                                    />
                                }) }
                            </FormRow>

                            <FormRow label="Air dates" hint="Which air dates count. A date matching any rule is considered, in the rules' order.">
                                { self.field_slot("tall", html! {
                                    <FiltersEditor
                                        rules={self.config.air_date_filters.clone()}
                                        on_change={link.callback(Msg::AirDateFiltersChanged)}
                                        kinds={AIR_DATE_KINDS}
                                        sources={AIR_DATE_SOURCES}
                                    />
                                }) }
                            </FormRow>
                        </div>
                    </section>

                    <section>
                        <h2>{"API keys"}</h2>

                        <div class="form-rows">
                            <FormRow label="TheTVDB API key">
                                { self.field_slot("", html! {
                                    <SecretInput
                                        id="tvdb-api-key"
                                        placeholder="Enter TVDB API key"
                                        value={self.config.tvdb_api_key.clone()}
                                        on_change={link.callback(Msg::TvdbKeyChanged)}
                                    />
                                }) }
                            </FormRow>

                            <FormRow label="TheTVDB subscriber PIN" hint="Optional.">
                                { self.field_slot("", html! {
                                    <SecretInput
                                        id="tvdb-pin"
                                        placeholder="Enter TVDB subscriber PIN"
                                        value={self.config.tvdb_pin.clone().unwrap_or_default()}
                                        on_change={link.callback(Msg::TvdbPinChanged)}
                                    />
                                }) }
                            </FormRow>

                            <FormRow label="TheMovieDB API key">
                                { self.field_slot("", html! {
                                    <SecretInput
                                        id="tmdb-api-key"
                                        placeholder="Enter TMDB API key"
                                        value={self.config.tmdb_api_key.clone()}
                                        on_change={link.callback(Msg::TmdbKeyChanged)}
                                    />
                                }) }
                            </FormRow>
                        </div>
                    </section>
                </div>
            </>
        }
    }
}

impl Settings {
    /// Render `control` once the config has loaded, or a skeleton placeholder of
    /// the given size class while it is still loading, so a field keeps its
    /// static label/hint without flashing a default value.
    fn field_slot(&self, skeleton: &'static str, control: Html) -> Html {
        if self.loaded {
            control
        } else {
            html! { <Skeleton class={classes!(skeleton)} /> }
        }
    }

    fn try_update(&mut self, ctx: &Context<Self>, msg: Msg) -> Result<bool, Error> {
        match msg {
            Msg::Channel(result) => {
                self.channel = result?;
                if self.channel.id() != ws::ChannelId::NONE {
                    self.load(ctx);
                } else {
                    self.config = api::Config::default();
                    self.loaded = false;
                }
                Ok(true)
            }
            Msg::AppBroadcast(packet) => {
                let event = packet?.decode_event()?;
                if event.channel == self.channel.id() {
                    return Ok(false);
                }
                if let api::AppEventKind::ConfigChanged { config } = event.kind {
                    self.config = config;
                    self.loaded = true;
                    return Ok(true);
                }
                Ok(false)
            }
            Msg::ConfigLoaded(result) => {
                self.config = result
                    .context(Message::LoadingConfig)?
                    .decode()
                    .context(Message::LoadingConfig)?
                    .config;
                self.loaded = true;
                Ok(true)
            }
            Msg::ThemeChanged(theme) => {
                self.config.theme = theme;
                self.persist(ctx);
                Ok(true)
            }
            Msg::TvdbKeyChanged(val) => {
                self.config.tvdb_api_key = val;
                self.persist(ctx);
                Ok(true)
            }
            Msg::TvdbPinChanged(val) => {
                let val = val.trim();

                if val.is_empty() {
                    self.config.tvdb_pin = None;
                } else {
                    self.config.tvdb_pin = Some(val.to_owned());
                }

                self.persist(ctx);
                Ok(true)
            }
            Msg::TmdbKeyChanged(value) => {
                self.config.tmdb_api_key = value;
                self.persist(ctx);
                Ok(true)
            }
            Msg::PageTitleChanged(title) => {
                self.config.page_title = title;
                self.persist(ctx);
                Ok(true)
            }
            Msg::TimezoneChanged(tz) => {
                self.config.timezone = tz;
                self.persist(ctx);
                Ok(true)
            }
            Msg::LanguageChanged(language) => {
                self.config.language = language;
                self.persist(ctx);
                Ok(true)
            }
            Msg::SyncLanguagesChanged(languages) => {
                self.config.sync_languages = languages;
                self.persist(ctx);
                Ok(true)
            }
            Msg::DashboardPageChanged(val) => {
                if let Ok(n) = val.parse::<u32>() {
                    self.config.dashboard_page = n;
                    self.persist(ctx);
                }
                Ok(false)
            }
            Msg::DashboardLookaheadChanged(lookahead) => {
                self.config.dashboard_lookahead = lookahead;
                self.persist(ctx);
                Ok(true)
            }
            Msg::AutoSyncEnabledToggle => {
                self.config.auto_sync_enabled = !self.config.auto_sync_enabled;
                self.persist(ctx);
                Ok(true)
            }
            Msg::AutoSyncIntervalChanged(val) => {
                if let Ok(n) = val.parse::<u32>() {
                    self.config.auto_sync_interval_hours = n;
                    self.persist(ctx);
                }
                Ok(false)
            }
            Msg::IncludeSpecialsToggle => {
                self.config.include_specials = !self.config.include_specials;
                self.persist(ctx);
                Ok(true)
            }
            Msg::ReleaseFiltersChanged(filters) => {
                self.config.release_filters = filters;
                self.persist(ctx);
                Ok(true)
            }
            Msg::AirDateFiltersChanged(filters) => {
                self.config.air_date_filters = filters;
                self.persist(ctx);
                Ok(true)
            }
            Msg::SyncKindsChanged(kinds) => {
                self.config.sync_kinds = kinds;
                self.persist(ctx);
                Ok(true)
            }
            Msg::SaveDone(result) => {
                result.context(Message::SavingConfig)?;
                Ok(false)
            }
        }
    }

    /// Persist the current config to the server. Called on every edit so the
    /// settings page has no explicit save step.
    fn persist(&mut self, ctx: &Context<Self>) {
        if self.channel.id() == ws::ChannelId::NONE {
            return;
        }

        self._save_req = self
            .channel
            .request()
            .body(api::SetConfigRequest {
                config: self.config.clone(),
            })
            .on_packet(ctx.link().callback(Msg::SaveDone))
            .send();
    }

    fn load(&mut self, ctx: &Context<Self>) {
        if self.channel.id() == ws::ChannelId::NONE {
            return;
        }

        self._config_req = self
            .channel
            .request()
            .body(api::GetConfigRequest)
            .on_packet(ctx.link().callback(Msg::ConfigLoaded))
            .send();
    }
}

/// The time zone best matching what was typed, ignoring case: one whose name
/// starts with it, else one whose city starts with it, else one containing it.
fn complete_timezone(typed: &str) -> Option<&'static str> {
    let typed = typed.trim().to_lowercase();

    if typed.is_empty() {
        return None;
    }

    let zones = jiff_tzdb::available;
    let city = |name: &str| name.rsplit('/').next().unwrap_or(name).to_lowercase();

    zones()
        .find(|name| name.to_lowercase().starts_with(&typed))
        .or_else(|| zones().find(|name| city(name).starts_with(&typed)))
        .or_else(|| zones().find(|name| name.to_lowercase().contains(&typed)))
}
