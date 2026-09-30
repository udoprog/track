use std::collections::HashSet;

use api::TimeInfo;
use musli_web::web03::prelude::*;
use yew::prelude::*;

use crate::SetupChannel;
use crate::background::Background;
use crate::error::{CustomContext, Error, Message};

use super::{
    AIR_DATE_KINDS, AIR_DATE_SOURCES, Button, FiltersEditor, Modal, RELEASE_KINDS, RELEASE_SOURCES,
};

/// What a [`ReleaseModal`] shows. Each variant fetches from its own endpoint; the
/// server resolves the `considered` flag and grouping `label` for both.
#[derive(Clone, Copy, PartialEq)]
pub(crate) enum ReleaseTarget {
    Movie(api::MovieId),
    Episode(api::EpisodeId),
}

#[derive(Properties, PartialEq)]
pub(crate) struct Props {
    pub(crate) target: ReleaseTarget,
    pub(crate) title: AttrValue,
    pub(crate) on_close: Callback<()>,
}

pub(crate) enum Msg {
    Channel(Result<ws::Channel, ws::Error>),
    AppBroadcast(Result<ws::Packet<api::AppBroadcast>, ws::Error>),
    MovieLoaded(Result<ws::Packet<api::GetMovieReleases>, ws::Error>),
    EpisodeLoaded(Result<ws::Packet<api::GetEpisodeReleases>, ws::Error>),
    ConfigLoaded(Result<ws::Packet<api::GetConfig>, ws::Error>),
    /// The active filter was edited (whichever scope is in effect).
    EditFilters(api::FilterRules),
    /// Switch between the per-media override and the global default.
    ToggleMode,
    SetReleaseFiltersDone(Result<ws::Packet<api::SetMovieReleaseFilters>, ws::Error>),
    SetAirDateFiltersDone(Result<ws::Packet<api::SetShowAirDateFilters>, ws::Error>),
    SaveConfigDone(Result<ws::Packet<api::SetConfig>, ws::Error>),
    ToggleGroup(AttrValue),
    SetTime(TimeInfo),
}

pub(crate) struct ReleaseModal {
    channel: ws::Channel,
    _setup: SetupChannel,
    _broadcast: ws::Listener,
    _req: ws::Request,
    _config_req: ws::Request,
    _mutate_req: ws::Request,
    /// `None` while loading, `Some` once the response has arrived.
    rows: Option<Vec<api::ReleaseRow>>,
    /// The global config, holding the default filters and resent on a global edit.
    config: Option<api::Config>,
    /// The per-media override; `None` means the active filter is the global default.
    override_filters: Option<api::FilterRules>,
    /// The owning show of an episode target, needed to mutate its air-date override.
    show_id: Option<api::ShowId>,
    /// Labels of the currently expanded groups.
    expanded: HashSet<AttrValue>,
    time: TimeInfo,
    _time_handle: ContextHandle<TimeInfo>,
    background: Background,
}

impl Component for ReleaseModal {
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
            _req: ws::Request::default(),
            _config_req: ws::Request::default(),
            _mutate_req: ws::Request::default(),
            rows: None,
            config: None,
            override_filters: None,
            show_id: None,
            expanded: HashSet::new(),
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
        let on_close = ctx.props().on_close.clone();
        let title = ctx.props().title.clone();

        html! {
            <Modal icon="calendar" title={title} on_close={on_close}>
                { self.view_filters(ctx) }
                { self.view_content(ctx) }
            </Modal>
        }
    }
}

impl ReleaseModal {
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

                // Sync broadcasts originate from another channel; ignore our own.
                if event.channel == self.channel.id() {
                    return Ok(false);
                }

                if relevant(ctx.props().target, &event.kind) {
                    self.load(ctx);
                }

                Ok(false)
            }
            Msg::MovieLoaded(result) => {
                let resp = result
                    .context(Message::LoadingReleases)?
                    .decode()
                    .context(Message::LoadingReleases)?;
                self.rows = Some(resp.releases);
                self.override_filters = resp.filters;
                Ok(true)
            }
            Msg::EpisodeLoaded(result) => {
                let resp = result
                    .context(Message::LoadingReleases)?
                    .decode()
                    .context(Message::LoadingReleases)?;
                self.rows = Some(resp.releases);
                self.override_filters = resp.filters;
                self.show_id = Some(resp.show_id);
                Ok(true)
            }
            Msg::ConfigLoaded(result) => {
                self.config = Some(
                    result
                        .context(Message::LoadingConfig)?
                        .decode()
                        .context(Message::LoadingConfig)?
                        .config,
                );
                Ok(true)
            }
            Msg::EditFilters(filters) => {
                if self.override_filters.is_some() {
                    self.override_filters = Some(filters.clone());
                    self.send_override(ctx, Some(filters));
                } else {
                    if let Some(config) = self.config.as_mut() {
                        match ctx.props().target {
                            ReleaseTarget::Movie(_) => config.release_filters = filters,
                            ReleaseTarget::Episode(_) => config.air_date_filters = filters,
                        }
                    }
                    self.send_global(ctx);
                }

                Ok(true)
            }
            Msg::ToggleMode => {
                if self.override_filters.is_some() {
                    self.override_filters = None;
                    self.send_override(ctx, None);
                } else if let Some(rules) = self.global_rules(ctx).cloned() {
                    self.override_filters = Some(rules.clone());
                    self.send_override(ctx, Some(rules));
                }
                Ok(true)
            }
            Msg::SetReleaseFiltersDone(result) => {
                result.context(Message::SettingReleaseFilters)?;
                // Re-fetch so the per-row `considered` indicators (resolved
                // server-side) reflect the new filter.
                self.load(ctx);
                Ok(false)
            }
            Msg::SetAirDateFiltersDone(result) => {
                result.context(Message::SettingAirDateFilters)?;
                self.load(ctx);
                Ok(false)
            }
            Msg::SaveConfigDone(result) => {
                result.context(Message::SavingConfig)?;
                self.load(ctx);
                Ok(false)
            }
            Msg::ToggleGroup(label) => {
                if !self.expanded.remove(&label) {
                    self.expanded.insert(label);
                }

                Ok(true)
            }
            Msg::SetTime(time) => {
                self.time = time;
                Ok(true)
            }
        }
    }

    /// Request this target's releases on the current channel.
    fn load(&mut self, ctx: &Context<Self>) {
        if self.channel.id() == ws::ChannelId::NONE {
            return;
        }

        self._req = match ctx.props().target {
            ReleaseTarget::Movie(movie_id) => self
                .channel
                .request()
                .body(api::GetMovieReleasesRequest { movie_id })
                .on_packet(ctx.link().callback(Msg::MovieLoaded))
                .send(),
            ReleaseTarget::Episode(episode_id) => self
                .channel
                .request()
                .body(api::GetEpisodeReleasesRequest { episode_id })
                .on_packet(ctx.link().callback(Msg::EpisodeLoaded))
                .send(),
        };
    }

    /// Request the global config (default filters, plus the full config resent on
    /// a global edit).
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

    /// The global default rules for this target, once the config has loaded.
    fn global_rules(&self, ctx: &Context<Self>) -> Option<&api::FilterRules> {
        let config = self.config.as_ref()?;
        Some(match ctx.props().target {
            ReleaseTarget::Movie(_) => &config.release_filters,
            ReleaseTarget::Episode(_) => &config.air_date_filters,
        })
    }

    /// Persist the per-media override (`None` reverts to the global default).
    fn send_override(&mut self, ctx: &Context<Self>, filters: Option<api::FilterRules>) {
        if self.channel.id() == ws::ChannelId::NONE {
            return;
        }

        match ctx.props().target {
            ReleaseTarget::Movie(id) => {
                self._mutate_req = self
                    .channel
                    .request()
                    .body(api::SetMovieReleaseFiltersRequest {
                        id,
                        release_filters: filters,
                    })
                    .on_packet(ctx.link().callback(Msg::SetReleaseFiltersDone))
                    .send();
            }
            ReleaseTarget::Episode(_) => {
                let Some(id) = self.show_id else {
                    return;
                };
                self._mutate_req = self
                    .channel
                    .request()
                    .body(api::SetShowAirDateFiltersRequest {
                        id,
                        air_date_filters: filters,
                    })
                    .on_packet(ctx.link().callback(Msg::SetAirDateFiltersDone))
                    .send();
            }
        }
    }

    /// Persist the global config after a global-default edit.
    fn send_global(&mut self, ctx: &Context<Self>) {
        if self.channel.id() == ws::ChannelId::NONE {
            return;
        }

        let Some(config) = self.config.clone() else {
            return;
        };

        self._mutate_req = self
            .channel
            .request()
            .body(api::SetConfigRequest { config })
            .on_packet(ctx.link().callback(Msg::SaveConfigDone))
            .send();
    }

    /// The active filter shown above the list: a scope toggle, a warning when the
    /// global default is being edited, and the editor bound to the active rules.
    fn view_filters(&self, ctx: &Context<Self>) -> Html {
        let Some(global) = self.global_rules(ctx) else {
            return html! {};
        };

        let target = ctx.props().target;

        // An episode's override lives on its show; without one there is nothing to
        // edit.
        if matches!(target, ReleaseTarget::Episode(_))
            && !matches!(self.show_id, Some(id) if id != api::ShowId::new(0))
        {
            return html! {};
        }

        let is_custom = self.override_filters.is_some();
        let rules = self
            .override_filters
            .clone()
            .unwrap_or_else(|| global.clone());

        let (kinds, sources, custom_label, warning) = match target {
            ReleaseTarget::Movie(_) => (
                RELEASE_KINDS,
                RELEASE_SOURCES,
                "Custom for this movie",
                "Global default, this applies to all movies.",
            ),
            ReleaseTarget::Episode(_) => (
                AIR_DATE_KINDS,
                AIR_DATE_SOURCES,
                "Custom for this show",
                "Global default, this applies to all shows.",
            ),
        };

        let link = ctx.link();
        let on_toggle = link.callback(|_| Msg::ToggleMode);
        let on_change = link.callback(Msg::EditFilters);

        html! {
            <div class="form">
                <div class="input-group">
                    <span class="input-label has-text">{"Active filter"}</span>

                    <Button label={if is_custom { custom_label } else { "Use global default" }} title="Switch between a custom filter and the global default" class="input-checkbox fill" onclick={on_toggle} />
                </div>

                if !is_custom {
                    <span class="row text-gap">
                        <span class="item-inline danger">
                            <span class="icon exclamation-triangle" />
                        </span>

                        {warning}
                    </span>
                }

                <FiltersEditor rules={rules} on_change={on_change} kinds={kinds} sources={sources} />
            </div>
        }
    }

    fn view_content(&self, ctx: &Context<Self>) -> Html {
        let Some(rows) = self.rows.as_ref() else {
            return html!(<div class="text-muted">{"Loading…"}</div>);
        };

        if rows.is_empty() {
            return html!(<div class="text-muted">{"No releases recorded."}</div>);
        }

        // The server orders rows so equal labels are contiguous; group them while
        // preserving that order.
        let mut groups: Vec<(AttrValue, Vec<&api::ReleaseRow>)> = Vec::new();

        for r in rows {
            match groups.last_mut() {
                Some((label, group)) if label.as_str() == r.label => group.push(r),
                _ => groups.push((AttrValue::from(r.label.clone()), vec![r])),
            }
        }

        html! {
            <div class="column">
                { for groups.into_iter().map(|(label, releases)| self.view_group(ctx, label, releases)) }
            </div>
        }
    }

    fn view_group(
        &self,
        ctx: &Context<Self>,
        label: AttrValue,
        releases: Vec<&api::ReleaseRow>,
    ) -> Html {
        let expanded = self.expanded.contains(&label);
        let group_considered = releases.iter().any(|r| r.considered);
        let earliest = releases.iter().min_by_key(|r| r.timestamp);

        let on_toggle = ctx.link().callback({
            let label = label.clone();
            move |_| Msg::ToggleGroup(label.clone())
        });

        html! {
            <div class="row clickable align-top" onclick={on_toggle}>
                <span class="item-inline">
                    <span class={classes!("icon", if expanded { "ellipsis-horizontal" } else { "chevron-right" })} />
                </span>

                <div class="column fill">
                    <div class="row-split">
                        <div class="row">
                            {indicator(group_considered)}

                            if let Some(earliest) = earliest {
                                <span class="item-inline" title={earliest.source.as_label()}>
                                    <span class={classes!("logo", earliest.source.as_id())} />
                                </span>

                                { view_country(earliest.country) }
                            }

                            <span>{label.clone()}</span>
                        </div>

                        if let Some(earliest) = earliest {
                            <div class="row">
                                <span class="text-muted">{earliest.timestamp.human_date_time(self.time.clone())}</span>
                            </div>
                        }
                    </div>

                    if expanded {
                        <div class="column">
                            { for releases.iter().map(|r| self.view_row(r)) }
                        </div>
                    }
                </div>
            </div>
        }
    }

    fn view_row(&self, r: &api::ReleaseRow) -> Html {
        html! {
            <div class="row-split">
                <div class="row">
                    {indicator(r.considered)}

                    <span class="item-inline" title={r.source.as_label()}>
                        <span class={classes!("logo", r.source.as_id())} />
                    </span>

                    { view_country(r.country) }
                </div>

                <span class="text-muted">{r.timestamp.human_date_time(self.time.clone())}</span>
            </div>
        }
    }
}

/// Whether a broadcast event could change a target's releases.
fn relevant(target: ReleaseTarget, kind: &api::AppEventKind) -> bool {
    match target {
        ReleaseTarget::Movie(id) => {
            matches!(kind, api::AppEventKind::MovieChanged { movie } if movie.id == id)
        }
        ReleaseTarget::Episode(id) => match kind {
            api::AppEventKind::EpisodeChanged { episode } => episode.id == id,
            api::AppEventKind::EpisodesChanged { .. } | api::AppEventKind::ShowChanged { .. } => {
                true
            }
            _ => false,
        },
    }
}

/// A release's country as a flag (falling back to its name/code), or nothing when
/// it is the default/unspecified country.
fn view_country(country: api::Country) -> Html {
    if country.is_default() {
        return html! {};
    }

    let Some(c) = country.to_iso() else {
        return html! { <span class="text-muted">{country}</span> };
    };

    if c.has_flag {
        html! {
            <span class="item-inline" title={c.name}>
                <span class={classes!("flag", c.alpha2)}></span>
            </span>
        }
    } else {
        html! {
            <span class="text-muted">
                <span class={c.name}></span>
            </span>
        }
    }
}

/// Whether a release is considered, as a check/minus indicator.
fn indicator(on: bool) -> Html {
    let (icon, title) = if on {
        ("check", "Considered for the release date")
    } else {
        ("minus", "Excluded by the current release date settings")
    };

    html! {
        <span class={classes!("item-inline", (!on).then_some("text-muted"))} title={title}>
            <span class={classes!("icon", icon)} />
        </span>
    }
}
