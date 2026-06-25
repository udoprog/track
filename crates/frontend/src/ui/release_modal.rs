use std::collections::HashSet;

use api::TimeInfo;
use musli_web::web03::prelude::*;
use yew::prelude::*;

use crate::SetupChannel;
use crate::background::Background;
use crate::error::{CustomContext, Error, Message};

use super::Modal;

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
    ToggleGroup(AttrValue),
    SetTime(TimeInfo),
}

pub(crate) struct ReleaseModal {
    channel: ws::Channel,
    _setup: SetupChannel,
    _broadcast: ws::Listener,
    _req: ws::Request,
    /// `None` while loading, `Some` once the response has arrived.
    rows: Option<Vec<api::ReleaseRow>>,
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
            rows: None,
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
                self.rows = Some(
                    result
                        .context(Message::LoadingReleases)?
                        .decode()
                        .context(Message::LoadingReleases)?
                        .releases,
                );
                Ok(true)
            }
            Msg::EpisodeLoaded(result) => {
                self.rows = Some(
                    result
                        .context(Message::LoadingReleases)?
                        .decode()
                        .context(Message::LoadingReleases)?
                        .releases,
                );
                Ok(true)
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
