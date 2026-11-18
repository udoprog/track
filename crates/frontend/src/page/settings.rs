use auth::yew023::AdminCloudflare;
use musli_web::web03::prelude::*;
use musli_web::yew023::ContextExt;
use sockel::Skeleton;
use yew::prelude::*;

use crate::background::Background;
use crate::class;
use crate::error::{CustomContext, Error, Message};
use crate::help;
use crate::router::{Route, SettingsPage};
use crate::ui::{
    AIR_DATE_KINDS, AIR_DATE_SOURCES, Button, DurationInput, FiltersEditor, FormRow,
    LanguagePicker, Link, NumberingsEditor, RELEASE_KINDS, RELEASE_SOURCES, SyncKindsEditor,
    SyncLanguagesEditor, XemLookupEditor,
};

fn tz_is_valid(name: &str) -> bool {
    name.is_empty() || jiff_tzdb::get(name).is_some()
}

#[derive(Properties, PartialEq)]
pub(crate) struct Props {
    /// Whether the user is an administrator, who also sees the system settings.
    pub(crate) admin: bool,
    /// The page to show; `None` lists the pages on a phone.
    pub(crate) page: Option<SettingsPage>,
}

pub(crate) struct Settings {
    channel: ws::Channel,
    background: Background,
    preferences: api::Preferences,
    /// The system configuration; only administrators load it.
    config: api::Config,
    /// Whether the real values have loaded; until then fields render as
    /// skeletons rather than flashing default values.
    preferences_loaded: bool,
    config_loaded: bool,
    _broadcast: ws::Listener,
    _preferences_req: ws::Request,
    _config_req: ws::Request,
    _save_preferences_req: ws::Request,
    _save_config_req: ws::Request,
}

pub(crate) enum Msg {
    Channel(ws::ChannelEvent),
    AppBroadcast(Result<ws::Packet<api::AppBroadcast>, ws::Error>),
    PreferencesLoaded(Result<ws::Packet<api::GetPreferences>, ws::Error>),
    ConfigLoaded(Result<ws::Packet<api::GetSystemConfig>, ws::Error>),
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
    XemLookupChanged(Vec<api::RemoteSource>),
    NumberingsChanged(Vec<api::NumberingPref>),
    PreferencesSaved(Result<ws::Packet<api::SetPreferences>, ws::Error>),
    ConfigSaved(Result<ws::Packet<api::SetSystemConfig>, ws::Error>),
}

impl Component for Settings {
    type Message = Msg;
    type Properties = Props;

    fn create(ctx: &Context<Self>) -> Self {
        let ws = ctx.handle();

        let channel = ws
            .channel()
            .on_change(ctx.link().callback(Msg::Channel))
            .open();
        let _broadcast = ws.on_broadcast(ctx.link().callback(Msg::AppBroadcast));

        let (background, _) = ctx
            .link()
            .context::<Background>(Callback::noop())
            .expect("Expected background handle in context");

        Self {
            channel,
            background,
            preferences: api::Preferences::default(),
            config: api::Config::default(),
            preferences_loaded: false,
            config_loaded: false,
            _broadcast,
            _preferences_req: ws::Request::default(),
            _config_req: ws::Request::default(),
            _save_preferences_req: ws::Request::default(),
            _save_config_req: ws::Request::default(),
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

        let theme_val = self.preferences.theme.to_string();

        let tz_valid = tz_is_valid(&self.preferences.timezone);
        let auto_sync = self.config.auto_sync_enabled;
        let specials = self.preferences.include_specials;
        let mine = |skeleton, control| self.field_slot(self.preferences_loaded, skeleton, control);
        let system = |skeleton, control| self.field_slot(self.config_loaded, skeleton, control);

        let admin = ctx.props().admin;
        // Only administrators have more than one page; anyone else lands on
        // their preferences whatever the address says.
        let current = ctx
            .props()
            .page
            .filter(|page| admin || !page.admin())
            .unwrap_or(SettingsPage::Preferences);
        let index = admin && ctx.props().page.is_none();

        html! {
            <>
                <h1 class={sockel::class::VISUALLY_HIDDEN}>{"Settings"}</h1>

                <div class={classes!(class::SETTINGS_LAYOUT, admin.then_some(class::HAS_NAV))}>
                if admin {
                    <nav class={classes!(class::SETTINGS_NAV, (!index).then_some(class::DESKTOP_ONLY))} aria-label="Settings pages">
                        { for SettingsPage::ALL.into_iter().map(|page| html! {
                            <Link to={Route::Settings(Some(page))} title={page.title()} class={classes!(class::SETTINGS_NAV_ITEM, (page == current).then_some(class::ACTIVE))} current={page == current}>
                                <span>{ page.title() }</span>
                            </Link>
                        }) }

                        <Link to={Route::Users} title="Users" class={class::SETTINGS_NAV_ITEM}><span>{"Users"}</span></Link>
                    </nav>
                }

                <div class={classes!(class::SETTINGS, index.then_some(class::DESKTOP_ONLY))}>
                    if admin {
                        <Link to={Route::Settings(None)} title="All settings" class={class::SETTINGS_BACK}><span>{"All settings"}</span></Link>
                    }

                    if current == SettingsPage::Preferences {
                    <section>
                        <h2>{"Appearance"}</h2>

                        <div class={sockel::form_rows(true)}>
                            <FormRow stack=true label="Theme">
                                { mine("", html! {
                                    <select class={classes!(sockel::class::INPUT, sockel::class::NATIVE_SELECT)} data-test="theme" title="Theme" onchange={on_theme} value={theme_val}>
                                        <option value="dark" selected={self.preferences.theme == api::ThemeType::Dark}>{"Dark"}</option>
                                        <option value="light" selected={self.preferences.theme == api::ThemeType::Light}>{"Light"}</option>
                                        <option value="system" selected={self.preferences.theme == api::ThemeType::System}>{"System"}</option>
                                    </select>
                                }) }
                            </FormRow>

                            <FormRow stack=true label="Time zone" hint="Leave empty to use the browser's time zone.">
                                { mine("", html! {
                                    <input class={classes!(sockel::class::INPUT, class::FILL)} type="text" title="Time zone" placeholder="Browser time zone" value={self.preferences.timezone.clone()} onchange={on_timezone} onkeydown={on_timezone_key} list="tz-datalist" autocomplete="off" />
                                }) }

                                <datalist id="tz-datalist">
                                    { for jiff_tzdb::available().map(|name| html! {
                                        <option value={name} />
                                    }) }
                                </datalist>

                                if !tz_valid {
                                    <span class={class::FIELD_ERROR} role="alert">{"Unknown time zone"}</span>
                                }
                            </FormRow>

                            <FormRow stack=true label="Up next per page" hint="How many cards What's Next shows on a page.">
                                { mine("", html! {
                                    <input
                                        type="number"
                                        class={classes!(sockel::class::INPUT, class::INPUT_NUMBER, sockel::class::NUMERIC)}
                                        title="Up next per page"
                                        min="1"
                                        max="100"
                                        value={self.preferences.dashboard_page.to_string()}
                                        onchange={on_dashboard_page}
                                    />
                                }) }
                            </FormRow>

                            <FormRow stack=true label="Up next look-ahead" hint="How far ahead What's Next shows episodes that have not aired yet.">
                                { mine("", html! {
                                    <div class={class::INPUT_GROUP}>
                                        <DurationInput
                                            value={self.preferences.dashboard_lookahead}
                                            on_change={link.callback(Msg::DashboardLookaheadChanged)}
                                        />
                                    </div>
                                }) }
                            </FormRow>

                            <FormRow stack=true label="Specials in What's Next">
                                { mine("", html! {
                                    <Button role="switch" checked={Some(specials)} title="Specials for Watch Next" onclick={on_include_specials_change} label={if specials { "Included" } else { "Skipped" }} />
                                }) }
                            </FormRow>
                        </div>
                    </section>

                    <section>
                        <h2>{"Language"}</h2>

                        <div class={sockel::form_rows(true)}>
                            <FormRow stack=true label="Language" hint="The language you see shows and movies in.">
                                { mine("", html! {
                                    <LanguagePicker
                                        current={self.preferences.language}
                                        placeholder="Default"
                                        on_change={link.callback(Msg::LanguageChanged)}
                                    />
                                }) }
                            </FormRow>
                        </div>
                    </section>
                    }

                    if current == SettingsPage::Site {
                        <section>
                            <h2>{"Site"}</h2>

                            <div class={sockel::form_rows(true)}>
                                <FormRow stack=true label="Page title">
                                    { system("", html! {
                                        <input class={classes!(sockel::class::INPUT, class::FILL)} type="text" title="Page title" placeholder="Track" value={self.config.page_title.clone()} onchange={on_page_title} autocomplete="off" />
                                    }) }
                                </FormRow>
                            </div>
                        </section>
                    }

                    if current == SettingsPage::Sync {
                        <section>
                            <h2>{"Sync"}</h2>

                            <div class={sockel::form_rows(true)}>
                                <FormRow stack=true label="Automatic sync" help={help::REMOTES}>
                                    { system("", html! {
                                        <Button role="switch" checked={Some(auto_sync)} title="Automatic sync" onclick={on_auto_sync_toggle} label={if auto_sync { "Enabled" } else { "Disabled" }} />
                                    }) }
                                </FormRow>

                                <FormRow stack=true label="Sync every">
                                    { system("", html! {
                                        <div class={class::INPUT_GROUP}>
                                            <input
                                                type="number"
                                                class={classes!(sockel::class::INPUT, class::INPUT_NUMBER, sockel::class::NUMERIC)}
                                                title="Sync every"
                                                min="1"
                                                max="168"
                                                value={self.config.auto_sync_interval_hours.to_string()}
                                                onchange={on_auto_sync_interval}
                                            />

                                            <span class={classes!(class::INPUT_LABEL, class::HAS_TEXT)}>{"hours"}</span>
                                        </div>
                                    }) }
                                </FormRow>

                                <FormRow stack=true label="Sync languages" hint="Translations to fetch; titles can be searched and filtered in these languages. Default also fetches each language users view a show or movie in.">
                                    { system(class::TALL, html! {
                                        <SyncLanguagesEditor
                                            languages={self.config.sync_languages.clone()}
                                            on_change={link.callback(Msg::SyncLanguagesChanged)}
                                        />
                                    }) }
                                </FormRow>
                            </div>
                        </section>
                    }

                    if current == SettingsPage::Sources {
                        <section>
                            <h2>{"Sources & dates"}</h2>

                            <div class={sockel::form_rows(true)}>
                                <FormRow stack=true label="Sync sources" help={help::REMOTES} hint="What each source contributes by default, in priority order (top wins). Base covers titles, overviews and episodes; air dates merge in this order; graphics come from every source. Shows and movies can override this per remote.">
                                    { system(class::TALL, html! {
                                        <SyncKindsEditor
                                            kinds={self.config.sync_kinds.clone()}
                                            on_change={link.callback(Msg::SyncKindsChanged)}
                                        />
                                    }) }
                                </FormRow>

                                <FormRow stack=true label="Find XEM through" help={help::NUMBERINGS} hint="Which of a show's remotes look its XEM entry up, top first. The first one XEM maps becomes the show's XEM remote.">
                                    { system(class::TALL, html! {
                                        <XemLookupEditor
                                            sources={self.config.xem_lookup.clone()}
                                            on_change={link.callback(Msg::XemLookupChanged)}
                                        />
                                    }) }
                                </FormRow>

                                <FormRow stack=true label="Other numberings" help={help::NUMBERINGS} hint="Shown on each episode in this order. A numbering that matches the episode's own code is left out, so episodes that line up show nothing extra.">
                                    { system(class::TALL, html! {
                                        <NumberingsEditor
                                            prefs={self.config.numberings.clone()}
                                            on_change={link.callback(Msg::NumberingsChanged)}
                                        />
                                    }) }
                                </FormRow>

                                <FormRow stack=true label="Release dates" hint="Which release dates count. A date matching any rule is considered, in the rules' order.">
                                    { system(class::TALL, html! {
                                        <FiltersEditor
                                            rules={self.config.release_filters.clone()}
                                            on_change={link.callback(Msg::ReleaseFiltersChanged)}
                                            kinds={RELEASE_KINDS}
                                            sources={RELEASE_SOURCES}
                                        />
                                    }) }
                                </FormRow>

                                <FormRow stack=true label="Air dates" hint="Which air dates count. A date matching any rule is considered, in the rules' order.">
                                    { system(class::TALL, html! {
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
                    }

                    if current == SettingsPage::ApiKeys {
                        <section>
                            <h2>{"API keys"}</h2>

                            <div class={sockel::form_rows(true)}>
                                <FormRow stack=true label="TheTVDB API key">
                                    { system("", html! {
                                        <sockel::SecretInput id="tvdb-api-key" label="TheTVDB API key" placeholder="Enter TVDB API key"
                                            value={AttrValue::from(self.config.tvdb_api_key.clone())}
                                            on_input={link.callback(Msg::TvdbKeyChanged)}
                                            commit=true titles=true show_label="Reveal"
                                            copy_label="Copy to clipboard" />
                                    }) }
                                </FormRow>

                                <FormRow stack=true label="TheTVDB subscriber PIN" hint="Optional.">
                                    { system("", html! {
                                        <sockel::SecretInput id="tvdb-pin" label="TheTVDB subscriber PIN" placeholder="Enter TVDB subscriber PIN"
                                            value={AttrValue::from(self.config.tvdb_pin.clone().unwrap_or_default())}
                                            on_input={link.callback(Msg::TvdbPinChanged)}
                                            commit=true titles=true show_label="Reveal"
                                            copy_label="Copy to clipboard" />
                                    }) }
                                </FormRow>

                                <FormRow stack=true label="TheMovieDB API key">
                                    { system("", html! {
                                        <sockel::SecretInput id="tmdb-api-key" label="TheMovieDB API key" placeholder="Enter TMDB API key"
                                            value={AttrValue::from(self.config.tmdb_api_key.clone())}
                                            on_input={link.callback(Msg::TmdbKeyChanged)}
                                            commit=true titles=true show_label="Reveal"
                                            copy_label="Copy to clipboard" />
                                    }) }
                                </FormRow>
                            </div>
                        </section>
                    }

                    if current == SettingsPage::CloudflareAccess {
                        <AdminCloudflare app_name="track" />
                    }
                </div>
                </div>
            </>
        }
    }
}

impl Settings {
    /// Render `control` once its values have loaded, or a skeleton placeholder
    /// of the given size class while they are still loading, so a field keeps
    /// its static label/hint without flashing a default value.
    fn field_slot(&self, loaded: bool, skeleton: &'static str, control: Html) -> Html {
        if loaded {
            control
        } else {
            html! { <Skeleton class={classes!(skeleton)} /> }
        }
    }

    fn try_update(&mut self, ctx: &Context<Self>, msg: Msg) -> Result<bool, Error> {
        match msg {
            Msg::Channel(event) => {
                if let ws::ChannelEvent::Failed(error) = event {
                    return Err(error.into());
                }
                if self.channel.is_open() {
                    self.load(ctx);
                } else {
                    self.preferences = api::Preferences::default();
                    self.config = api::Config::default();
                    self.preferences_loaded = false;
                    self.config_loaded = false;
                }
                Ok(true)
            }
            Msg::AppBroadcast(packet) => {
                let packet = packet?;
                let event = packet.decode_event()?;
                if packet.is_from(self.channel.id()) {
                    return Ok(false);
                }
                match event.kind {
                    api::AppEventKind::PreferencesChanged { preferences } => {
                        self.preferences = preferences;
                        self.preferences_loaded = true;
                        Ok(true)
                    }
                    api::AppEventKind::ConfigChanged { config } => {
                        self.config = config;
                        self.config_loaded = true;
                        Ok(true)
                    }
                    _ => Ok(false),
                }
            }
            Msg::PreferencesLoaded(result) => {
                self.preferences = result
                    .context(Message::LoadingConfig)?
                    .decode()
                    .context(Message::LoadingConfig)?
                    .preferences;
                self.preferences_loaded = true;
                Ok(true)
            }
            Msg::ConfigLoaded(result) => {
                self.config = result
                    .context(Message::LoadingConfig)?
                    .decode()
                    .context(Message::LoadingConfig)?
                    .config;
                self.config_loaded = true;
                Ok(true)
            }
            Msg::ThemeChanged(theme) => {
                self.preferences.theme = theme;
                self.save_preferences(ctx);
                Ok(true)
            }
            Msg::TvdbKeyChanged(val) => {
                self.config.tvdb_api_key = val;
                self.save_config(ctx);
                Ok(true)
            }
            Msg::TvdbPinChanged(val) => {
                let val = val.trim();

                if val.is_empty() {
                    self.config.tvdb_pin = None;
                } else {
                    self.config.tvdb_pin = Some(val.to_owned());
                }

                self.save_config(ctx);
                Ok(true)
            }
            Msg::TmdbKeyChanged(value) => {
                self.config.tmdb_api_key = value;
                self.save_config(ctx);
                Ok(true)
            }
            Msg::PageTitleChanged(title) => {
                self.config.page_title = title;
                self.save_config(ctx);
                Ok(true)
            }
            Msg::TimezoneChanged(tz) => {
                self.preferences.timezone = tz;
                self.save_preferences(ctx);
                Ok(true)
            }
            Msg::LanguageChanged(language) => {
                self.preferences.language = language;
                self.save_preferences(ctx);
                Ok(true)
            }
            Msg::SyncLanguagesChanged(languages) => {
                self.config.sync_languages = languages;
                self.save_config(ctx);
                Ok(true)
            }
            Msg::DashboardPageChanged(val) => {
                if let Ok(n) = val.parse::<u32>() {
                    self.preferences.dashboard_page = n;
                    self.save_preferences(ctx);
                }
                Ok(false)
            }
            Msg::DashboardLookaheadChanged(lookahead) => {
                self.preferences.dashboard_lookahead = lookahead;
                self.save_preferences(ctx);
                Ok(true)
            }
            Msg::AutoSyncEnabledToggle => {
                self.config.auto_sync_enabled = !self.config.auto_sync_enabled;
                self.save_config(ctx);
                Ok(true)
            }
            Msg::AutoSyncIntervalChanged(val) => {
                if let Ok(n) = val.parse::<u32>() {
                    self.config.auto_sync_interval_hours = n;
                    self.save_config(ctx);
                }
                Ok(false)
            }
            Msg::IncludeSpecialsToggle => {
                self.preferences.include_specials = !self.preferences.include_specials;
                self.save_preferences(ctx);
                Ok(true)
            }
            Msg::ReleaseFiltersChanged(filters) => {
                self.config.release_filters = filters;
                self.save_config(ctx);
                Ok(true)
            }
            Msg::AirDateFiltersChanged(filters) => {
                self.config.air_date_filters = filters;
                self.save_config(ctx);
                Ok(true)
            }
            Msg::SyncKindsChanged(kinds) => {
                self.config.sync_kinds = kinds;
                self.save_config(ctx);
                Ok(true)
            }
            Msg::XemLookupChanged(sources) => {
                self.config.xem_lookup = sources;
                self.save_config(ctx);
                Ok(true)
            }
            Msg::NumberingsChanged(prefs) => {
                self.config.numberings = prefs;
                self.save_config(ctx);
                Ok(true)
            }
            Msg::PreferencesSaved(result) => {
                result.context(Message::SavingConfig)?;
                Ok(false)
            }
            Msg::ConfigSaved(result) => {
                result.context(Message::SavingConfig)?;
                Ok(false)
            }
        }
    }

    /// Save the preferences. Called on every edit so the settings page has no
    /// explicit save step.
    fn save_preferences(&mut self, ctx: &Context<Self>) {
        if !self.channel.is_open() {
            return;
        }

        self._save_preferences_req = self
            .channel
            .request()
            .body(api::SetPreferencesRequest {
                preferences: self.preferences.clone(),
            })
            .on_packet(ctx.link().callback(Msg::PreferencesSaved))
            .send();
    }

    /// Save the system configuration, as [`Self::save_preferences`].
    fn save_config(&mut self, ctx: &Context<Self>) {
        if !self.channel.is_open() || !self.config_loaded {
            return;
        }

        self._save_config_req = self
            .channel
            .request()
            .body(api::SetSystemConfigRequest {
                config: self.config.clone(),
            })
            .on_packet(ctx.link().callback(Msg::ConfigSaved))
            .send();
    }

    fn load(&mut self, ctx: &Context<Self>) {
        if !self.channel.is_open() {
            return;
        }

        self._preferences_req = self
            .channel
            .request()
            .body(api::GetPreferencesRequest)
            .on_packet(ctx.link().callback(Msg::PreferencesLoaded))
            .send();

        if ctx.props().admin {
            self._config_req = self
                .channel
                .request()
                .body(api::GetSystemConfigRequest)
                .on_packet(ctx.link().callback(Msg::ConfigLoaded))
                .send();
        }
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
