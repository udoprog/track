use api::TimeInfo;
use musli_web::web03::prelude::*;
use yew::prelude::*;

use super::detail::{Graphics, ImageMsg, ImageUpdate, RemoteMsg, RemoteUpdate, Remotes};
use crate::SetupChannel;
use crate::background::Background;
use crate::error::{CustomContext, Error, Message};
use crate::router::{MediaQuery, Route, Router};
use crate::ui::{
    Button, ConfirmDanger, ContextMenu, DetailHero, DetailSkeleton, Image, Link, MarkTimeMenu,
    MediaSettingsModal, Modal, ReleaseModal, ReleaseTarget, RemoteEditor, RemoteSourceKind,
    SettingsTarget, TimePreset, Tracked, TranslatedText, TranslationsModal, Variant,
};

const CAP: usize = 8;

struct WatchedState {
    remove_watch_anchor: NodeRef,
    watched: api::Watched,
}

/// Load state for the movie this page renders.
enum MovieState {
    /// The initial request has not resolved yet.
    Loading,
    /// The backend confirmed there is no such movie (e.g. deleted or a
    /// hand-edited URL). Resolves itself if a matching create/change broadcast
    /// arrives.
    Missing,
    Loaded(Box<api::Movie>),
}

pub(crate) struct MovieDetail {
    channel: ws::Channel,
    movie: MovieState,
    graphics: Graphics,
    remotes: Remotes,
    credits: Vec<api::Credit>,
    /// Whether the full cast list is expanded past the initial cap.
    credits_expanded: bool,
    watched: Vec<WatchedState>,
    confirm_remove: bool,
    remove_anchor: NodeRef,
    confirm_remove_watch: Option<api::WatchedId>,
    syncing: bool,
    actions_expanded: bool,
    detailed_expand: bool,
    open_watched: bool,
    open_releases: bool,
    default_release_filters: api::FilterRules,
    global_sync_kinds: Vec<api::SourceSyncKinds>,
    image_modal: bool,
    settings_modal: bool,
    remote_editor: bool,
    translations_modal: bool,
    background: Background,
    router: Router,
    time: TimeInfo,
    _time_handle: ContextHandle<TimeInfo>,
    _setup: SetupChannel,
    _broadcast: ws::Listener,
    _movie_req: ws::Request,
    _credits_req: ws::Request,
    _watched_req: ws::Request,
    _mark_req: ws::Request,
    _remove_watch_req: ws::Request,
    _remove_req: ws::Request,
    _sync_req: ws::Request,
    _untrack_req: ws::Request,
    _pending_req: ws::Request,
    _config_req: ws::Request,
}

pub(crate) enum Msg {
    Channel(Result<ws::Channel, ws::Error>),
    AppBroadcast(Result<ws::Packet<api::AppBroadcast>, ws::Error>),
    SetTime(TimeInfo),
    Image(ImageMsg),
    Remote(RemoteMsg),
    Load(LoadMsg),
    Watch(WatchMsg),
    Action(ActionMsg),
    Ui(UiMsg),
}

/// Responses loading the page's data.
pub(crate) enum LoadMsg {
    Movie(Result<ws::Packet<api::GetMovie>, ws::Error>),
    Credits(Result<ws::Packet<api::ListCredits>, ws::Error>),
    Watched(Result<ws::Packet<api::ListWatched>, ws::Error>),
    Config(Result<ws::Packet<api::GetPreferences>, ws::Error>),
}

/// Marking the movie watched, and its pending (watch next) entry.
pub(crate) enum WatchMsg {
    MarkWatched(api::MarkTime),
    MarkWatchedDone(Result<ws::Packet<api::MarkWatched>, ws::Error>),
    RemoveWatched(api::WatchedId, api::WatchedKind),
    RemoveWatchedDone(Result<ws::Packet<api::RemoveWatched>, ws::Error>),
    ConfirmRemoveWatch(api::WatchedId),
    CancelRemoveWatch,
    OnWatchNext(api::MarkTime),
    AddPendingDone(Result<ws::Packet<api::AddPending>, ws::Error>),
    OnRemoveNext,
    RemovePendingDone(Result<ws::Packet<api::RemovePending>, ws::Error>),
}

/// Tracking, syncing and removing the movie.
pub(crate) enum ActionMsg {
    ConfirmRemove,
    CancelRemove,
    RemoveMovie,
    RemoveDone(Result<ws::Packet<api::RemoveMovie>, ws::Error>),
    SyncMovie,
    SyncDone(Result<ws::Packet<api::SyncMovie>, ws::Error>),
    SetTracked(bool),
    SetTrackedDone(bool, Result<ws::Packet<api::UntrackMovie>, ws::Error>),
}

/// Modals and expandable sections.
pub(crate) enum UiMsg {
    ToggleCreditsExpanded,
    OpenImageModal,
    CloseImageModal,
    OpenSettingsModal,
    CloseSettingsModal,
    OpenTranslations,
    CloseTranslations,
    OpenRemoteEditor,
    CloseRemoteEditor,
    ToggleActionsExpanded,
    ToggleDetailedActionsExpanded,
    ToggleOpenWatched,
    ToggleOpenReleases,
}

impl From<ImageMsg> for Msg {
    #[inline]
    fn from(msg: ImageMsg) -> Self {
        Msg::Image(msg)
    }
}

impl From<RemoteMsg> for Msg {
    #[inline]
    fn from(msg: RemoteMsg) -> Self {
        Msg::Remote(msg)
    }
}

impl From<LoadMsg> for Msg {
    #[inline]
    fn from(msg: LoadMsg) -> Self {
        Msg::Load(msg)
    }
}

impl From<WatchMsg> for Msg {
    #[inline]
    fn from(msg: WatchMsg) -> Self {
        Msg::Watch(msg)
    }
}

impl From<ActionMsg> for Msg {
    #[inline]
    fn from(msg: ActionMsg) -> Self {
        Msg::Action(msg)
    }
}

impl From<UiMsg> for Msg {
    #[inline]
    fn from(msg: UiMsg) -> Self {
        Msg::Ui(msg)
    }
}

#[derive(Properties, PartialEq)]
pub(crate) struct Props {
    pub(crate) movie_id: api::MovieId,
}

impl Component for MovieDetail {
    type Message = Msg;
    type Properties = Props;

    fn create(ctx: &Context<Self>) -> Self {
        let (ws, _) = ctx
            .link()
            .context::<ws::Handle>(Callback::noop())
            .expect("Expected ws::Handle in context");

        let _setup = SetupChannel::new(ws.clone(), ctx.link().callback(Msg::Channel));
        let _broadcast = ws.on_broadcast(ctx.link().callback(Msg::AppBroadcast));

        let (time, _time_handle) = ctx
            .link()
            .context::<TimeInfo>(ctx.link().callback(Msg::SetTime))
            .expect("Expected a configured time zone");

        let (background, _) = ctx
            .link()
            .context::<Background>(Callback::noop())
            .expect("Expected background handle in context");

        let (router, _) = ctx
            .link()
            .context::<Router>(Callback::noop())
            .expect("Expected router in context");

        Self {
            channel: ws::Channel::default(),
            movie: MovieState::Loading,
            graphics: Graphics::default(),
            remotes: Remotes::default(),
            credits: Vec::new(),
            credits_expanded: false,
            watched: Vec::new(),
            confirm_remove: false,
            remove_anchor: NodeRef::default(),
            confirm_remove_watch: None,
            syncing: false,
            actions_expanded: false,
            detailed_expand: false,
            open_watched: false,
            open_releases: false,
            default_release_filters: api::FilterRules::default_release_rules(),
            global_sync_kinds: Vec::new(),
            image_modal: false,
            settings_modal: false,
            remote_editor: false,
            translations_modal: false,
            background,
            router,
            time,
            _time_handle,
            _setup,
            _broadcast,
            _movie_req: ws::Request::default(),
            _credits_req: ws::Request::default(),
            _watched_req: ws::Request::default(),
            _mark_req: ws::Request::default(),
            _remove_watch_req: ws::Request::default(),
            _remove_req: ws::Request::default(),
            _sync_req: ws::Request::default(),
            _untrack_req: ws::Request::default(),
            _pending_req: ws::Request::default(),
            _config_req: ws::Request::default(),
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

    fn destroy(&mut self, _ctx: &Context<Self>) {
        self.background.title(None);
    }

    fn view(&self, ctx: &Context<Self>) -> Html {
        let movie = match &self.movie {
            MovieState::Loading => return html!(<DetailSkeleton />),
            MovieState::Missing => {
                return html! {
                    <div class="box info">
                        <span class="icon exclamation-triangle" aria-hidden="true" />
                        <span>{"No such movie"}</span>
                    </div>
                };
            }
            MovieState::Loaded(movie) => movie,
        };

        html! {
            <>
                { self.view_header(ctx, movie) }

                { self.view_body(ctx, movie) }
            </>
        }
    }

    fn changed(&mut self, ctx: &Context<Self>, old_props: &Props) -> bool {
        if ctx.props().movie_id != old_props.movie_id {
            self.movie = MovieState::Loading;
            self.watched.clear();
            self.confirm_remove = false;
            self.confirm_remove_watch = None;

            if self.channel.id() != ws::ChannelId::NONE {
                self.load_movie(ctx);
                self.load_credits(ctx);
                self.load_watched(ctx);
            }
        }

        true
    }
}

impl MovieDetail {
    fn try_update(&mut self, ctx: &Context<Self>, msg: Msg) -> Result<bool, Error> {
        match msg {
            Msg::Channel(result) => {
                self.channel = result?;
                if self.channel.id() != ws::ChannelId::NONE {
                    self.load_movie(ctx);
                    self.load_credits(ctx);
                    self.load_watched(ctx);
                    self.load_config(ctx);
                } else {
                    self.movie = MovieState::Loading;
                    self.watched.clear();
                }
                Ok(true)
            }
            Msg::AppBroadcast(packet) => self.on_broadcast(ctx, packet),
            Msg::SetTime(time) => {
                self.time = time;
                Ok(true)
            }
            Msg::Image(msg) => self.update_image(ctx, msg),
            Msg::Remote(msg) => self.update_remote(ctx, msg),
            Msg::Load(msg) => self.update_load(msg),
            Msg::Watch(msg) => self.update_watch(ctx, msg),
            Msg::Action(msg) => self.update_action(ctx, msg),
            Msg::Ui(msg) => self.update_ui(msg),
        }
    }

    fn on_broadcast(
        &mut self,
        ctx: &Context<Self>,
        packet: Result<ws::Packet<api::AppBroadcast>, ws::Error>,
    ) -> Result<bool, Error> {
        let event = packet?.decode_event()?;

        if event.channel == self.channel.id() {
            return Ok(false);
        }

        match &event.kind {
            api::AppEventKind::MovieChanged { movie }
            | api::AppEventKind::MovieCreated { movie }
                if movie.id == ctx.props().movie_id =>
            {
                self.set_movie(movie.clone());
                Ok(true)
            }
            api::AppEventKind::MovieDeleted { movie_id } if *movie_id == ctx.props().movie_id => {
                self.router.push(Route::Media(MediaQuery::default()));
                Ok(false)
            }
            api::AppEventKind::CreditsChanged {
                target: api::TranslationTarget::Movie(movie_id),
            } if *movie_id == ctx.props().movie_id => {
                if self.channel.id() != ws::ChannelId::NONE {
                    self.load_credits(ctx);
                }

                Ok(false)
            }
            api::AppEventKind::PersonChanged { person_id }
                if self.credits.iter().any(|c| c.person_id == *person_id) =>
            {
                if self.channel.id() != ws::ChannelId::NONE {
                    self.load_credits(ctx);
                }

                Ok(false)
            }
            api::AppEventKind::WatchedChanged { event: kind } => {
                let relevant = matches!(
                    kind,
                    api::WatchedEvent::Movie { movie } if *movie == ctx.props().movie_id
                );

                if relevant && self.channel.id() != ws::ChannelId::NONE {
                    self.load_movie(ctx);
                    self.load_watched(ctx);
                }

                Ok(false)
            }
            api::AppEventKind::TaskAdded { task } | api::AppEventKind::TaskStarted { task } => {
                if matches!(&task.kind, api::TaskKind::SyncMovie { movie_id, .. } if *movie_id == ctx.props().movie_id)
                {
                    self.syncing = true;
                    return Ok(true);
                }
                Ok(false)
            }
            api::AppEventKind::TaskCompleted { task } => {
                if matches!(&task.kind, api::TaskKind::SyncMovie { movie_id, .. } if *movie_id == ctx.props().movie_id)
                {
                    self.syncing = false;
                    self.load_movie(ctx);
                    return Ok(true);
                }
                Ok(false)
            }
            _ => Ok(false),
        }
    }

    fn update_image(&mut self, ctx: &Context<Self>, msg: ImageMsg) -> Result<bool, Error> {
        let owner = api::ImageOwner::Movie(ctx.props().movie_id);

        match self
            .graphics
            .update(ctx.link(), &self.channel, owner, msg)?
        {
            ImageUpdate::Render(render) => Ok(render),
            ImageUpdate::Reload => {
                self.load_movie(ctx);
                Ok(true)
            }
            ImageUpdate::Cleared => {
                self.image_modal = false;
                self.load_movie(ctx);
                Ok(true)
            }
        }
    }

    fn update_remote(&mut self, ctx: &Context<Self>, msg: RemoteMsg) -> Result<bool, Error> {
        let remotes = match &mut self.movie {
            MovieState::Loaded(movie) => Some(&mut movie.remotes),
            _ => None,
        };

        match self.remotes.update(
            ctx.link(),
            &self.channel,
            ctx.props().movie_id,
            remotes,
            msg,
        )? {
            RemoteUpdate::Render(render) => Ok(render),
            RemoteUpdate::Reload => {
                self.load_movie(ctx);
                Ok(false)
            }
        }
    }

    fn update_load(&mut self, msg: LoadMsg) -> Result<bool, Error> {
        match msg {
            LoadMsg::Movie(result) => {
                let movie = result
                    .context(Message::LoadingMovies)?
                    .decode()
                    .context(Message::LoadingMovies)?;

                match movie {
                    Some(movie) => self.set_movie(movie),
                    None => {
                        self.background.background(None);
                        self.background.title(None);
                        self.movie = MovieState::Missing;
                        self.update_graphics();
                    }
                }

                Ok(true)
            }
            LoadMsg::Credits(result) => {
                self.credits = result
                    .context(Message::LoadingCredits)?
                    .decode()
                    .context(Message::LoadingCredits)?
                    .credits;

                Ok(true)
            }
            LoadMsg::Watched(result) => {
                let watched = result
                    .context(Message::LoadingWatched)?
                    .decode()
                    .context(Message::LoadingWatched)?
                    .watched;

                self.watched.clear();

                for w in watched {
                    self.watched.push(WatchedState {
                        remove_watch_anchor: NodeRef::default(),
                        watched: w,
                    });
                }

                Ok(true)
            }
            LoadMsg::Config(result) => {
                let site = result
                    .context(Message::LoadingConfig)?
                    .decode()
                    .context(Message::LoadingConfig)?
                    .site;
                self.default_release_filters = site.release_filters;
                self.global_sync_kinds = site.sync_kinds;
                Ok(true)
            }
        }
    }

    fn update_watch(&mut self, ctx: &Context<Self>, msg: WatchMsg) -> Result<bool, Error> {
        match msg {
            WatchMsg::MarkWatched(mark_time) => {
                let movie = ctx.props().movie_id;

                if self.channel.id() != ws::ChannelId::NONE {
                    self._mark_req = self
                        .channel
                        .request()
                        .body(api::MarkWatchedRequest {
                            kind: api::WatchedKind::Movie { movie },
                            mark_time,
                        })
                        .on_packet(ctx.link().callback(WatchMsg::MarkWatchedDone))
                        .send();
                }

                Ok(true)
            }
            WatchMsg::MarkWatchedDone(result) => {
                let response = result
                    .context(Message::MarkingWatched)?
                    .decode()
                    .context(Message::MarkingWatched)?;

                self.background.offer_undo(&response);
                self.load_movie(ctx);
                self.load_watched(ctx);
                Ok(false)
            }
            WatchMsg::RemoveWatched(id, kind) => {
                if self.channel.id() != ws::ChannelId::NONE {
                    self._remove_watch_req = self
                        .channel
                        .request()
                        .body(api::RemoveWatchedRequest { id, kind })
                        .on_packet(ctx.link().callback(WatchMsg::RemoveWatchedDone))
                        .send();
                }

                Ok(false)
            }
            WatchMsg::RemoveWatchedDone(result) => {
                result.context(Message::RemovingWatched)?;
                self.confirm_remove_watch = None;
                self.load_movie(ctx);
                self.load_watched(ctx);
                Ok(false)
            }
            WatchMsg::ConfirmRemoveWatch(watched_id) => {
                self.confirm_remove_watch = Some(watched_id);
                Ok(true)
            }
            WatchMsg::CancelRemoveWatch => {
                self.confirm_remove_watch = None;
                Ok(true)
            }
            WatchMsg::OnWatchNext(mark_time) => {
                let movie = ctx.props().movie_id;

                if self.channel.id() != ws::ChannelId::NONE {
                    self._pending_req = self
                        .channel
                        .request()
                        .body(api::AddPendingRequest {
                            kind: api::PendingKind::Movie { movie },
                            mark_time,
                        })
                        .on_packet(ctx.link().callback(WatchMsg::AddPendingDone))
                        .send();
                }

                Ok(true)
            }
            WatchMsg::AddPendingDone(result) => {
                let packet = result
                    .context(Message::AddingPending)?
                    .decode()
                    .context(Message::AddingPending)?;

                if let MovieState::Loaded(movie) = &mut self.movie {
                    movie.pending = packet.pending.map(|p| p.timestamp);
                }

                Ok(true)
            }
            WatchMsg::OnRemoveNext => {
                let movie = ctx.props().movie_id;

                if self.channel.id() != ws::ChannelId::NONE {
                    self._pending_req = self
                        .channel
                        .request()
                        .body(api::RemovePendingRequest {
                            kind: api::PendingKind::Movie { movie },
                        })
                        .on_packet(ctx.link().callback(WatchMsg::RemovePendingDone))
                        .send();
                }

                Ok(false)
            }
            WatchMsg::RemovePendingDone(result) => {
                _ = result
                    .context(Message::RemovingPending)?
                    .decode()
                    .context(Message::RemovingPending)?;

                if let MovieState::Loaded(movie) = &mut self.movie {
                    movie.pending = None;
                }

                Ok(true)
            }
        }
    }

    fn update_action(&mut self, ctx: &Context<Self>, msg: ActionMsg) -> Result<bool, Error> {
        match msg {
            ActionMsg::ConfirmRemove => {
                self.confirm_remove = true;
                Ok(true)
            }
            ActionMsg::CancelRemove => {
                self.confirm_remove = false;
                Ok(true)
            }
            ActionMsg::RemoveMovie => {
                let id = ctx.props().movie_id;

                if self.channel.id() != ws::ChannelId::NONE {
                    self._remove_req = self
                        .channel
                        .request()
                        .body(api::RemoveMovieRequest { id })
                        .on_packet(ctx.link().callback(ActionMsg::RemoveDone))
                        .send();
                }

                Ok(false)
            }
            ActionMsg::RemoveDone(result) => {
                result.context(Message::RemovingMovie)?;
                self.router.push(Route::Media(MediaQuery::default()));
                Ok(false)
            }
            ActionMsg::SyncMovie => {
                let id = ctx.props().movie_id;

                if self.channel.id() != ws::ChannelId::NONE {
                    self._sync_req = self
                        .channel
                        .request()
                        .body(api::SyncMovieRequest { id })
                        .on_packet(ctx.link().callback(ActionMsg::SyncDone))
                        .send();
                }

                Ok(true)
            }
            ActionMsg::SyncDone(result) => {
                result.context(Message::SyncingMovie)?;
                Ok(false)
            }
            ActionMsg::SetTracked(tracked) => {
                self.actions_expanded = false;

                let id = ctx.props().movie_id;

                if self.channel.id() != ws::ChannelId::NONE {
                    self._untrack_req = self
                        .channel
                        .request()
                        .body(api::UntrackMovieRequest { id, tracked })
                        .on_packet(
                            ctx.link()
                                .callback(move |r| ActionMsg::SetTrackedDone(tracked, r)),
                        )
                        .send();
                }

                Ok(false)
            }
            ActionMsg::SetTrackedDone(tracked, result) => {
                result.context(Message::UntrackingMovie)?;

                if let MovieState::Loaded(movie) = &mut self.movie {
                    movie.tracked = tracked;
                }

                Ok(true)
            }
        }
    }

    fn update_ui(&mut self, msg: UiMsg) -> Result<bool, Error> {
        match msg {
            UiMsg::ToggleCreditsExpanded => {
                self.credits_expanded = !self.credits_expanded;
                Ok(true)
            }
            UiMsg::OpenImageModal => {
                self.image_modal = true;
                self.settings_modal = false;
                Ok(true)
            }
            UiMsg::CloseImageModal => {
                self.image_modal = false;
                // Opened from Settings, so closing goes back there.
                self.settings_modal = true;
                Ok(true)
            }
            UiMsg::OpenSettingsModal => {
                self.settings_modal = true;
                self.actions_expanded = false;
                Ok(true)
            }
            UiMsg::CloseSettingsModal => {
                self.settings_modal = false;
                Ok(true)
            }
            UiMsg::OpenTranslations => {
                self.translations_modal = true;
                self.actions_expanded = false;
                Ok(true)
            }
            UiMsg::CloseTranslations => {
                self.translations_modal = false;
                Ok(true)
            }
            UiMsg::OpenRemoteEditor => {
                self.remote_editor = true;
                self.settings_modal = false;
                self.actions_expanded = false;
                Ok(true)
            }
            UiMsg::CloseRemoteEditor => {
                self.remote_editor = false;
                // Opened from Settings, so closing goes back there.
                self.settings_modal = true;
                Ok(true)
            }
            UiMsg::ToggleActionsExpanded => {
                self.actions_expanded = !self.actions_expanded;
                Ok(true)
            }
            UiMsg::ToggleDetailedActionsExpanded => {
                self.detailed_expand = !self.detailed_expand;
                Ok(true)
            }
            UiMsg::ToggleOpenWatched => {
                self.open_watched = !self.open_watched;
                Ok(true)
            }
            UiMsg::ToggleOpenReleases => {
                self.open_releases = !self.open_releases;
                Ok(true)
            }
        }
    }

    fn set_movie(&mut self, movie: api::Movie) {
        self.background
            .background(movie.backdrop.as_ref().map(|i| i.proxy_url()));
        self.background
            .title(movie.strings.title().map(str::to_owned));
        self.movie = MovieState::Loaded(Box::new(movie));
        self.update_graphics();
    }

    fn update_graphics(&mut self) {
        match &self.movie {
            MovieState::Loaded(movie) => self
                .graphics
                .set(&movie.images, |kind, key| movie.is_selected(kind, key)),
            _ => self.graphics.clear(),
        }
    }

    fn load_movie(&mut self, ctx: &Context<Self>) {
        if self.channel.id() == ws::ChannelId::NONE {
            return;
        }

        self._movie_req = self
            .channel
            .request()
            .body(api::GetMovieRequest {
                id: ctx.props().movie_id,
            })
            .on_packet(ctx.link().callback(LoadMsg::Movie))
            .send();
    }

    fn load_credits(&mut self, ctx: &Context<Self>) {
        if self.channel.id() == ws::ChannelId::NONE {
            return;
        }

        self._credits_req = self
            .channel
            .request()
            .body(api::ListCreditsRequest {
                owner: api::CreditOwner::Movie(ctx.props().movie_id),
            })
            .on_packet(ctx.link().callback(LoadMsg::Credits))
            .send();
    }

    /// Cast grid plus a compact key-crew line. Cast is capped until expanded.
    fn view_credits(&self, ctx: &Context<Self>) -> Html {
        if self.credits.is_empty() {
            return html! {};
        }

        let shown = if self.credits_expanded {
            self.credits.as_slice()
        } else {
            self.credits
                .get(..CAP.min(self.credits.len()))
                .unwrap_or_default()
        };

        html! {
            <section class="credits">
                <h2>{"Cast & crew"}</h2>

                if !shown.is_empty() {
                    <div class="cast-grid">
                        { for shown.iter().map(|c| self.view_credit_card(c, c.character.character().or(c.job.as_deref()))) }
                    </div>
                }

                if self.credits.len() > CAP {
                    <Button icon={if self.credits_expanded { "chevron-up" } else { "chevron-down" }} label={if self.credits_expanded { "Show fewer" } else { "Show all cast" }} title={if self.credits_expanded { "Show fewer cast" } else { "Show all cast" }} class="credits-toggle" expanded={Some(self.credits_expanded)} onclick={ctx.link().callback(|_| UiMsg::ToggleCreditsExpanded)} />
                }
            </section>
        }
    }

    /// A clickable credit card - photo, name and a subtitle (the character for cast,
    /// the job for crew) - that navigates to the person's page.
    fn view_credit_card(&self, credit: &api::Credit, subtitle: Option<&str>) -> Html {
        let name = credit.name.title().unwrap_or("Unknown").to_owned();
        let subtitle = subtitle.map(str::to_owned);

        html! {
            <Link to={Route::PersonDetail(credit.person_id)} class="cast-card">
                <Image class="cast-photo" placeholder={true} placeholder_icon="user" src={credit.profile.clone()} alt={name.clone()} />

                <div class="cast-info">
                    <div class="cast-name">{ name }</div>

                    if let Some(subtitle) = subtitle {
                        <div class="cast-character">{ subtitle }</div>
                    }
                </div>
            </Link>
        }
    }

    fn load_watched(&mut self, ctx: &Context<Self>) {
        if self.channel.id() == ws::ChannelId::NONE {
            return;
        }

        let movie = ctx.props().movie_id;
        self._watched_req = self
            .channel
            .request()
            .body(api::ListWatchedRequest {
                kind: api::WatchedKind::Movie { movie },
            })
            .on_packet(ctx.link().callback(LoadMsg::Watched))
            .send();
    }

    fn load_config(&mut self, ctx: &Context<Self>) {
        if self.channel.id() == ws::ChannelId::NONE {
            return;
        }

        self._config_req = self
            .channel
            .request()
            .body(api::GetPreferencesRequest)
            .on_packet(ctx.link().callback(LoadMsg::Config))
            .send();
    }

    fn view_header(&self, ctx: &Context<Self>, movie: &api::Movie) -> Html {
        let link = ctx.link();

        // The source of the earliest considered release, i.e. the one that determines
        // the effective release date shown below.
        let filters = movie.effective_release_filters(&self.default_release_filters);
        let release_source = movie
            .releases
            .iter()
            .filter(|r| filters.release_accepted(r))
            .min_by_key(|r| r.timestamp)
            .map(|r| r.source);

        html! {
            <div class="column">
                <DetailHero
                    title={movie.strings.title().unwrap_or("Untitled Movie").to_owned()}
                    meta={movie.release_date.map(|ts| ts.date(self.time.clone()).year().to_string())}
                    backdrop={movie.backdrop.clone()}
                />

                <div class="toolbar">
                    <div class="row detail-sources">
                        {for movie.remotes.iter().filter_map(|r| {
                            let url = r.remote.movie_url()?;
                            let id = r.remote.source().as_id();

                            Some(html! {
                                <a class="item-inline-source" href={url} target="_blank" rel="noopener noreferrer" title={format!("Open on {id}")}>
                                    <span class={classes!("logo", id)} />
                                </a>
                            })
                        })}
                    </div>

                    <div class="toolbar-toggle">
                        <Button icon={if self.actions_expanded { "ellipsis-horizontal" } else { "bars-3" }} title="Actions" expanded={Some(self.actions_expanded)} onclick={link.callback(|_| UiMsg::ToggleActionsExpanded)} />
                    </div>

                    <div class={classes!("toolbar-dropdown", (!self.actions_expanded).then_some("desktop-only"))}>
                        <div class="desktop-row mobile-column desktop-input-group">
                            <Tracked kind="movie" tracked={movie.tracked} ontoggle={link.callback(ActionMsg::SetTracked)} />

                            if !movie.remotes.is_empty() {
                                <Button icon="arrow-path" spin={self.syncing} onclick={link.callback(|_| ActionMsg::SyncMovie)} title="Sync now" text="Sync" />
                            }

                            <Button icon="language" title="Translations" text="Translations" onclick={link.callback(|_| UiMsg::OpenTranslations)} />

                            <Button icon="cog-6-tooth" title="Settings" text="Settings" onclick={link.callback(|_| UiMsg::OpenSettingsModal)} />

                            if crate::is_admin(ctx) {
                                <Button node_ref={self.remove_anchor.clone()} icon="trash" variant={Variant::Danger} class="detached" title="Remove movie" text="Remove" expanded={Some(self.confirm_remove)} haspopup="dialog" onclick={link.callback(|_| ActionMsg::ConfirmRemove)} />

                                if self.confirm_remove {
                                    <ContextMenu prompt="Remove movie" label={movie.strings.title().map(str::to_owned)} anchor={self.remove_anchor.clone()} on_close={link.callback(|_| ActionMsg::CancelRemove)}>
                                        <ConfirmDanger
                                            on_confirm={link.callback(|_| ActionMsg::RemoveMovie)}
                                            on_cancel={link.callback(|_| ActionMsg::CancelRemove)}
                                        />
                                    </ContextMenu>
                                }
                            }
                        </div>
                    </div>
                </div>

                <indicator title="Release date">
                    <span class="item-inline">
                        <span class={classes!("icon", if movie.release_date.is_some() { "clock" } else { "exclamation-circle" })} />
                    </span>

                    <content>
                        if let Some(ts) = movie.release_date {
                            <span>{if self.time.now() < ts { "Releases" } else { "Released" }}</span>
                            {ts.human_date_time(self.time.clone()).lower().view()}

                            if let Some(source) = release_source {
                                <span class="item-inline" title={source.as_label()}>
                                    <span class={classes!("logo", source.as_id())} />
                                </span>
                            }
                        } else {
                            <span class="text-muted">{"No release date"}</span>
                        }
                    </content>
                </indicator>
            </div>
        }
    }

    fn view_body(&self, ctx: &Context<Self>, movie: &api::Movie) -> Html {
        let link = ctx.link();

        // Earliest release, used to pre-fill the "Released" quick option.
        let release_at = movie.releases.iter().map(|r| r.timestamp).min();

        // Mark-watched only offers the release date once the movie is actually out.
        let watched_preset = release_at
            .filter(|&r| r <= self.time.now())
            .map(|ts| TimePreset::at("calendar", "Released", ts));

        let release_preset = release_at.map(|ts| TimePreset::at("calendar", "Released", ts));

        let on_remove_next = link.callback(move |_| WatchMsg::OnRemoveNext);

        html! {
            <>
            <TranslatedText strings={movie.strings.clone()} />

            <div class="detail-layout">
                <div class="mobile-only">
                    if let Some(ref banner) = movie.banner {
                        <Image class="banner" src={banner.clone()} />
                    }
                </div>

                <div class="detail-sidebar">
                    <Image class="poster desktop-only artwork" src={movie.poster.clone()} />
                </div>

                <div class="detail-content">
                    <div class="row-split">
                        <div class="column fill">
                            <div class="toolbar">
                                <indicator title="Release date">
                                    if movie.pending.is_some() {
                                        <span class="item-inline" title="Next movie">
                                            <span class="icon primary exclamation-circle" aria-hidden="true" />
                                        </span>
                                    } else if !self.watched.is_empty() {
                                        <span class="item-inline" title="Watched">
                                            <span class="icon primary check-circle" aria-hidden="true" />
                                        </span>
                                    } else {
                                        <span class="item-inline" title="Never watched">
                                            <span class="icon secondary x-circle" aria-hidden="true" />
                                        </span>
                                    }

                                    <content>
                                        if let Some(ts) = movie.pending {
                                            <span>{"Movie scheduled for"}</span>
                                            {ts.human_date_time(self.time.clone()).lower().view()}
                                        } else {
                                            {match &self.watched[..] {
                                                [] => html!(<span>{"Never watched"}</span>),
                                                [w] => html! {
                                                    <>
                                                        <span>{"Watched once"}</span>
                                                        {w.watched.timestamp.human_date_time(self.time.clone()).lower().view()}
                                                    </>
                                                },
                                                [w, ..] => html! {
                                                    <>
                                                        {format!("Watched {} times, first", self.watched.len())}
                                                        {w.watched.timestamp.human_date_time(self.time.clone()).view()}
                                                    </>
                                                }
                                            }}
                                        }
                                    </content>
                                </indicator>

                                <div class="toolbar-toggle">
                                    <Button icon={if self.detailed_expand { "ellipsis-horizontal" } else { "bars-3" }} title="Watch actions" onclick={link.callback(move |_| UiMsg::ToggleDetailedActionsExpanded)} />
                                </div>

                                <div class={classes!("toolbar-dropdown", "desktop-input-group", (!self.detailed_expand).then_some("desktop-only"))}>
                                    <MarkTimeMenu quick=true class="primary" icon="check" title="Mark watched" prompt="When did you watch the movie?" preset={watched_preset.clone()} on_confirm={link.callback(WatchMsg::MarkWatched)} text="Mark watched" />

                                    if movie.pending.is_some() {
                                        <Button icon="bookmark" variant={Variant::Primary} title="Next movie" text="Next movie" onclick={on_remove_next} />
                                    } else {
                                        <MarkTimeMenu class="has-text" title="Not next movie" prompt="When do you want to watch the movie?" preset={release_preset.clone()} on_confirm={link.callback(WatchMsg::OnWatchNext)}>
                                            <span class="icon bookmark-slash" aria-hidden="true" />
                                            <span class="mobile-only">{"Not next movie"}</span>
                                        </MarkTimeMenu>
                                    }

                                    <Button icon="clock" title="Watch history" text="Watch history" onclick={link.callback(|_| UiMsg::ToggleOpenWatched)} />

                                    <Button icon="calendar" title="Releases" text="Releases" onclick={link.callback(|_| UiMsg::ToggleOpenReleases)} />
                                </div>
                            </div>
                        </div>
                    </div>

                    if self.open_watched {
                        <Modal icon="clock" title="Watch history" on_close={link.callback(|_| UiMsg::ToggleOpenWatched)}>
                            {self.view_watched(ctx, movie.id)}
                        </Modal>
                    }

                    if self.open_releases {
                        <ReleaseModal
                            target={ReleaseTarget::Movie(movie.id)}
                            title="Releases"
                            on_close={link.callback(|_| UiMsg::ToggleOpenReleases)}
                        />
                    }

                    { self.view_credits(ctx) }
                </div>
            </div>

            if self.image_modal {
                { self.view_image_modal(ctx) }
            }

            if self.translations_modal {
                <TranslationsModal
                    target={api::TranslationTarget::Movie(movie.id)}
                    on_close={link.callback(|_| UiMsg::CloseTranslations)}
                />
            }

            if self.settings_modal {
                <MediaSettingsModal
                    target={SettingsTarget::Movie(movie.id)}
                    on_edit_graphics={link.callback(|_| UiMsg::OpenImageModal)}
                    on_edit_remotes={link.callback(|_| UiMsg::OpenRemoteEditor)}
                    on_close={link.callback(|_| UiMsg::CloseSettingsModal)}
                />
            }

            if self.remote_editor {
                <RemoteEditor
                    title={movie.strings.title().unwrap_or("Untitled Movie").to_owned()}
                    kind={RemoteSourceKind::Movie}
                    remotes={movie.remotes.clone()}
                    on_add={link.callback(|(slug, remote)| RemoteMsg::AddRemote(slug, remote))}
                    on_edit={link.callback(|(id, slug, remote)| RemoteMsg::EditRemote(id, slug, remote))}
                    on_remove={link.callback(RemoteMsg::RemoveRemote)}
                    on_purge_cache={link.callback(RemoteMsg::PurgeRemoteCache)}
                    on_set_enabled={link.callback(|(id, enabled)| RemoteMsg::SetRemoteEnabled(id, enabled))}
                    on_reorder={link.callback(RemoteMsg::ReorderRemotes)}
                    on_set_sync_kinds={link.callback(|(id, kinds)| RemoteMsg::SetRemoteSyncKinds(id, kinds))}
                    global_sync_kinds={self.global_sync_kinds.clone()}
                    on_close={link.callback(|_| UiMsg::CloseRemoteEditor)}
                />
            }
            </>
        }
    }

    fn view_watched(&self, ctx: &Context<Self>, movie_id: api::MovieId) -> Html {
        let link = ctx.link();

        html! {
            <div class="watch-history">
                { for self.watched.iter().map(|w| {
                    let wid = w.watched.id;
                    let kind = api::WatchedKind::Movie { movie: movie_id };

                    html! {
                        <div class="watch-row">
                            <span class="watch-when">{w.watched.timestamp.human_date_time(self.time.clone()).view()}</span>
                            <span class="watch-age">{w.watched.timestamp.relative_to(self.time.now())}</span>

                            <div class="watch-actions">
                            <Button node_ref={w.remove_watch_anchor.clone()} icon="trash" label="Remove" title="Remove" expanded={Some(self.confirm_remove_watch == Some(wid))} haspopup="dialog" onclick={link.callback(move |_| WatchMsg::ConfirmRemoveWatch(wid))} />

                            if self.confirm_remove_watch == Some(wid) {
                                <ContextMenu prompt="Remove watch at" label={w.watched.timestamp.human_date_time(self.time.clone())} anchor={w.remove_watch_anchor.clone()} on_close={link.callback(|_| WatchMsg::CancelRemoveWatch)}>
                                    <ConfirmDanger
                                        on_confirm={link.callback(move |_| WatchMsg::RemoveWatched(wid, kind))}
                                        on_cancel={link.callback(|_| WatchMsg::CancelRemoveWatch)}
                                    />
                                </ContextMenu>
                            }
                            </div>
                        </div>
                    }
                }) }
            </div>
        }
    }

    fn view_image_modal(&self, ctx: &Context<Self>) -> Html {
        let user_selected = |kind: api::ImageKind| match &self.movie {
            MovieState::Loaded(movie) => movie.is_user_selected(kind),
            _ => false,
        };

        self.graphics.view_modal(
            ctx.link(),
            user_selected,
            ctx.link().callback(|_| UiMsg::CloseImageModal),
        )
    }
}
