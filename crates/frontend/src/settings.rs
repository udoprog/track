use musli_web::web03::prelude::*;
use yew::prelude::*;

use crate::SetupChannel;
use crate::background::Background;
use crate::error::{CustomContext, Error, Message};
use crate::ui::{LanguagePicker, ReleaseFiltersEditor, SecretInput};

fn tz_is_valid(name: &str) -> bool {
    name.is_empty() || jiff_tzdb::get(name).is_some()
}

pub(super) struct Settings {
    saving: bool,
    channel: ws::Channel,
    background: Background,
    config: api::Config,
    _setup: SetupChannel,
    _broadcast: ws::Listener,
    _config_req: ws::Request,
    _save_req: ws::Request,
}

pub(super) enum Msg {
    Channel(Result<ws::Channel, ws::Error>),
    AppBroadcast(Result<ws::Packet<api::AppBroadcast>, ws::Error>),
    ConfigLoaded(Result<ws::Packet<api::GetConfig>, ws::Error>),
    ThemeChanged(api::ThemeType),
    TvdbKeyChanged(String),
    TvdbPinChanged(String),
    TmdbKeyChanged(String),
    TimezoneChanged(String),
    LanguageChanged(Option<String>),
    ScheduleDaysChanged(String),
    DashboardPageChanged(String),
    AutoSyncEnabledToggle,
    AutoSyncIntervalChanged(String),
    IncludeSpecialsChanged(bool),
    ReleaseFiltersChanged(Vec<api::ReleaseFilter>),
    Save,
    SaveDone(Result<ws::Packet<api::SetConfig>, ws::Error>),
}

#[derive(Properties, PartialEq)]
pub(super) struct Props {
    pub(super) onerror: Callback<Option<Error>>,
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
            saving: false,
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
                ctx.props().onerror.emit(Some(e));
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

        let on_schedule_days = link.callback(|e: InputEvent| {
            let input: web_sys::HtmlInputElement = e.target_unchecked_into();
            Msg::ScheduleDaysChanged(input.value())
        });

        let on_dashboard_page = link.callback(|e: InputEvent| {
            let input: web_sys::HtmlInputElement = e.target_unchecked_into();
            Msg::DashboardPageChanged(input.value())
        });

        let on_timezone = link.callback(|e: InputEvent| {
            let input: web_sys::HtmlInputElement = e.target_unchecked_into();
            Msg::TimezoneChanged(input.value())
        });

        let on_auto_sync_toggle = link.callback(|_| Msg::AutoSyncEnabledToggle);
        let on_include_specials_change = link.callback(|e: Event| {
            let select: web_sys::HtmlSelectElement = e.target_unchecked_into();
            Msg::IncludeSpecialsChanged(select.value() == "include")
        });

        let on_auto_sync_interval = link.callback(|e: InputEvent| {
            let input: web_sys::HtmlInputElement = e.target_unchecked_into();
            Msg::AutoSyncIntervalChanged(input.value())
        });

        let on_save = link.callback(|e: SubmitEvent| {
            e.prevent_default();
            Msg::Save
        });

        let theme_val = self.config.theme.to_string();

        html! {
            <form onsubmit={on_save} style="display: contents">
                if self.saving {
                    <div class="box info">
                        <span class="item-inline"><span class="icon arrow-path spin" /></span>
                        <span>{"Loading…"}</span>
                    </div>
                }

                <div class="column">
                    <h2>{"Appearance"}</h2>

                    <div class="form">
                        <div class="field">
                            <label>{"Theme"}</label>
                            <select class="input-select" onchange={on_theme} value={theme_val}>
                                <option value="dark" selected={self.config.theme == api::ThemeType::Dark}>{"Dark"}</option>
                                <option value="light" selected={self.config.theme == api::ThemeType::Light}>{"Light"}</option>
                            </select>
                        </div>
                    </div>
                </div>

                <div class="column">
                    <h2>{"API Keys"}</h2>

                    <div class="form">
                        <div class="field">
                            <label>{"TheTVDB API Key"}</label>
                            <SecretInput
                                placeholder="Enter TVDB API key"
                                value={self.config.tvdb_api_key.clone()}
                                on_change={link.callback(Msg::TvdbKeyChanged)}
                            />
                        </div>
                        <div class="field">
                            <label>{"TheTVDB Subscriber PIN (optional)"}</label>
                            <SecretInput
                                placeholder="Enter TVDB subscriber PIN"
                                value={self.config.tvdb_pin.clone().unwrap_or_default()}
                                on_change={link.callback(Msg::TvdbPinChanged)}
                            />
                        </div>
                        <div class="field">
                            <label>{"TheMovieDB API Key"}</label>
                            <SecretInput
                                placeholder="Enter TMDB API key"
                                value={self.config.tmdb_api_key.clone()}
                                on_change={link.callback(Msg::TmdbKeyChanged)}
                            />
                        </div>
                    </div>
                </div>

                <div class="column">
                    <h2>{"Dashboard"}</h2>

                    <div class="form">
                        <div class="field fill">
                            <label>{"Pending size"}</label>

                            <input
                                type="number"
                                class="input-number"
                                min="1"
                                max="100"
                                value={self.config.dashboard_page.to_string()}
                                oninput={on_dashboard_page}
                            />
                        </div>

                        <div class="field fill">
                            <label>{"Schedule days"}</label>

                            <input
                                type="number"
                                class="input-number"
                                min="1"
                                max="90"
                                value={self.config.schedule_duration_days.to_string()}
                                oninput={on_schedule_days}
                            />
                        </div>
                    </div>
                </div>

                <div class="column">
                    <h2>{"Display"}</h2>

                    <div class="form">
                        <div class={classes!("field", (!tz_is_valid(&self.config.timezone)).then_some("error"))}>
                            <label>{"Timezone (IANA name)"}</label>

                            <input type="text" class="input-text" placeholder="Leave empty to use browser timezone" value={self.config.timezone.clone()} oninput={on_timezone} list="tz-datalist" autocomplete="off" />

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

                        <div class="field">
                            <label>{"Default language"}</label>

                            <LanguagePicker
                                current={self.config.language.clone()}
                                placeholder="Default"
                                on_change={link.callback(Msg::LanguageChanged)}
                            />
                        </div>
                    </div>
                </div>

                <div class="column">
                    <h2>{"Sync"}</h2>

                    <div class="form">
                        <div class="field">
                            <label class="clickable" onclick={&on_auto_sync_toggle}>{"Auto-sync enabled"}</label>

                            <div class="row">
                                <span class={classes!("input-checkbox", self.config.auto_sync_enabled.then_some("checked"))} id="auto-sync-enabled" onclick={on_auto_sync_toggle}>
                                    <span class="mark" />
                                </span>
                            </div>
                        </div>

                        <div class="field">
                            <label>{"Sync interval (hours)"}</label>
                            <input
                                type="number"
                                class="input-number"
                                min="1"
                                max="168"
                                value={self.config.auto_sync_interval_hours.to_string()}
                                oninput={on_auto_sync_interval}
                            />
                        </div>

                        <div class="field">
                            <label>{"Specials when syncing"}</label>
                            <select class="input-select" onchange={on_include_specials_change}>
                                <option value="include" selected={self.config.include_specials}>{"Include"}</option>
                                <option value="skip" selected={!self.config.include_specials}>{"Skip"}</option>
                            </select>
                        </div>

                        <div class="field">
                            <label>{"Release dates"}</label>
                            <span class="hint">{"Release types (and countries) used to determine when a movie becomes available. The earliest matching date is used."}</span>
                            <ReleaseFiltersEditor
                                filters={self.config.release_filters.clone()}
                                on_change={link.callback(Msg::ReleaseFiltersChanged)}
                            />
                        </div>
                    </div>
                </div>

                <div class="row">
                    <button type="submit" class="btn">{"Save"}</button>
                </div>
            </form>
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
                Ok(true)
            }
            Msg::TvdbKeyChanged(val) => {
                self.config.tvdb_api_key = val;
                Ok(true)
            }
            Msg::TvdbPinChanged(val) => {
                let val = val.trim();

                if val.is_empty() {
                    self.config.tvdb_pin = None;
                } else {
                    self.config.tvdb_pin = Some(val.to_owned());
                }

                Ok(true)
            }
            Msg::TmdbKeyChanged(val) => {
                self.config.tmdb_api_key = val;
                Ok(true)
            }
            Msg::TimezoneChanged(val) => {
                self.config.timezone = val;
                Ok(true)
            }
            Msg::LanguageChanged(val) => {
                self.config.language = val;
                Ok(true)
            }
            Msg::ScheduleDaysChanged(val) => {
                if let Ok(n) = val.parse::<u32>() {
                    self.config.schedule_duration_days = n;
                }
                Ok(false)
            }
            Msg::DashboardPageChanged(val) => {
                if let Ok(n) = val.parse::<u32>() {
                    self.config.dashboard_page = n;
                }
                Ok(false)
            }
            Msg::AutoSyncEnabledToggle => {
                self.config.auto_sync_enabled = !self.config.auto_sync_enabled;
                Ok(true)
            }
            Msg::AutoSyncIntervalChanged(val) => {
                if let Ok(n) = val.parse::<u32>() {
                    self.config.auto_sync_interval_hours = n;
                }
                Ok(false)
            }
            Msg::IncludeSpecialsChanged(include) => {
                self.config.include_specials = include;
                Ok(true)
            }
            Msg::ReleaseFiltersChanged(filters) => {
                self.config.release_filters = filters;
                Ok(true)
            }
            Msg::Save => {
                self.saving = true;

                self._save_req = self
                    .channel
                    .request()
                    .body(api::SetConfigRequest {
                        config: self.config.clone(),
                    })
                    .on_packet(ctx.link().callback(Msg::SaveDone))
                    .send();

                Ok(true)
            }
            Msg::SaveDone(result) => {
                self.saving = false;
                result.context(Message::SavingConfig)?;
                Ok(true)
            }
        }
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
