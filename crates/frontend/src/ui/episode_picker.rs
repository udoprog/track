use web_sys::Event;
use yew::prelude::*;

use musli_web::web03::prelude::*;

use crate::SetupChannel;

/// Inline season + episode picker used for moving or fixing watched entries.
/// Renders two `<select>` elements and confirm/cancel buttons, fitting inside
/// a `row` or `table-entry` without taking up extra vertical space.
pub(crate) struct EpisodePicker {
    channel: ws::Channel,
    selected_season: Option<api::SeasonNumber>,
    episodes: Vec<api::Episode>,
    selected_episode: Option<u32>,
    _setup: SetupChannel,
    _req: ws::Request,
}

pub(crate) enum Msg {
    Channel(Result<ws::Channel, ws::Error>),
    SelectSeason(api::SeasonNumber),
    EpisodesLoaded(Result<ws::Packet<api::ListEpisodes>, ws::Error>),
    SelectEpisode(u32),
    Confirm,
    Cancel,
}

#[derive(Properties, PartialEq)]
pub(crate) struct Props {
    pub(crate) show_id: api::ShowId,
    pub(crate) seasons: Vec<api::Season>,
    #[prop_or_default]
    pub(crate) selected_season: Option<api::SeasonNumber>,
    #[prop_or_default]
    pub(crate) selected_episode: Option<u32>,
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

        let selected_season = match ctx.props().selected_season {
            Some(selected_season) => Some(selected_season),
            None => ctx
                .props()
                .seasons
                .iter()
                .find(|s| !s.season.is_special())
                .or_else(|| ctx.props().seasons.first())
                .map(|s| s.season),
        };

        let _setup = SetupChannel::new(ws, ctx.link().callback(Msg::Channel));

        tracing::warn!(selected_episode = ?ctx.props().selected_episode);

        Self {
            channel: ws::Channel::default(),
            selected_season,
            episodes: Vec::new(),
            selected_episode: ctx.props().selected_episode,
            _setup,
            _req: ws::Request::default(),
        }
    }

    fn update(&mut self, ctx: &Context<Self>, msg: Self::Message) -> bool {
        match msg {
            Msg::Channel(result) => {
                self.channel = result.unwrap_or_default();
                if self.channel.id() != ws::ChannelId::NONE
                    && let Some(season) = self.selected_season
                {
                    self.load_episodes(ctx, season);
                }
                false
            }
            Msg::SelectSeason(season) => {
                self.selected_season = Some(season);
                self.episodes.clear();
                self.selected_episode = None;
                self.load_episodes(ctx, season);
                true
            }
            Msg::EpisodesLoaded(result) => {
                if let Ok(packet) = result
                    && let Ok(resp) = packet.decode()
                {
                    self.episodes = resp.episodes;

                    if let Some(selected_episode) = self.selected_episode
                        && !self
                            .episodes
                            .iter()
                            .any(|ep| ep.episode == selected_episode)
                    {
                        self.selected_episode = None;
                    }
                }

                true
            }
            Msg::SelectEpisode(episode) => {
                self.selected_episode = Some(episode);
                false
            }
            Msg::Confirm => {
                if let (Some(season), Some(episode)) = (self.selected_season, self.selected_episode)
                {
                    ctx.props().on_confirm.emit((season, episode));
                }

                false
            }
            Msg::Cancel => {
                ctx.props().on_cancel.emit(());
                false
            }
        }
    }

    fn view(&self, ctx: &Context<Self>) -> Html {
        let link = ctx.link();
        let props = ctx.props();

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

        let can_confirm = self.selected_season.is_some() && self.selected_episode.is_some();

        html! {
            <div class="column">
                <div class="row align-end">
                    <select class="input-select" onchange={on_season_change}>
                        { for props.seasons.iter().map(|s| {
                            let value = s.season.ordinal().to_string();
                            let selected = self.selected_season == Some(s.season);
                            html! { <option {value} {selected}>{s.season.long().to_string()}</option> }
                        }) }
                    </select>

                    <select class="input-select" onchange={on_episode_change} disabled={self.episodes.is_empty()}>
                        { for self.episodes.iter().map(|ep| {
                            let value = ep.episode.to_string();
                            let label = format!("E{:02}", ep.episode);
                            let selected = self.selected_episode == Some(ep.episode);
                            html! { <option {value} {selected}>{label}</option> }
                        }) }
                    </select>

                    <div class="input-group">
                        <button class="btn" onclick={link.callback(|_| Msg::Cancel)}
                            title="Cancel">
                            <span class="icon x-mark" />
                        </button>

                        <button class="btn-success" onclick={link.callback(|_| Msg::Confirm)}
                            title="Confirm" disabled={!can_confirm}>
                            <span class="icon check" />
                        </button>
                    </div>
                </div>
            </div>
        }
    }
}

impl EpisodePicker {
    fn load_episodes(&mut self, ctx: &Context<Self>, season: api::SeasonNumber) {
        self._req = self
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
