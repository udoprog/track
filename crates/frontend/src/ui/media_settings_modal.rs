use api::{IncludeSpecials, TimeInfo};
use musli_web::web03::prelude::*;
use web_sys::{Event, MouseEvent};
use yew::prelude::*;

use crate::SetupChannel;
use crate::background::Background;
use crate::error::{CustomContext, Error, Message};
use crate::ui::Button;

use super::{
    AIR_DATE_KINDS, AIR_DATE_SOURCES, FiltersEditor, LanguagePicker, Modal, RELEASE_KINDS,
    RELEASE_SOURCES,
};

/// Which media's settings a [`MediaSettingsModal`] manages. Each variant loads and
/// mutates through its own endpoints.
#[derive(Clone, Copy, PartialEq)]
pub(crate) enum SettingsTarget {
    Movie(api::MovieId),
    Show(api::ShowId),
}

#[derive(Properties, PartialEq)]
pub(crate) struct Props {
    pub(crate) target: SettingsTarget,
    /// Open the parent-owned graphics modal (which closes this one).
    pub(crate) on_edit_graphics: Callback<()>,
    /// Open the parent-owned remote editor (which closes this one).
    pub(crate) on_edit_remotes: Callback<()>,
    pub(crate) on_close: Callback<()>,
}

/// The loaded record whose settings are being edited; `None` while loading.
enum Loaded {
    Movie(api::Movie),
    Show(api::Show),
}

pub(crate) enum Msg {
    Channel(Result<ws::Channel, ws::Error>),
    AppBroadcast(Result<ws::Packet<api::AppBroadcast>, ws::Error>),
    MovieLoaded(Result<ws::Packet<api::GetMovie>, ws::Error>),
    ShowLoaded(Result<ws::Packet<api::GetShow>, ws::Error>),
    ConfigLoaded(Result<ws::Packet<api::GetConfig>, ws::Error>),
    SetLanguage(api::Locale),
    SetAutoSync(bool),
    SetIncludeSpecials(IncludeSpecials),
    SetReleaseFilters(Option<api::FilterRules>),
    SetAirDateFilters(Option<api::FilterRules>),
    Sync,
    SetMovieLanguageDone(
        api::Locale,
        Result<ws::Packet<api::SetMovieLanguage>, ws::Error>,
    ),
    SetShowLanguageDone(
        api::Locale,
        Result<ws::Packet<api::SetShowLanguage>, ws::Error>,
    ),
    SetMovieAutoSyncDone(bool, Result<ws::Packet<api::SetMovieAutoSync>, ws::Error>),
    SetShowAutoSyncDone(bool, Result<ws::Packet<api::SetShowAutoSync>, ws::Error>),
    SetIncludeSpecialsDone(
        IncludeSpecials,
        Result<ws::Packet<api::SetShowIncludeSpecials>, ws::Error>,
    ),
    SetReleaseFiltersDone(Result<ws::Packet<api::SetMovieReleaseFilters>, ws::Error>),
    SetAirDateFiltersDone(Result<ws::Packet<api::SetShowAirDateFilters>, ws::Error>),
    SyncMovieDone(Result<ws::Packet<api::SyncMovie>, ws::Error>),
    SyncShowDone(Result<ws::Packet<api::SyncShow>, ws::Error>),
    SetTime(TimeInfo),
}

pub(crate) struct MediaSettingsModal {
    channel: ws::Channel,
    _setup: SetupChannel,
    _broadcast: ws::Listener,
    _load_req: ws::Request,
    _config_req: ws::Request,
    _mutate_req: ws::Request,
    _sync_req: ws::Request,
    data: Option<Loaded>,
    default_release_filters: api::FilterRules,
    default_air_date_filters: api::FilterRules,
    syncing: bool,
    time: TimeInfo,
    _time_handle: ContextHandle<TimeInfo>,
    background: Background,
}

impl Component for MediaSettingsModal {
    type Message = Msg;
    type Properties = Props;

    fn create(ctx: &Context<Self>) -> Self {
        let (ws, _) = ctx
            .link()
            .context::<ws::Handle>(Callback::noop())
            .expect("Expected ws::Handle in context");

        let (background, _) = ctx
            .link()
            .context::<Background>(Callback::noop())
            .expect("Expected Background in context");

        let (time, _time_handle) = ctx
            .link()
            .context::<TimeInfo>(ctx.link().callback(Msg::SetTime))
            .expect("Expected TimeInfo in context");

        let _setup = SetupChannel::new(ws.clone(), ctx.link().callback(Msg::Channel));
        let _broadcast = ws.on_broadcast(ctx.link().callback(Msg::AppBroadcast));

        Self {
            channel: ws::Channel::default(),
            _setup,
            _broadcast,
            _load_req: ws::Request::default(),
            _config_req: ws::Request::default(),
            _mutate_req: ws::Request::default(),
            _sync_req: ws::Request::default(),
            data: None,
            default_release_filters: api::FilterRules::default_release_rules(),
            default_air_date_filters: api::FilterRules::default(),
            syncing: false,
            time,
            _time_handle,
            background,
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

    fn view(&self, ctx: &Context<Self>) -> Html {
        html! {
            <Modal icon="cog-6-tooth" title="Settings" on_close={ctx.props().on_close.clone()}>
                { self.view_content(ctx) }
            </Modal>
        }
    }
}

impl MediaSettingsModal {
    fn try_update(&mut self, ctx: &Context<Self>, msg: Msg) -> Result<bool, Error> {
        match msg {
            Msg::Channel(result) => {
                self.channel = result?;
                self.load(ctx);
                self.load_config(ctx);
                Ok(true)
            }
            Msg::AppBroadcast(packet) => {
                let event = packet?.decode_event()?;

                // Mutations originate from our own channel; ignore their echoes.
                if event.channel == self.channel.id() {
                    return Ok(false);
                }

                match &event.kind {
                    api::AppEventKind::MovieChanged { movie } if matches!(ctx.props().target, SettingsTarget::Movie(id) if movie.id == id) =>
                    {
                        self.data = Some(Loaded::Movie(movie.clone()));
                        Ok(true)
                    }
                    api::AppEventKind::ShowChanged { show } if matches!(ctx.props().target, SettingsTarget::Show(id) if show.id == id) =>
                    {
                        self.data = Some(Loaded::Show(show.clone()));
                        Ok(true)
                    }
                    api::AppEventKind::TaskAdded { task }
                    | api::AppEventKind::TaskStarted { task } => {
                        if self.is_our_sync(ctx, &task.kind) {
                            self.syncing = true;
                            return Ok(true);
                        }
                        Ok(false)
                    }
                    api::AppEventKind::TaskCompleted { task } => {
                        if self.is_our_sync(ctx, &task.kind) {
                            self.syncing = false;
                            self.load(ctx);
                            return Ok(true);
                        }
                        Ok(false)
                    }
                    _ => Ok(false),
                }
            }
            Msg::MovieLoaded(result) => {
                let movie = result
                    .context(Message::LoadingMovies)?
                    .decode()
                    .context(Message::LoadingMovies)?;
                if let Some(movie) = movie {
                    self.data = Some(Loaded::Movie(movie));
                }
                Ok(true)
            }
            Msg::ShowLoaded(result) => {
                let show = result
                    .context(Message::LoadingShow)?
                    .decode()
                    .context(Message::LoadingShow)?;
                if let Some(show) = show {
                    self.data = Some(Loaded::Show(show));
                }
                Ok(true)
            }
            Msg::ConfigLoaded(result) => {
                let config = result
                    .context(Message::LoadingConfig)?
                    .decode()
                    .context(Message::LoadingConfig)?
                    .config;
                self.default_release_filters = config.release_filters;
                self.default_air_date_filters = config.air_date_filters;
                Ok(true)
            }
            Msg::SetLanguage(language) => {
                // Skip the optimistic update too: it mirrors server state, so it
                // must not be applied when it cannot be persisted.
                if self.channel.id() == ws::ChannelId::NONE {
                    return Ok(false);
                }

                match ctx.props().target {
                    SettingsTarget::Movie(id) => {
                        if let Some(Loaded::Movie(m)) = &mut self.data {
                            m.language = language;
                        }

                        self._mutate_req = self
                            .channel
                            .request()
                            .body(api::SetMovieLanguageRequest { id, language })
                            .on_packet(
                                ctx.link()
                                    .callback(move |r| Msg::SetMovieLanguageDone(language, r)),
                            )
                            .send();
                    }
                    SettingsTarget::Show(id) => {
                        if let Some(Loaded::Show(s)) = &mut self.data {
                            s.language = language;
                        }
                        self._mutate_req = self
                            .channel
                            .request()
                            .body(api::SetShowLanguageRequest { id, language })
                            .on_packet(
                                ctx.link()
                                    .callback(move |r| Msg::SetShowLanguageDone(language, r)),
                            )
                            .send();
                    }
                }
                Ok(true)
            }
            Msg::SetAutoSync(auto_sync) => {
                if self.channel.id() == ws::ChannelId::NONE {
                    return Ok(false);
                }

                match ctx.props().target {
                    SettingsTarget::Movie(id) => {
                        if let Some(Loaded::Movie(m)) = &mut self.data {
                            m.auto_sync = auto_sync;
                        }
                        self._mutate_req = self
                            .channel
                            .request()
                            .body(api::SetMovieAutoSyncRequest { id, auto_sync })
                            .on_packet(
                                ctx.link()
                                    .callback(move |r| Msg::SetMovieAutoSyncDone(auto_sync, r)),
                            )
                            .send();
                    }
                    SettingsTarget::Show(id) => {
                        if let Some(Loaded::Show(s)) = &mut self.data {
                            s.auto_sync = auto_sync;
                        }
                        self._mutate_req = self
                            .channel
                            .request()
                            .body(api::SetShowAutoSyncRequest { id, auto_sync })
                            .on_packet(
                                ctx.link()
                                    .callback(move |r| Msg::SetShowAutoSyncDone(auto_sync, r)),
                            )
                            .send();
                    }
                }
                Ok(true)
            }
            Msg::SetIncludeSpecials(include_specials) => {
                if self.channel.id() == ws::ChannelId::NONE {
                    return Ok(false);
                }

                if let SettingsTarget::Show(id) = ctx.props().target {
                    if let Some(Loaded::Show(s)) = &mut self.data {
                        s.include_specials = include_specials;
                    }
                    self._mutate_req =
                        self.channel
                            .request()
                            .body(api::SetShowIncludeSpecialsRequest {
                                id,
                                include_specials,
                            })
                            .on_packet(ctx.link().callback(move |r| {
                                Msg::SetIncludeSpecialsDone(include_specials, r)
                            }))
                            .send();
                }
                Ok(true)
            }
            Msg::SetReleaseFilters(release_filters) => {
                if self.channel.id() == ws::ChannelId::NONE {
                    return Ok(false);
                }

                if let SettingsTarget::Movie(id) = ctx.props().target {
                    if let Some(Loaded::Movie(m)) = &mut self.data {
                        m.release_filters = release_filters.clone();
                    }
                    self._mutate_req = self
                        .channel
                        .request()
                        .body(api::SetMovieReleaseFiltersRequest {
                            id,
                            release_filters,
                        })
                        .on_packet(ctx.link().callback(Msg::SetReleaseFiltersDone))
                        .send();
                }
                Ok(true)
            }
            Msg::SetAirDateFilters(air_date_filters) => {
                if self.channel.id() == ws::ChannelId::NONE {
                    return Ok(false);
                }

                if let SettingsTarget::Show(id) = ctx.props().target {
                    if let Some(Loaded::Show(s)) = &mut self.data {
                        s.air_date_filters = air_date_filters.clone();
                    }
                    self._mutate_req = self
                        .channel
                        .request()
                        .body(api::SetShowAirDateFiltersRequest {
                            id,
                            air_date_filters,
                        })
                        .on_packet(ctx.link().callback(Msg::SetAirDateFiltersDone))
                        .send();
                }
                Ok(true)
            }
            Msg::Sync => {
                if self.channel.id() == ws::ChannelId::NONE {
                    return Ok(false);
                }

                // Show the spinner immediately; the task broadcasts then keep it in
                // sync (and clear it on completion).
                self.syncing = true;

                match ctx.props().target {
                    SettingsTarget::Movie(id) => {
                        self._sync_req = self
                            .channel
                            .request()
                            .body(api::SyncMovieRequest { id })
                            .on_packet(ctx.link().callback(Msg::SyncMovieDone))
                            .send();
                    }
                    SettingsTarget::Show(id) => {
                        self._sync_req = self
                            .channel
                            .request()
                            .body(api::SyncShowRequest { id })
                            .on_packet(ctx.link().callback(Msg::SyncShowDone))
                            .send();
                    }
                }
                Ok(true)
            }
            Msg::SetMovieLanguageDone(language, result) => {
                result.context(Message::SettingLanguage(language))?;
                Ok(false)
            }
            Msg::SetShowLanguageDone(language, result) => {
                result.context(Message::SettingLanguage(language))?;
                Ok(false)
            }
            Msg::SetMovieAutoSyncDone(auto_sync, result) => {
                result.context(Message::SettingAutoSync(auto_sync))?;
                Ok(false)
            }
            Msg::SetShowAutoSyncDone(auto_sync, result) => {
                result.context(Message::SettingAutoSync(auto_sync))?;
                Ok(false)
            }
            Msg::SetIncludeSpecialsDone(include_specials, result) => {
                result.context(Message::SettingIncludeSpecials(include_specials))?;
                Ok(false)
            }
            Msg::SetReleaseFiltersDone(result) => {
                result.context(Message::SettingReleaseFilters)?;
                Ok(false)
            }
            Msg::SetAirDateFiltersDone(result) => {
                result.context(Message::SettingAirDateFilters)?;
                Ok(false)
            }
            Msg::SyncMovieDone(result) => {
                result.context(Message::SyncingMovie)?;
                Ok(false)
            }
            Msg::SyncShowDone(result) => {
                result.context(Message::SyncingShow)?;
                Ok(false)
            }
            Msg::SetTime(time) => {
                self.time = time;
                Ok(true)
            }
        }
    }

    /// Request this target's record on the current channel.
    fn load(&mut self, ctx: &Context<Self>) {
        if self.channel.id() == ws::ChannelId::NONE {
            return;
        }

        self._load_req = match ctx.props().target {
            SettingsTarget::Movie(id) => self
                .channel
                .request()
                .body(api::GetMovieRequest { id })
                .on_packet(ctx.link().callback(Msg::MovieLoaded))
                .send(),
            SettingsTarget::Show(id) => self
                .channel
                .request()
                .body(api::GetShowRequest { id })
                .on_packet(ctx.link().callback(Msg::ShowLoaded))
                .send(),
        };
    }

    fn load_config(&mut self, ctx: &Context<Self>) {
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

    /// Whether a task is a sync of this modal's target.
    fn is_our_sync(&self, ctx: &Context<Self>, kind: &api::TaskKind) -> bool {
        match ctx.props().target {
            SettingsTarget::Movie(id) => {
                matches!(kind, api::TaskKind::SyncMovie { movie_id, .. } if *movie_id == id)
            }
            SettingsTarget::Show(id) => {
                matches!(kind, api::TaskKind::SyncShow { show_id, .. } if *show_id == id)
            }
        }
    }

    fn view_content(&self, ctx: &Context<Self>) -> Html {
        let Some(data) = self.data.as_ref() else {
            return html!(<div class="text-muted">{"Loading…"}</div>);
        };

        let link = ctx.link();

        let (language, auto_sync, has_images, has_remotes, last_synced_at) = match data {
            Loaded::Movie(m) => (
                m.language,
                m.auto_sync,
                !m.images.is_empty(),
                !m.remotes.is_empty(),
                m.last_synced_at,
            ),
            Loaded::Show(s) => (
                s.language,
                s.auto_sync,
                !s.images.is_empty(),
                !s.remotes.is_empty(),
                s.last_synced_at,
            ),
        };

        let on_auto_sync = link.callback(move |_: MouseEvent| Msg::SetAutoSync(!auto_sync));

        // Specials and air-date filters are show-only; release filters are movie-only.
        let specials = match data {
            Loaded::Show(s) => {
                let include_specials = s.include_specials;
                let on_change = link.callback(move |_: MouseEvent| {
                    Msg::SetIncludeSpecials(include_specials.cycle())
                });

                Some(html! {
                    <div class="input-group">
                        <span class="input-label has-text">{"Include Specials"}</span>
                        <div class="input-checkbox has-text fill" onclick={on_change}>
                            {include_specials.as_label()}
                        </div>
                    </div>
                })
            }
            Loaded::Movie(_) => None,
        };

        let release = match data {
            Loaded::Movie(m) => Some(self.view_release(ctx, m)),
            Loaded::Show(_) => None,
        };

        let air_dates = match data {
            Loaded::Show(s) => Some(self.view_air_dates(ctx, s)),
            Loaded::Movie(_) => None,
        };

        let last_synced =
            last_synced_at.map(|ts| AttrValue::from(ts.human_date_time(self.time.clone())));
        let on_sync = link.callback(|_: MouseEvent| Msg::Sync);
        let on_edit_graphics = ctx.props().on_edit_graphics.reform(|_: MouseEvent| ());
        let on_edit_remotes = ctx.props().on_edit_remotes.reform(|_: MouseEvent| ());

        html! {
            <div class="form">
                <div class="field">
                    <label>{"Language"}</label>

                    <LanguagePicker
                        current={language}
                        placeholder="Default"
                        on_change={link.callback(Msg::SetLanguage)}
                    />
                </div>

                <div class="input-group">
                    <div class="input-label has-text">{"Automatic Sync"}</div>
                    <div class={classes!("input-checkbox", "has-text", "fill", auto_sync.then_some("checked"))} id="auto-sync-enabled" onclick={on_auto_sync}>
                        <span class="mark" />
                        {if auto_sync { "Enabled" } else { "Disabled" }}
                    </div>
                </div>

                {specials}

                {release}

                {air_dates}

                <div class="input-group">
                    <span class="input-label has-text">{"Last Sync"}</span>

                    if let Some(ts) = last_synced {
                        <div class="input-text has-text fill" title="Last synced at">
                            <span>{ts}</span>
                        </div>
                    } else {
                        <div class="input-text has-text fill text-muted">
                            <span>{"Never synced"}</span>
                        </div>
                    }

                    if has_remotes {
                        <Button icon="arrow-path" spin={self.syncing} onclick={on_sync} title="Sync now" text="Sync" />
                    }
                </div>

                <div class="field">
                    if has_images {
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
        }
    }

    fn view_release(&self, ctx: &Context<Self>, movie: &api::Movie) -> Html {
        let link = ctx.link();
        let is_custom = movie.release_filters.is_some();

        let on_mode = {
            let default = self.default_release_filters.clone();
            link.callback(move |e: Event| {
                let select: web_sys::HtmlSelectElement = e.target_unchecked_into();
                match select.value().as_str() {
                    "custom" => Msg::SetReleaseFilters(Some(default.clone())),
                    _ => Msg::SetReleaseFilters(None),
                }
            })
        };

        let editor = movie.release_filters.as_ref().map(|filters| {
            let on_change = link.callback(|f: api::FilterRules| Msg::SetReleaseFilters(Some(f)));
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
    }

    fn view_air_dates(&self, ctx: &Context<Self>, show: &api::Show) -> Html {
        let link = ctx.link();
        let is_custom = show.air_date_filters.is_some();

        let on_mode = {
            let default = self.default_air_date_filters.clone();
            link.callback(move |_: MouseEvent| {
                Msg::SetAirDateFilters((!is_custom).then(|| default.clone()))
            })
        };

        let editor = show.air_date_filters.as_ref().map(|filters| {
            let on_change = link.callback(|f: api::FilterRules| Msg::SetAirDateFilters(Some(f)));
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
    }
}
