use core::fmt;

use api::Timed as _;
use web_sys::Event;
use yew::prelude::*;

use musli_web::web03::prelude::*;

use crate::SetupChannel;
use crate::background::Background;
use crate::error::{CustomContext as _, Error, Message};
use crate::ui::{Button, Variant};

pub(crate) struct EpisodePicker {
    channel: ws::Channel,
    seasons: Vec<api::Season>,
    episodes: Vec<api::Episode>,
    // The selected season.
    season: Option<api::SeasonNumber>,
    // The selected episode.
    episode: Option<u32>,
    time: api::TimeInfo,
    background: Background,
    // Ensures the timestamp-based best match only resolves once.
    did_best_match: bool,
    _time_handle: ContextHandle<api::TimeInfo>,
    _setup: SetupChannel,
    _seasons_req: ws::Request,
    _episodes_req: ws::Request,
    _best_match_req: ws::Request,
}

pub(crate) enum Msg {
    Channel(Result<ws::Channel, ws::Error>),
    SelectSeason(api::SeasonNumber),
    SeasonsLoaded(Result<ws::Packet<api::ListSeasons>, ws::Error>),
    EpisodesLoaded(Result<ws::Packet<api::ListEpisodes>, ws::Error>),
    BestMatchLoaded(Result<ws::Packet<api::FindEpisodeByTimestamp>, ws::Error>),
    SelectEpisode(u32),
    Confirm,
    Cancel,
}

#[derive(Properties, PartialEq)]
pub(crate) struct Props {
    pub(crate) show_id: api::ShowId,
    #[prop_or_default]
    pub(crate) season: Option<api::SeasonNumber>,
    #[prop_or_default]
    pub(crate) episode: Option<u32>,
    #[prop_or_default]
    pub(crate) timestamp: Option<api::Timestamp>,
    pub(crate) on_confirm: Callback<(api::SeasonNumber, u32)>,
    pub(crate) on_cancel: Callback<()>,
}

impl Component for EpisodePicker {
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
            .context::<api::TimeInfo>(Callback::noop())
            .expect("Expected api::TimeInfo in context");

        let _setup = SetupChannel::new(ws, ctx.link().callback(Msg::Channel));

        Self {
            channel: ws::Channel::default(),
            seasons: Vec::new(),
            episodes: Vec::new(),
            season: ctx.props().season,
            episode: ctx.props().episode,
            time,
            background,
            did_best_match: false,
            _time_handle,
            _setup,
            _seasons_req: ws::Request::default(),
            _episodes_req: ws::Request::default(),
            _best_match_req: ws::Request::default(),
        }
    }

    fn update(&mut self, ctx: &Context<Self>, msg: Self::Message) -> bool {
        match self.try_update(ctx, msg) {
            Ok(changed) => changed,
            Err(error) => {
                self.background.error(error);
                false
            }
        }
    }

    fn view(&self, ctx: &Context<Self>) -> Html {
        let link = ctx.link();

        let on_season_change = link.callback(|e: Event| {
            let select: web_sys::HtmlSelectElement = e.target_unchecked_into();
            let n: u32 = select.value().parse().unwrap_or(0);
            Msg::SelectSeason(api::SeasonNumber::from_ordinal(n))
        });

        let on_episode_change = link.callback(|e: Event| {
            let select: web_sys::HtmlSelectElement = e.target_unchecked_into();
            let n: u32 = select.value().parse().unwrap_or(1);
            Msg::SelectEpisode(n)
        });

        let can_confirm = self.season.is_some() && self.episode.is_some();

        html! {
            <div class="column align-end">
                <select class="input-select" onchange={on_season_change}>
                    { for self.seasons.iter().map(|s| {
                        let value = s.season.ordinal().to_string();
                        let selected = self.season == Some(s.season);

                        html! {
                            <option {value} {selected}>
                                {format_season_string(s, self.time.clone()).to_string()}
                            </option>
                        }
                    }) }
                </select>

                <select class="input-select" onchange={on_episode_change} disabled={self.episodes.is_empty()}>
                    { for self.episodes.iter().map(|e| {
                        let value = e.episode.to_string();
                        let selected = self.episode == Some(e.episode);

                        html! {
                            <option {value} {selected}>
                                {format_episode_string(e, self.time.clone()).to_string()}
                            </option>
                        }
                    }) }
                </select>

                <div class="input-group">
                    <Button icon="x-mark" title="Cancel" onclick={link.callback(|_| Msg::Cancel)} />

                    <Button icon="check" title="Confirm" variant={Variant::Success} disabled={!can_confirm} onclick={link.callback(|_| Msg::Confirm)} />
                </div>
            </div>
        }
    }
}

impl EpisodePicker {
    fn try_update(&mut self, ctx: &Context<Self>, msg: Msg) -> Result<bool, Error> {
        match msg {
            Msg::Channel(result) => {
                self.channel = result.unwrap_or_default();

                if self.channel.id() != ws::ChannelId::NONE {
                    self.load_seasons(ctx);

                    if let (Some(ts), false) = (ctx.props().timestamp, self.did_best_match) {
                        self.load_best_match(ctx, ts);
                    } else if let Some(season) = self.season {
                        self.load_episodes(ctx, season);
                    }
                }

                Ok(false)
            }
            Msg::SelectSeason(season) => {
                self.season = Some(season);
                self.episodes.clear();
                self.episode = None;
                self.load_episodes(ctx, season);
                Ok(true)
            }
            Msg::SeasonsLoaded(result) => {
                let packet = result
                    .context(Message::LoadingSeasons)?
                    .decode()
                    .context(Message::LoadingSeasons)?;

                self.seasons = packet.seasons;
                Ok(true)
            }
            Msg::EpisodesLoaded(result) => {
                let packet = result
                    .context(Message::LoadingEpisodes)?
                    .decode()
                    .context(Message::LoadingEpisodes)?;

                self.episodes = packet.episodes;
                Ok(true)
            }
            Msg::BestMatchLoaded(result) => {
                let packet = result
                    .context(Message::LoadingEpisodes)?
                    .decode()
                    .context(Message::LoadingEpisodes)?;

                self.did_best_match = true;

                if let Some(matched) = packet.matched {
                    self.season = Some(matched.season);
                    self.episode = Some(matched.episode);
                    self.load_episodes(ctx, matched.season);
                }

                Ok(true)
            }
            Msg::SelectEpisode(episode) => {
                self.episode = Some(episode);
                Ok(false)
            }
            Msg::Confirm => {
                if let (Some(season), Some(episode)) = (self.season, self.episode) {
                    ctx.props().on_confirm.emit((season, episode));
                }

                Ok(false)
            }
            Msg::Cancel => {
                ctx.props().on_cancel.emit(());
                Ok(false)
            }
        }
    }

    fn load_best_match(&mut self, ctx: &Context<Self>, timestamp: api::Timestamp) {
        if self.channel.id() == ws::ChannelId::NONE {
            return;
        }

        self._best_match_req = self
            .channel
            .request()
            .body(api::FindEpisodeByTimestampRequest {
                show_id: ctx.props().show_id,
                timestamp,
            })
            .on_packet(ctx.link().callback(Msg::BestMatchLoaded))
            .send();
    }

    fn load_seasons(&mut self, ctx: &Context<Self>) {
        if self.channel.id() == ws::ChannelId::NONE {
            return;
        }

        self._seasons_req = self
            .channel
            .request()
            .body(api::ListSeasonsRequest {
                show_id: ctx.props().show_id,
            })
            .on_packet(ctx.link().callback(Msg::SeasonsLoaded))
            .send();
    }

    fn load_episodes(&mut self, ctx: &Context<Self>, season: api::SeasonNumber) {
        if self.channel.id() == ws::ChannelId::NONE {
            return;
        }

        self._episodes_req = self
            .channel
            .request()
            .body(api::ListEpisodesRequest {
                show_id: ctx.props().show_id,
                season,
            })
            .on_packet(ctx.link().callback(Msg::EpisodesLoaded))
            .send();
    }
}

fn format_season_string(season: &api::Season, time: api::TimeInfo) -> impl fmt::Display + '_ {
    fmt::from_fn(move |f| {
        if let Some(title) = season.strings.title() {
            write!(f, "{} - {}", season.season.short(), title)?;
        } else {
            write!(f, "{}", season.season.long())?;
        }

        if let Some(date_time) = season.human_date_time(time.clone()) {
            write!(f, " - {date_time}")?;
        }

        Ok(())
    })
}

fn format_episode_string(e: &api::Episode, time: api::TimeInfo) -> impl fmt::Display + '_ {
    fmt::from_fn(move |f| {
        if let Some(title) = e.strings.title() {
            write!(f, "{title}")?;
        } else {
            write!(f, "E{}", e.episode)?;
        }

        if let Some(date_time) = e.human_date_time(time.clone()) {
            write!(f, " - {date_time}")?;
        }

        if e.watched_count > 0 {
            write!(f, " - Watched {} time(s)", e.watched_count)?;
        }

        Ok(())
    })
}
