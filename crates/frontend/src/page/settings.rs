use musli_web::web03::prelude::*;
use yew::prelude::*;

use crate::SetupChannel;
use crate::background::Background;
use crate::error::{CustomContext, Error, Message};
use crate::ui::{
    AirDateFiltersEditor, LanguagePicker, ReleaseFiltersEditor, SecretInput, SyncKindsEditor,
    SyncLanguagesEditor,
};

fn tz_is_valid(name: &str) -> bool {
    name.is_empty() || jiff_tzdb::get(name).is_some()
}

pub(crate) struct Settings {
    channel: ws::Channel,
    background: Background,
    config: api::Config,
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
    TimezoneChanged(String),
    LanguageChanged(api::Locale),
    SyncLanguagesChanged(Vec<api::Locale>),
    DashboardPageChanged(String),
    AutoSyncEnabledToggle,
    AutoSyncIntervalChanged(String),
    IncludeSpecialsToggle,
    ReleaseFiltersChanged(Vec<api::ReleaseFilter>),
    AirDateFiltersChanged(Vec<api::AirDateFilter>),
    SyncKindsChanged(Vec<api::SourceSyncKinds>),
    SaveDone(Result<ws::Packet<api::SetConfig>, ws::Error>),
}

#[derive(Properties, PartialEq)]
pub(crate) struct Props {
    pub(crate) onerror: Callback<Error>,
}

impl Component for Settings {
    type Message = Msg;
    type Properties = Props;

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
                ctx.props().onerror.emit(e);
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

        html! {
            <>
                <div class="desktop-row mobile-column align-top">
                    <div class="column fill">
                        <h4>{"Appearance"}</h4>

                        <div class="field">
                            <label>{"Theme"}</label>
                            <select class="input-select" onchange={on_theme} value={theme_val}>
                                <option value="dark" selected={self.config.theme == api::ThemeType::Dark}>{"Dark"}</option>
                                <option value="light" selected={self.config.theme == api::ThemeType::Light}>{"Light"}</option>
                            </select>
                        </div>

                        <div class={classes!("field", (!tz_is_valid(&self.config.timezone)).then_some("error"))}>
                            <label>{"TimeZone"}</label>

                            <input class="input-text" type="text" placeholder="Leave empty to use browser timezone" value={self.config.timezone.clone()} onchange={on_timezone} list="tz-datalist" autocomplete="off" />

                            <datalist id="tz-datalist">
                                { for jiff_tzdb::available().map(|name| html! {
                                    <option value={name} />
                                }) }
                            </datalist>

                            if !tz_is_valid(&self.config.timezone) {
                                <span>
                                    <span class="item-inline"><span class="icon exclamation-triangle" /></span>
                                    {"Unknown timezone"}
                                </span>
                            }
                        </div>

                        <div class="field fill">
                            <label>{"Pending size"}</label>

                            <input
                                type="number"
                                class="input-number"
                                min="1"
                                max="100"
                                value={self.config.dashboard_page.to_string()}
                                onchange={on_dashboard_page}
                            />
                        </div>
                    </div>

                    <div class="column fill">
                        <h4>{"Language"}</h4>

                        <div class="field">
                            <label>{"Language"}</label>
                            <span class="hint">{"The default language used for shows and movies."}</span>

                            <LanguagePicker
                                current={self.config.language}
                                placeholder="Default"
                                on_change={link.callback(Msg::LanguageChanged)}
                            />
                        </div>

                        <div class="field">
                            <label>{"Sync Languages"}</label>
                            <span class="hint">{"Which languages to fetch translations for. This will allow for searching and filtering based on these languages."}</span>

                            <SyncLanguagesEditor
                                languages={self.config.sync_languages.clone()}
                                on_change={link.callback(Msg::SyncLanguagesChanged)}
                            />
                        </div>
                    </div>
                </div>

                <div class="column">
                    <h4>{"Sync"}</h4>

                    <div class="desktop-row mobile-column align-top">
                        <div class="column fill">
                            <span class={classes!("input-checkbox", "has-text", self.config.auto_sync_enabled.then_some("checked"))} onclick={on_auto_sync_toggle}>
                                <span class="mark" />
                                <span>{"Automatic Sync"}</span>
                            </span>

                            <div class="input-group fill">
                                <span class="input-label has-text">
                                    {"Sync Interval in Hours"}
                                </span>

                                <input
                                    type="number"
                                    class="input-number fill"
                                    min="1"
                                    max="168"
                                    value={self.config.auto_sync_interval_hours.to_string()}
                                    onchange={on_auto_sync_interval}
                                />
                            </div>

                            <span class={classes!("input-checkbox", "has-text", self.config.include_specials.then_some("checked"))} onclick={on_include_specials_change}>
                                <span class="mark" />
                                <span>{"Specials for Watch Next"}</span>
                            </span>

                            <div class="field">
                                <label>{"Sync Sources"}</label>
                                <span class="hint">{"Which kinds of data each source contributes by default. Base covers titles, overviews and episodes; air dates merge by remote priority. Graphics always accumulate from every source. Individual shows and movies can override this per remote."}</span>

                                <SyncKindsEditor
                                    kinds={self.config.sync_kinds.clone()}
                                    on_change={link.callback(Msg::SyncKindsChanged)}
                                />
                            </div>
                        </div>

                        <div class="column fill">
                            <div class="field">
                                <label>{"Release Date"}</label>
                                <span class="hint">{"Release types (and countries) used to determine when a movie becomes available. The earliest matching date is used."}</span>

                                <ReleaseFiltersEditor
                                    filters={self.config.release_filters.clone()}
                                    on_change={link.callback(Msg::ReleaseFiltersChanged)}
                                />
                            </div>


                            <div class="field">
                                <label>{"Air Date"}</label>
                                <span class="hint">{"Restrict which sources' episode air dates qualify, by country and network. Source priority comes from each show's remote order (TVmaze ranks above TMDB by default)."}</span>

                                <AirDateFiltersEditor
                                    filters={self.config.air_date_filters.clone()}
                                    on_change={link.callback(Msg::AirDateFiltersChanged)}
                                />
                            </div>
                        </div>
                    </div>
                </div>

                <div class="column">
                    <h4>{"API Keys"}</h4>

                    <div class="field">
                        <label for="tvdb-api-key">{"TheTVDB API Key"}</label>

                        <SecretInput
                            id="tvdb-api-key"
                            placeholder="Enter TVDB API key"
                            value={self.config.tvdb_api_key.clone()}
                            on_change={link.callback(Msg::TvdbKeyChanged)}
                        />
                    </div>

                    <div class="field">
                        <label for="tvdb-pin">{"TheTVDB Subscriber PIN (optional)"}</label>

                        <SecretInput
                            id="tvdb-pin"
                            placeholder="Enter TVDB subscriber PIN"
                            value={self.config.tvdb_pin.clone().unwrap_or_default()}
                            on_change={link.callback(Msg::TvdbPinChanged)}
                        />
                    </div>

                    <div class="field">
                        <label for="tmdb-api-key">{"TheMovieDB API Key"}</label>

                        <SecretInput
                            id="tmdb-api-key"
                            placeholder="Enter TMDB API key"
                            value={self.config.tmdb_api_key.clone()}
                            on_change={link.callback(Msg::TmdbKeyChanged)}
                        />
                    </div>
                </div>
            </>
        }
    }
}

impl Settings {
    fn try_update(&mut self, ctx: &Context<Self>, msg: Msg) -> Result<bool, Error> {
        match msg {
            Msg::Channel(result) => {
                self.channel = result?;
                if self.channel.id() != ws::ChannelId::NONE {
                    self.load(ctx);
                } else {
                    self.config = api::Config::default();
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
        self._config_req = self
            .channel
            .request()
            .body(api::GetConfigRequest)
            .on_packet(ctx.link().callback(Msg::ConfigLoaded))
            .send();
    }
}
