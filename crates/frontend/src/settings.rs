use musli_web::web03::prelude::*;
use yew::prelude::*;

use crate::error::{CustomContext, Error, Message};

fn tz_is_valid(name: &str) -> bool {
    name.is_empty() || jiff_tzdb::get(name).is_some()
}

pub(super) struct Settings {
    channel: ws::Channel,
    config: api::Config,
    _setup: crate::SetupChannel,
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
    TmdbKeyChanged(String),
    TimezoneChanged(String),
    ScheduleDaysChanged(String),
    DashboardLimitChanged(String),
    DashboardPageChanged(String),
    AutoSyncEnabledToggle,
    AutoSyncIntervalChanged(String),
    Save,
    SaveDone(Result<ws::Packet<api::SetConfig>, ws::Error>),
}

#[derive(Properties, PartialEq)]
pub(super) struct Props {
    pub(super) onerror: Callback<Error>,
}

impl Component for Settings {
    type Message = Msg;
    type Properties = Props;

    fn create(ctx: &Context<Self>) -> Self {
        let (ws, _) = ctx
            .link()
            .context::<ws::Handle>(Callback::noop())
            .expect("ws::Handle context not found");

        let _setup = crate::SetupChannel::new(ws.clone(), ctx.link().callback(Msg::Channel));
        let _broadcast = ws.on_broadcast(ctx.link().callback(Msg::AppBroadcast));

        Self {
            channel: ws::Channel::default(),
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

        let on_tvdb = link.callback(|e: InputEvent| {
            let input: web_sys::HtmlInputElement = e.target_unchecked_into();
            Msg::TvdbKeyChanged(input.value())
        });

        let on_tmdb = link.callback(|e: InputEvent| {
            let input: web_sys::HtmlInputElement = e.target_unchecked_into();
            Msg::TmdbKeyChanged(input.value())
        });

        let on_schedule_days = link.callback(|e: InputEvent| {
            let input: web_sys::HtmlInputElement = e.target_unchecked_into();
            Msg::ScheduleDaysChanged(input.value())
        });

        let on_dashboard_limit = link.callback(|e: InputEvent| {
            let input: web_sys::HtmlInputElement = e.target_unchecked_into();
            Msg::DashboardLimitChanged(input.value())
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
            <form class="page" onsubmit={on_save}>
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
                            <label>{"TheTVDB Legacy API Key"}</label>

                            <input
                                type="text"
                                class="input-text"
                                placeholder="Enter TVDB API key"
                                value={self.config.tvdb_legacy_apikey.clone()}
                                oninput={on_tvdb}
                            />
                        </div>
                        <div class="field">
                            <label>{"TheMovieDB API Key"}</label>

                            <input
                                type="text"
                                class="input-text"
                                placeholder="Enter TMDB API key"
                                value={self.config.tmdb_api_key.clone()}
                                oninput={on_tmdb}
                            />
                        </div>
                    </div>
                </div>

                <div class="column">
                    <h2>{"Dashboard"}</h2>

                    <div class="form">
                        <div class="field fill">
                            <label>{"Pending limit"}</label>

                            <input
                                type="number"
                                class="input-number"
                                min="1"
                                max="100"
                                value={self.config.dashboard_limit.to_string()}
                                oninput={on_dashboard_limit}
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

                        <div class="field fill">
                            <label>{"Page size"}</label>

                            <input
                                type="number"
                                class="input-number"
                                min="1"
                                max="100"
                                value={self.config.dashboard_page.to_string()}
                                oninput={on_dashboard_page}
                            />
                        </div>
                    </div>
                </div>

                <div class="column">
                    <h2>{"Display"}</h2>

                    <div class="form">
                        <div class={classes!("field", (!tz_is_valid(&self.config.timezone)).then_some("error"))}>
                            <label>{"Timezone (IANA name)"}</label>

                            <input
                                type="text"
                                class="input-text"
                                placeholder="Leave empty to use browser timezone"
                                value={self.config.timezone.clone()}
                                oninput={on_timezone}
                                list="tz-datalist"
                                autocomplete="off"
                            />
                            <datalist id="tz-datalist">
                                { for jiff_tzdb::available().map(|name| html! {
                                    <option value={name} />
                                }) }
                            </datalist>

                            if !tz_is_valid(&self.config.timezone) {
                                <span>
                                    <span class="icon-inline"><span class="icon exclamation-triangle" /></span>
                                    {"Unknown timezone"}
                                </span>
                            }
                        </div>
                    </div>
                </div>

                <div class="column">
                    <h2>{"Sync"}</h2>

                    <div class="form">
                        <div class="field">
                            <label class="clickable" onclick={&on_auto_sync_toggle}>{"Auto-sync enabled"}</label>

                            <span
                                class={classes!("input-checkbox", self.config.auto_sync_enabled.then_some("checked"))}
                                id="auto-sync-enabled"
                                onclick={on_auto_sync_toggle}
                            >
                                <span class="mark" />
                            </span>
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
                    </div>
                </div>

                <button type="submit" class="btn">{"Save"}</button>
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
                self.config.tvdb_legacy_apikey = val;
                Ok(false)
            }
            Msg::TmdbKeyChanged(val) => {
                self.config.tmdb_api_key = val;
                Ok(false)
            }
            Msg::TimezoneChanged(val) => {
                self.config.timezone = val;
                Ok(false)
            }
            Msg::ScheduleDaysChanged(val) => {
                if let Ok(n) = val.parse::<u32>() {
                    self.config.schedule_duration_days = n;
                }
                Ok(false)
            }
            Msg::DashboardLimitChanged(val) => {
                if let Ok(n) = val.parse::<u32>() {
                    self.config.dashboard_limit = n;
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
            Msg::Save => {
                self._save_req = self
                    .channel
                    .request()
                    .body(api::SetConfigRequest {
                        config: self.config.clone(),
                    })
                    .on_packet(ctx.link().callback(Msg::SaveDone))
                    .send();
                Ok(false)
            }
            Msg::SaveDone(result) => {
                result.context(Message::SavingConfig)?;
                Ok(false)
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
