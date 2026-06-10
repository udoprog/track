use web_sys::{Event, MouseEvent};
use yew::prelude::*;

use iso639::{LanguageToCountry, Languages};
use musli_web::web03::prelude::*;

use crate::SetupChannel;

#[derive(Properties, PartialEq)]
pub(super) struct PaginationButtonsProps {
    pub(super) page: usize,
    pub(super) total_pages: usize,
    pub(super) on_page: Callback<usize>,
}

#[function_component]
pub(super) fn PaginationButtons(props: &PaginationButtonsProps) -> Html {
    let page = props.page.min(props.total_pages.saturating_sub(1));
    let prev = page.checked_sub(1);
    let next = (page + 1 < props.total_pages).then_some(page + 1);
    let on_page = props.on_page.clone();
    let on_page2 = props.on_page.clone();

    if props.total_pages <= 1 {
        return html! {};
    }

    html! {
        <>
            <button class="btn-icon" disabled={prev.is_none()}
                onclick={Callback::from(move |_| { if let Some(p) = prev { on_page.emit(p); } })}>
                <span class="icon arrow-left" />
            </button>
            <span class="text-muted">{format!("{} / {}", page + 1, props.total_pages)}</span>
            <button class="btn-icon" disabled={next.is_none()}
                onclick={Callback::from(move |_| { if let Some(p) = next { on_page2.emit(p); } })}>
                <span class="icon arrow-right" />
            </button>
        </>
    }
}

#[derive(Properties, PartialEq)]
pub(super) struct ConfirmDangerProps {
    pub(super) prompt: AttrValue,
    pub(super) label: Option<AttrValue>,
    pub(super) on_confirm: Callback<()>,
    pub(super) on_cancel: Callback<()>,
    #[prop_or_default]
    pub(super) btn_class: Classes,
}

#[function_component]
pub(super) fn ConfirmDanger(props: &ConfirmDangerProps) -> Html {
    let on_confirm = {
        let cb = props.on_confirm.clone();

        Callback::from(move |e: MouseEvent| {
            e.stop_propagation();
            cb.emit(());
        })
    };

    let on_cancel = {
        let cb = props.on_cancel.clone();
        Callback::from(move |e: MouseEvent| {
            e.stop_propagation();
            cb.emit(());
        })
    };

    html! {
        <div class="row-fill fill">
            if let Some(ref label) = props.label {
                <span class="fill">{&props.prompt}{" "}{label}{"?"}</span>
            } else {
                <span class="fill">{&props.prompt}{"?"}</span>
            }

            <div class="input-group end">
                <button onclick={on_cancel} class={classes!("btn", &props.btn_class)} title="No">
                    <span class="icon x-mark" />
                </button>

                <button onclick={on_confirm} class={classes!("btn-danger", &props.btn_class)} title="Yes">
                    <span class="icon check" />
                </button>
            </div>
        </div>
    }
}

// ── MarkWatchedPicker ─────────────────────────────────────────────────────────

/// Two-button step shown after clicking "Mark watched": choose now or when aired.
/// Renders as a `row-fill fill` that can replace the watch button's action area.
#[derive(Properties, PartialEq)]
pub(super) struct MarkWatchedPickerProps {
    pub(super) on_confirm: Callback<api::MarkTime>,
    pub(super) on_cancel: Callback<()>,
}

#[function_component]
pub(super) fn MarkWatchedPicker(props: &MarkWatchedPickerProps) -> Html {
    let on_now = props.on_confirm.reform(|e: MouseEvent| {
        e.stop_propagation();
        api::MarkTime::Now
    });

    let on_aired = props.on_confirm.reform(move |e: MouseEvent| {
        e.stop_propagation();
        api::MarkTime::WhenAired
    });

    let on_cancel = props.on_cancel.reform(move |e: MouseEvent| {
        e.stop_propagation();
    });

    html! {
        <div class="row-fill fill">
            <span class="fill">{"Watched when?"}</span>

            <div class="input-group end">
                <button class="btn-icon" onclick={on_cancel} title="Cancel">
                    <span class="icon x-mark" />
                </button>
                <button class="btn-success" onclick={on_now} title="Watched now">
                    <span class="icon-inline"><span class="icon check" /></span>
                    {"Now"}
                </button>
                <button class="btn" onclick={on_aired} title="Watched when aired">
                    <span class="icon-inline"><span class="icon clock" /></span>
                    {"Aired"}
                </button>
            </div>
        </div>
    }
}

// ── RemoteSourceSelect ───────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum RemoteSourceKind {
    Series,
    Movie,
}

#[derive(Properties, PartialEq)]
pub(super) struct RemoteSourceSelectProps {
    pub(super) remotes: Vec<api::RemoteId>,
    pub(super) current_source: Option<api::SyncSource>,
    pub(super) kind: RemoteSourceKind,
    pub(super) on_change: Callback<api::SyncSource>,
}

#[function_component]
pub(super) fn RemoteSourceSelect(props: &RemoteSourceSelectProps) -> Html {
    let mut options: Vec<api::SyncSource> = Vec::new();

    for remote in &props.remotes {
        let Some(source) = api::SyncSource::from_remote_source(remote.source()) else {
            continue;
        };

        if source == api::SyncSource::Tmdb
            || (matches!(props.kind, RemoteSourceKind::Series) && source == api::SyncSource::Tvdb)
        {
            if !options.iter().any(|existing| existing == &source) {
                options.push(source);
            }
        }
    }

    if options.is_empty() {
        return Html::default();
    }

    let selected = props
        .current_source
        .as_ref()
        .filter(|source| options.iter().any(|option| option == *source))
        .copied()
        .unwrap_or(options[0]);

    let disabled = options.len() <= 1;

    let on_change = {
        let cb = props.on_change.clone();

        Callback::from(move |e: Event| {
            let input: web_sys::HtmlSelectElement = e.target_unchecked_into();
            if let Some(source) = api::SyncSource::from_str(&input.value()) {
                cb.emit(source);
            }
        })
    };

    html! {
        <select class="input-select" onchange={on_change} {disabled} title="Select remote source">
            {
                for options.into_iter().map(|source| {
                    let label = source.as_str().to_uppercase();

                    html! {
                        <option value={source.as_str()} selected={source == selected}>{label}</option>
                    }
                })
            }
        </select>
    }
}

// ── ImageGallery ──────────────────────────────────────────────────────────────

/// A single entry shown in the image gallery.
#[derive(Clone, PartialEq)]
pub(super) struct ImageItem {
    pub(super) id: api::ImageId,
    pub(super) kind: api::ImageKind,
    pub(super) source: api::ImageSource,
    pub(super) image: api::Image,
    pub(super) selected: bool,
}

#[derive(Properties, PartialEq)]
pub(super) struct ImageGalleryProps {
    pub(super) items: Vec<ImageItem>,
    pub(super) kind: api::ImageKind,
    pub(super) on_select: Callback<api::ImageId>,
    pub(super) on_clear: Option<Callback<()>>,
    pub(super) on_close: Callback<()>,
}

#[function_component]
pub(super) fn ImageGallery(props: &ImageGalleryProps) -> Html {
    let images: Vec<_> = props
        .items
        .iter()
        .filter(|img| img.kind == props.kind)
        .collect();
    let on_select = props.on_select.clone();
    let on_close = props.on_close.clone();

    html! {
        <div class="modal-background" onclick={Callback::from(move |_| on_close.emit(()))}>
            <div class="modal" onclick={Callback::from(|e: MouseEvent| e.stop_propagation())}>
                <div class="row">
                    <span class="fill">{format!("Select {}", props.kind)}</span>

                    if let Some(on_clear) = props.on_clear.clone() {
                        <button class="btn" onclick={Callback::from(move |_| on_clear.emit(()))}>
                            <span class="icon-inline"><span class="icon x-mark" /></span>
                            {"Clear"}
                        </button>
                    }

                    <button class="btn-icon" onclick={props.on_close.reform(|_| ())}>
                        <span class="icon x-mark" />
                    </button>
                </div>

                if images.is_empty() {
                    <div class="empty text-muted">{"No images"}</div>
                } else {
                    <div class={classes!("image-gallery", props.kind.as_str())}>
                        { for images.iter().map(|img| {
                            let id = img.id;
                            let selected = img.selected;
                            let on_select = on_select.clone();
                            let title = img.source.to_string();

                            html! {
                                <div class={classes!("image-thumb", selected.then_some("selected"))} onclick={Callback::from(move |_| on_select.emit(id))} {title}>
                                    <img src={img.image.proxy_url()} />

                                    <div class="image-thumb-source">
                                        {img.source.to_string()}
                                    </div>

                                    if selected {
                                        <span class="image-thumb-check">{"✓"}</span>
                                    }
                                </div>
                            }
                        })}
                    </div>
                }
            </div>
        </div>
    }
}

// ── LanguagePicker ────────────────────────────────────────────────────────────

const LANGUAGE_PAGE_SIZE: usize = 5;

#[derive(Properties, PartialEq)]
pub(super) struct LanguagePickerProps {
    pub(super) current: Option<String>,
    pub(super) on_change: Callback<Option<String>>,
    pub(super) placeholder: &'static str,
}

pub(super) enum Msg {
    Open,
    Close,
    Filter(String),
    Page(usize),
    Pick(Option<String>),
}

pub(super) struct LanguagePicker {
    languages: Languages,
    language_to_country: LanguageToCountry,
    open: bool,
    filter: String,
    page: usize,
}

impl Component for LanguagePicker {
    type Message = Msg;
    type Properties = LanguagePickerProps;

    fn create(_ctx: &Context<Self>) -> Self {
        Self {
            languages: Languages::new(),
            language_to_country: LanguageToCountry::new(),
            open: false,
            filter: String::new(),
            page: 0,
        }
    }

    fn update(&mut self, ctx: &Context<Self>, msg: Self::Message) -> bool {
        match msg {
            Msg::Open => {
                self.open = true;
                self.filter.clear();
                self.page = 0;
            }
            Msg::Close => {
                self.open = false;
            }
            Msg::Filter(s) => {
                self.filter = s;
                self.page = 0;
            }
            Msg::Page(p) => {
                self.page = p;
            }
            Msg::Pick(value) => {
                self.open = false;
                ctx.props().on_change.emit(value);
            }
        }

        true
    }

    fn view(&self, ctx: &Context<Self>) -> Html {
        let link = ctx.link();
        let props = ctx.props();

        let value = ctx.props().current.as_ref().and_then(|code| {
            self.languages
                .get_by_part1(code)
                .and_then(|entry| Some((entry.ref_name, entry.part1?)))
        });

        let trigger = match value {
            Some((label, code)) => html! {
                <button class="btn" onclick={link.callback(|_| Msg::Open)} title="Select language">
                    <span class="icon-inline"><span class="icon language" /></span>
                    <span>{label}</span>


                    if let Some(code) = self.language_to_country.get_by_part1(code) {
                        <span class={classes!("flag", code)}></span>
                    }
                </button>
            },
            None => html! {
                <button class="btn" onclick={link.callback(|_| Msg::Open)} title="Select language">
                    <span class="icon-inline"><span class="icon language" /></span>
                    <span>{props.placeholder}</span>
                </button>
            },
        };

        if !self.open {
            return trigger;
        }

        let needle = self.filter.to_lowercase();
        let filtered: Vec<(&'static str, &'static iso639::Entry)> = self
            .languages
            .iter()
            .filter(|(_, entry)| {
                needle.is_empty() || entry.ref_name.to_lowercase().contains(&needle)
            })
            .collect();

        let total_pages = filtered.len().div_ceil(LANGUAGE_PAGE_SIZE).max(1);
        let page = self.page.min(total_pages.saturating_sub(1));

        let on_filter = link.callback(|e: InputEvent| {
            let input: web_sys::HtmlInputElement = e.target_unchecked_into();
            Msg::Filter(input.value())
        });

        let current = props.current.clone();

        html! {
            <>
                {trigger}
                <div class="modal-background" onclick={link.callback(|_| Msg::Close)}>
                    <div class="modal" onclick={Callback::from(|e: MouseEvent| e.stop_propagation())}>
                        <div class="row">
                            <span class="fill">{"Language"}</span>

                            <button class="btn-icon" onclick={link.callback(|_| Msg::Close)}>
                                <span class="icon x-mark" />
                            </button>
                        </div>

                        <div class="row">
                            <input type="text" class="input-text fill" placeholder="Filter" value={self.filter.clone()} oninput={on_filter} />
                        </div>

                        <div class="table">
                            <div class="table-entry row clickable" onclick={link.callback(|_| Msg::Pick(None))}>
                                <span class="fill">{props.placeholder}</span>

                                if current.is_none() {
                                    <span class="icon check" />
                                }
                            </div>

                            {
                                for filtered.iter()
                                    .skip(page.saturating_mul(LANGUAGE_PAGE_SIZE))
                                    .take(LANGUAGE_PAGE_SIZE)
                                    .map(|&(part1, entry)| {
                                        let selected = current.as_deref() == Some(part1);

                                        html! {
                                            <div key={part1} class={classes!("table-entry", "row", "clickable", selected.then_some("active"))} onclick={link.callback(move |_| Msg::Pick(Some(part1.to_string())))}>
                                                <span class="fill">{entry.ref_name}</span>

                                                if let Some(code) = self.language_to_country.get_by_part1(part1) {
                                                    <span class={classes!("flag-inline", "flag", code)} />
                                                }

                                                <span class="text-muted">{part1}</span>
                                            </div>
                                        }
                                    })
                            }
                        </div>

                        <div class="row center">
                            <PaginationButtons
                                page={page}
                                total_pages={total_pages}
                                on_page={link.callback(Msg::Page)}
                            />
                        </div>
                    </div>
                </div>
            </>
        }
    }
}

// ── EpisodePicker ─────────────────────────────────────────────────────────────

/// Inline season + episode picker used for moving or fixing watched entries.
/// Renders two `<select>` elements and confirm/cancel buttons, fitting inside
/// a `row` or `table-entry` without taking up extra vertical space.
pub(super) struct EpisodePicker {
    channel: ws::Channel,
    selected_season: Option<api::SeasonNumber>,
    episodes: Vec<api::Episode>,
    selected_episode: Option<u32>,
    _setup: SetupChannel,
    _req: ws::Request,
}

pub(super) enum EpisodePickerMsg {
    Channel(Result<ws::Channel, ws::Error>),
    SelectSeason(api::SeasonNumber),
    EpisodesLoaded(Result<ws::Packet<api::ListEpisodes>, ws::Error>),
    SelectEpisode(u32),
    Confirm,
    Cancel,
}

#[derive(Properties, PartialEq)]
pub(super) struct EpisodePickerProps {
    pub(super) prompt: AttrValue,
    pub(super) label: Option<AttrValue>,
    pub(super) series_id: api::SeriesId,
    pub(super) seasons: Vec<api::Season>,
    #[prop_or_default]
    pub(super) selected_season: Option<api::SeasonNumber>,
    #[prop_or_default]
    pub(super) selected_episode: Option<u32>,
    pub(super) on_confirm: Callback<(api::SeasonNumber, u32)>,
    pub(super) on_cancel: Callback<()>,
}

impl Component for EpisodePicker {
    type Message = EpisodePickerMsg;
    type Properties = EpisodePickerProps;

    fn create(ctx: &Context<Self>) -> Self {
        let (ws, _) = ctx
            .link()
            .context::<ws::Handle>(Callback::noop())
            .expect("ws::Handle context not found");

        let selected_season = match ctx.props().selected_season {
            Some(selected_season) => Some(selected_season),
            None => ctx
                .props()
                .seasons
                .iter()
                .find(|s| !s.number.is_special())
                .or_else(|| ctx.props().seasons.first())
                .map(|s| s.number),
        };

        let _setup = SetupChannel::new(ws, ctx.link().callback(EpisodePickerMsg::Channel));

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
            EpisodePickerMsg::Channel(result) => {
                self.channel = result.unwrap_or_default();
                if self.channel.id() != ws::ChannelId::NONE {
                    if let Some(season) = self.selected_season {
                        self.load_episodes(ctx, season);
                    }
                }
                false
            }
            EpisodePickerMsg::SelectSeason(season) => {
                self.selected_season = Some(season);
                self.episodes.clear();
                self.selected_episode = None;
                self.load_episodes(ctx, season);
                true
            }
            EpisodePickerMsg::EpisodesLoaded(result) => {
                if let Ok(packet) = result
                    && let Ok(resp) = packet.decode()
                {
                    self.episodes = resp.episodes;

                    if let Some(selected_episode) = self.selected_episode
                        && !self.episodes.iter().any(|ep| ep.number == selected_episode)
                    {
                        self.selected_episode = None;
                    }
                }

                true
            }
            EpisodePickerMsg::SelectEpisode(n) => {
                self.selected_episode = Some(n);
                false
            }
            EpisodePickerMsg::Confirm => {
                if let (Some(season), Some(episode)) = (self.selected_season, self.selected_episode)
                {
                    ctx.props().on_confirm.emit((season, episode));
                }

                false
            }
            EpisodePickerMsg::Cancel => {
                ctx.props().on_cancel.emit(());
                false
            }
        }
    }

    fn view(&self, ctx: &Context<Self>) -> Html {
        let link = ctx.link();

        let on_season_change = link.callback(|e: Event| {
            let select: web_sys::HtmlSelectElement = e.target_unchecked_into();
            let n: u32 = select.value().parse().unwrap_or(0);
            EpisodePickerMsg::SelectSeason(api::SeasonNumber::from_u32(n))
        });

        let on_episode_change = link.callback(|e: Event| {
            let select: web_sys::HtmlSelectElement = e.target_unchecked_into();
            let n: u32 = select.value().parse().unwrap_or(1);
            EpisodePickerMsg::SelectEpisode(n)
        });

        let can_confirm = self.selected_season.is_some() && self.selected_episode.is_some();

        html! {
            <div class="row-fill fill">
                <div class="row fill">
                    if let Some(ref label) = ctx.props().label {
                        <span class="fill">{&ctx.props().prompt}{" "}{label}{"?"}</span>
                    } else {
                        <span class="fill">{&ctx.props().prompt}{"?"}</span>
                    }

                    <select class="input-select" onchange={on_season_change}>
                        { for ctx.props().seasons.iter().map(|s| {
                            let value = s.number.to_u32().to_string();

                            let label = match s.number {
                                api::SeasonNumber::Specials => "Specials".to_string(),
                                api::SeasonNumber::Number(n) => format!("S{n:02}"),
                            };

                            let selected = self.selected_season == Some(s.number);
                            html! { <option {value} {selected}>{label}</option> }
                        }) }
                    </select>

                    <select class="input-select" onchange={on_episode_change} disabled={self.episodes.is_empty()}>
                        { for self.episodes.iter().map(|ep| {
                            let value = ep.number.to_string();
                            let label = format!("E{:02}", ep.number);
                            let selected = self.selected_episode == Some(ep.number);
                            html! { <option {value} {selected}>{label}</option> }
                        }) }
                    </select>
                </div>

                <div class="input-group end">
                    <button class="btn-icon" onclick={link.callback(|_| EpisodePickerMsg::Cancel)}
                        title="Cancel">
                        <span class="icon x-mark" />
                    </button>
                    <button class="btn-icon-success" onclick={link.callback(|_| EpisodePickerMsg::Confirm)}
                        title="Confirm" disabled={!can_confirm}>
                        <span class="icon check" />
                    </button>
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
                series_id: ctx.props().series_id,
                season,
            })
            .on_packet(ctx.link().callback(EpisodePickerMsg::EpisodesLoaded))
            .send();
    }
}
