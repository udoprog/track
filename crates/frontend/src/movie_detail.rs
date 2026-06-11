use api::TimeZone;
use musli_web::web03::prelude::*;
use yew::prelude::*;

use crate::SetupChannel;
use crate::error::{CustomContext, Error, Message, RcError};
use crate::router::{PagedQuery, Route};
use crate::ui::{
    ConfirmDanger, ErrorBox, ImageGallery, ImageItem, LanguagePicker, LoadingPage,
    MarkWatchedPicker, RemoteSourceKind, RemoteSourceSelect, Tracked,
};

pub(super) struct MovieDetail {
    channel: ws::Channel,
    movie: Option<api::Movie>,
    watched: Vec<api::Watched>,
    confirm_remove: bool,
    confirm_mark_watch: bool,
    confirm_remove_watch: Option<api::WatchedId>,
    syncing: bool,
    actions_expanded: bool,
    detailed_expand: bool,
    image_modal: Option<api::ImageKind>,
    tz: TimeZone,
    _tz_handle: ContextHandle<TimeZone>,
    _setup: SetupChannel,
    _broadcast: ws::Listener,
    _movie_req: ws::Request,
    _watched_req: ws::Request,
    _mark_req: ws::Request,
    _remove_watch_req: ws::Request,
    _remove_req: ws::Request,
    _sync_req: ws::Request,
    _untrack_req: ws::Request,
    _pending_req: ws::Request,
    _select_image_req: ws::Request,
    _clear_image_req: ws::Request,
    _set_sync_source_req: ws::Request,
    _set_language_req: ws::Request,
}

pub(super) enum Msg {
    Channel(Result<ws::Channel, ws::Error>),
    AppBroadcast(Result<ws::Packet<api::AppBroadcast>, ws::Error>),
    MovieLoaded(Result<ws::Packet<api::GetMovie>, ws::Error>),
    WatchedLoaded(Result<ws::Packet<api::ListWatched>, ws::Error>),
    AskMarkWatched,
    CancelMarkWatch,
    MarkWatched(api::MarkTime),
    MarkWatchedDone(Result<ws::Packet<api::MarkWatched>, ws::Error>),
    RemoveWatched(api::WatchedId, api::WatchedKind),
    RemoveWatchedDone(Result<ws::Packet<api::RemoveWatched>, ws::Error>),
    ConfirmRemoveWatch(api::WatchedId),
    CancelRemoveWatch,
    SelectImage(api::ImageId),
    ClearSelectedImage(api::ImageKind),
    SelectImageDone(Result<ws::Packet<api::SelectImage>, ws::Error>),
    ClearSelectedImageDone(Result<ws::Packet<api::ClearSelectedImage>, ws::Error>),
    OpenImageModal(api::ImageKind),
    CloseImageModal,
    ConfirmRemove,
    CancelRemove,
    RemoveMovie,
    RemoveDone(Result<ws::Packet<api::RemoveMovie>, ws::Error>),
    SyncMovie,
    SyncDone(Result<ws::Packet<api::SyncMovie>, ws::Error>),
    SetSyncSource(api::SyncSource),
    SetSyncSourceDone(
        api::SyncSource,
        Result<ws::Packet<api::SetMovieSyncSource>, ws::Error>,
    ),
    SetLanguage(Option<String>),
    SetLanguageDone(
        Option<String>,
        Result<ws::Packet<api::SetMovieLanguage>, ws::Error>,
    ),
    SetTracked(bool),
    SetTrackedDone(bool, Result<ws::Packet<api::UntrackMovie>, ws::Error>),
    OnWatchNext,
    AddPendingDone(Result<ws::Packet<api::AddPending>, ws::Error>),
    OnRemoveNext,
    RemovePendingDone(Result<ws::Packet<api::RemovePending>, ws::Error>),
    SetTz(TimeZone),
    ToggleActionsExpanded,
    ToggleDetailedActionsExpanded,
}

#[derive(Properties, PartialEq)]
pub(super) struct Props {
    pub(super) error: Option<RcError>,
    pub(super) onerror: Callback<Option<Error>>,
    pub(super) movie_id: api::MovieId,
    pub(super) on_navigate: Callback<Route>,
}

impl Component for MovieDetail {
    type Message = Msg;
    type Properties = Props;

    fn create(ctx: &Context<Self>) -> Self {
        let (ws, _) = ctx
            .link()
            .context::<ws::Handle>(Callback::noop())
            .expect("ws::Handle context not found");

        let _setup = SetupChannel::new(ws.clone(), ctx.link().callback(Msg::Channel));
        let _broadcast = ws.on_broadcast(ctx.link().callback(Msg::AppBroadcast));

        let (tz, _tz_handle) = ctx
            .link()
            .context::<TimeZone>(ctx.link().callback(Msg::SetTz))
            .expect("time zone not found");

        Self {
            channel: ws::Channel::default(),
            movie: None,
            watched: Vec::new(),
            confirm_remove: false,
            confirm_mark_watch: false,
            confirm_remove_watch: None,
            syncing: false,
            actions_expanded: false,
            detailed_expand: false,
            image_modal: None,
            tz,
            _tz_handle,
            _setup,
            _broadcast,
            _movie_req: ws::Request::default(),
            _watched_req: ws::Request::default(),
            _mark_req: ws::Request::default(),
            _remove_watch_req: ws::Request::default(),
            _remove_req: ws::Request::default(),
            _sync_req: ws::Request::default(),
            _untrack_req: ws::Request::default(),
            _pending_req: ws::Request::default(),
            _select_image_req: ws::Request::default(),
            _clear_image_req: ws::Request::default(),
            _set_sync_source_req: ws::Request::default(),
            _set_language_req: ws::Request::default(),
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

    fn view(&self, ctx: &Context<Self>) -> Html {
        let Some(ref movie) = self.movie else {
            return html!(<LoadingPage />);
        };

        let url = movie.backdrop.as_ref().map(|i| i.proxy_url());

        let style = url
            .as_ref()
            .map(|url| format!("--background: url('{}')", url))
            .unwrap_or_default();

        html! {
            <div class="page-container" {style}>
                <div class="page">
                    if let Some(ref error) = ctx.props().error {
                        <ErrorBox error={error.clone()} onclearerror={ctx.props().onerror.reform(|()| None)} />
                    }

                    { self.view_header(ctx, movie) }

                    { self.view_body(ctx, movie) }
                </div>
            </div>
        }
    }

    fn changed(&mut self, ctx: &Context<Self>, old_props: &Props) -> bool {
        if ctx.props().movie_id != old_props.movie_id {
            self.movie = None;
            self.watched.clear();
            self.confirm_remove = false;
            self.confirm_mark_watch = false;
            self.confirm_remove_watch = None;

            if self.channel.id() != ws::ChannelId::NONE {
                self.load_movie(ctx);
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
                    self.load_watched(ctx);
                } else {
                    self.movie = None;
                    self.watched.clear();
                }
                Ok(true)
            }
            Msg::AppBroadcast(packet) => {
                let event = packet?.decode_event()?;

                if event.channel == self.channel.id() {
                    return Ok(false);
                }

                match &event.kind {
                    api::AppEventKind::MovieChanged { movie }
                        if movie.id == ctx.props().movie_id =>
                    {
                        self.movie = Some(movie.clone());
                        Ok(true)
                    }
                    api::AppEventKind::MovieDeleted { movie_id }
                        if *movie_id == ctx.props().movie_id =>
                    {
                        ctx.props()
                            .on_navigate
                            .emit(Route::Movies(PagedQuery::default()));
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
                    api::AppEventKind::TaskAdded { task }
                    | api::AppEventKind::TaskStarted { task } => {
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
            Msg::MovieLoaded(result) => {
                self.movie = Some(
                    result
                        .context(Message::LoadingMovies)?
                        .decode()
                        .context(Message::LoadingMovies)?,
                );
                Ok(true)
            }
            Msg::WatchedLoaded(result) => {
                self.watched = result
                    .context(Message::LoadingWatched)?
                    .decode()
                    .context(Message::LoadingWatched)?
                    .watched;
                Ok(true)
            }
            Msg::AskMarkWatched => {
                self.confirm_mark_watch = true;
                self.confirm_remove_watch = None;
                Ok(true)
            }
            Msg::CancelMarkWatch => {
                self.confirm_mark_watch = false;
                Ok(true)
            }
            Msg::MarkWatched(mark_time) => {
                self.confirm_mark_watch = false;
                let movie = ctx.props().movie_id;
                self._mark_req = self
                    .channel
                    .request()
                    .body(api::MarkWatchedRequest {
                        kind: api::WatchedKind::Movie { movie },
                        mark_time,
                    })
                    .on_packet(ctx.link().callback(Msg::MarkWatchedDone))
                    .send();
                Ok(true)
            }
            Msg::MarkWatchedDone(result) => {
                result.context(Message::MarkingWatched)?;
                self.load_movie(ctx);
                self.load_watched(ctx);
                Ok(false)
            }
            Msg::RemoveWatched(id, kind) => {
                self._remove_watch_req = self
                    .channel
                    .request()
                    .body(api::RemoveWatchedRequest { id, kind })
                    .on_packet(ctx.link().callback(Msg::RemoveWatchedDone))
                    .send();
                Ok(false)
            }
            Msg::RemoveWatchedDone(result) => {
                result.context(Message::RemovingWatched)?;
                self.confirm_remove_watch = None;
                self.load_movie(ctx);
                self.load_watched(ctx);
                Ok(false)
            }
            Msg::ConfirmRemoveWatch(watched_id) => {
                self.confirm_mark_watch = false;
                self.confirm_remove_watch = Some(watched_id);
                Ok(true)
            }
            Msg::CancelRemoveWatch => {
                self.confirm_remove_watch = None;
                Ok(true)
            }
            Msg::ConfirmRemove => {
                self.confirm_remove = true;
                Ok(true)
            }
            Msg::CancelRemove => {
                self.confirm_remove = false;
                Ok(true)
            }
            Msg::RemoveMovie => {
                let id = ctx.props().movie_id;
                self._remove_req = self
                    .channel
                    .request()
                    .body(api::RemoveMovieRequest { id })
                    .on_packet(ctx.link().callback(Msg::RemoveDone))
                    .send();
                Ok(false)
            }
            Msg::RemoveDone(result) => {
                result.context(Message::RemovingMovie)?;
                ctx.props()
                    .on_navigate
                    .emit(Route::Movies(PagedQuery::default()));
                Ok(false)
            }
            Msg::SyncMovie => {
                let id = ctx.props().movie_id;

                self._sync_req = self
                    .channel
                    .request()
                    .body(api::SyncMovieRequest { id })
                    .on_packet(ctx.link().callback(Msg::SyncDone))
                    .send();

                Ok(true)
            }
            Msg::SyncDone(result) => {
                result.context(Message::SyncingSeries)?;
                Ok(false)
            }
            Msg::SetSyncSource(source) => {
                let id = ctx.props().movie_id;
                self._set_sync_source_req = self
                    .channel
                    .request()
                    .body(api::SetMovieSyncSourceRequest {
                        id,
                        source: source.clone(),
                    })
                    .on_packet(
                        ctx.link()
                            .callback(move |r| Msg::SetSyncSourceDone(source.clone(), r)),
                    )
                    .send();
                Ok(false)
            }
            Msg::SetSyncSourceDone(source, result) => {
                result.context(Message::SettingSyncSource)?;
                if let Some(ref mut movie) = self.movie {
                    movie.sync_source = Some(source);
                }
                Ok(true)
            }
            Msg::SetLanguage(language) => {
                let id = ctx.props().movie_id;

                self._set_language_req = self
                    .channel
                    .request()
                    .body(api::SetMovieLanguageRequest {
                        id,
                        language: language.clone(),
                    })
                    .on_packet(
                        ctx.link()
                            .callback(move |r| Msg::SetLanguageDone(language.clone(), r)),
                    )
                    .send();

                Ok(false)
            }
            Msg::SetLanguageDone(language, result) => {
                result.context(Message::SettingLanguage)?;
                if let Some(ref mut movie) = self.movie {
                    movie.language = language;
                }
                Ok(true)
            }
            Msg::SetTracked(tracked) => {
                self.actions_expanded = false;

                let id = ctx.props().movie_id;

                self._untrack_req = self
                    .channel
                    .request()
                    .body(api::UntrackMovieRequest { id, tracked })
                    .on_packet(
                        ctx.link()
                            .callback(move |r| Msg::SetTrackedDone(tracked, r)),
                    )
                    .send();

                Ok(false)
            }
            Msg::SetTrackedDone(tracked, result) => {
                result.context(Message::UntrackingMovie)?;
                if let Some(ref mut movie) = self.movie {
                    movie.tracked = tracked;
                }
                Ok(true)
            }
            Msg::OnWatchNext => {
                let movie = ctx.props().movie_id;
                self._pending_req = self
                    .channel
                    .request()
                    .body(api::AddPendingRequest {
                        kind: api::PendingKind::Movie { movie },
                    })
                    .on_packet(ctx.link().callback(Msg::AddPendingDone))
                    .send();
                Ok(false)
            }
            Msg::AddPendingDone(result) => {
                result.context(Message::SyncingSeries)?;
                if let Some(ref mut movie) = self.movie {
                    movie.pending = true;
                }
                Ok(true)
            }
            Msg::OnRemoveNext => {
                let movie = ctx.props().movie_id;
                self._pending_req = self
                    .channel
                    .request()
                    .body(api::RemovePendingRequest {
                        kind: api::PendingKind::Movie { movie },
                    })
                    .on_packet(ctx.link().callback(Msg::RemovePendingDone))
                    .send();
                Ok(false)
            }
            Msg::RemovePendingDone(result) => {
                result.context(Message::SyncingSeries)?;
                if let Some(ref mut movie) = self.movie {
                    movie.pending = false;
                }
                Ok(true)
            }
            Msg::SelectImage(id) => {
                self._select_image_req = self
                    .channel
                    .request()
                    .body(api::SelectImageRequest { id })
                    .on_packet(ctx.link().callback(Msg::SelectImageDone))
                    .send();
                Ok(false)
            }
            Msg::ClearSelectedImage(kind) => {
                self._clear_image_req = self
                    .channel
                    .request()
                    .body(api::ClearSelectedImageRequest {
                        owner: api::ImageOwner::Movie(ctx.props().movie_id),
                        kind,
                    })
                    .on_packet(ctx.link().callback(Msg::ClearSelectedImageDone))
                    .send();
                Ok(false)
            }
            Msg::SelectImageDone(result) => {
                result.context(Message::SyncingSeries)?;
                self.image_modal = None;
                self.load_movie(ctx);
                Ok(true)
            }
            Msg::ClearSelectedImageDone(result) => {
                result.context(Message::SyncingSeries)?;
                self.image_modal = None;
                self.load_movie(ctx);
                Ok(true)
            }
            Msg::OpenImageModal(kind) => {
                self.image_modal = Some(kind);
                Ok(true)
            }
            Msg::CloseImageModal => {
                self.image_modal = None;
                Ok(true)
            }
            Msg::SetTz(tz) => {
                self.tz = tz;
                Ok(true)
            }
            Msg::ToggleActionsExpanded => {
                self.actions_expanded = !self.actions_expanded;
                Ok(true)
            }
            Msg::ToggleDetailedActionsExpanded => {
                self.detailed_expand = !self.detailed_expand;
                Ok(true)
            }
        }
    }

    fn load_movie(&mut self, ctx: &Context<Self>) {
        self._movie_req = self
            .channel
            .request()
            .body(api::GetMovieRequest {
                id: ctx.props().movie_id,
            })
            .on_packet(ctx.link().callback(Msg::MovieLoaded))
            .send();
    }

    fn load_watched(&mut self, ctx: &Context<Self>) {
        let movie = ctx.props().movie_id;
        self._watched_req = self
            .channel
            .request()
            .body(api::ListWatchedRequest {
                kind: api::WatchedKind::Movie { movie },
            })
            .on_packet(ctx.link().callback(Msg::WatchedLoaded))
            .send();
    }

    fn view_header(&self, ctx: &Context<Self>, movie: &api::Movie) -> Html {
        let link = ctx.link();

        html! {
            <div class="row-fill page-title">
                if let Some(ref title) = movie.title {
                    <span class="fill">{title}</span>
                } else {
                    <span class="fill text-muted">{"Untitled Movie"}</span>
                }

                <div class="row end">
                    if !movie.remotes.is_empty() {
                        <div class="row hide-mobile">
                            <div class="input-group">
                                {for movie.remotes.iter().filter_map(|r| {
                                    let url = r.movie_url()?;
                                    let label = r.source().as_str().to_uppercase();

                                    Some(html! {
                                        <a class="btn" href={url} target="_blank" rel="noopener noreferrer" title={format!("Open on {label}")}>
                                            <span class="icon-inline"><span class="icon arrow-top-right-on-square" /></span>
                                            <span>{label}</span>
                                        </a>
                                    })
                                })}
                            </div>
                        </div>
                    }

                    <button class="btn hide-desktop" onclick={link.callback(|_| Msg::ToggleActionsExpanded)}>
                        <span class="icon-inline"><span class={classes!("icon", if self.actions_expanded { "ellipsis-horizontal" } else { "bars-3" })} /></span>
                    </button>
                </div>
            </div>
        }
    }

    fn view_body(&self, ctx: &Context<Self>, movie: &api::Movie) -> Html {
        let movie_id = ctx.props().movie_id;
        let link = ctx.link();

        let toggle_pending = move |mobile: bool| {
            let on_remove_next = link.callback(move |_| Msg::OnRemoveNext);
            let on_watch_next = link.callback(move |_| Msg::OnWatchNext);

            html! {
                if movie.pending {
                    <button class="btn" onclick={on_remove_next} title="Remove from watch next">
                        <span class="icon bookmark-slash" />
                        <span class={classes!(mobile.then_some("hide-mobile"), "hide-desktop")}>{"Remove watch next"}</span>
                    </button>
                } else {
                    <button class="btn" onclick={on_watch_next} title="Watch next">
                        <span class="icon bookmark" />
                        <span class={classes!(mobile.then_some("hide-mobile"), "hide-desktop")}>{"Watch next"}</span>
                    </button>
                }
            }
        };

        let on_ask_mark = link.callback(|_| Msg::AskMarkWatched);

        let actions = 'actions: {
            if self.confirm_mark_watch {
                break 'actions html! {
                    <div class="row actions">
                        <MarkWatchedPicker
                            on_confirm={link.callback(Msg::MarkWatched)}
                            on_cancel={link.callback(|_| Msg::CancelMarkWatch)}
                        />
                    </div>
                };
            }

            html! {
                <div class="actions row-fill">
                    <div class="column fill">
                        <div class="row-fill">
                            <div class="row">
                                if self.watched.len() > 0 {
                                    <span class="icon-inline" title="Watched"><span class="icon check-circle" /></span>
                                } else {
                                    <span class="icon-inline" title="Not watched"><span class="icon x-circle" /></span>
                                }

                                <span class="text-muted">
                                    {match &self.watched[..] {
                                        [] => "Not watched".to_string(),
                                        [w] => format!("Watched once at {}", w.timestamp.display(self.tz.clone())),
                                        [first, ..] => format!("Watched {} times, first at {}", self.watched.len(), first.timestamp.display(self.tz.clone())),
                                    }}
                                </span>
                            </div>

                            <div class="hide-desktop row end">
                                <div class="input-group">
                                    {toggle_pending(true)}

                                    <button class="btn" onclick={link.callback(move |_| Msg::ToggleDetailedActionsExpanded)}>
                                        <span class="icon-inline"><span class={classes!("icon", if self.detailed_expand { "ellipsis-horizontal" } else { "bars-3" })} /></span>
                                    </button>
                                </div>
                            </div>

                            <div class="hide-mobile row end">
                                <div class="input-group">
                                    <button class="btn-success" onclick={&on_ask_mark} title="Mark watched">
                                        <span class="icon check" />
                                        <span>{"Mark watched"}</span>
                                    </button>

                                    {toggle_pending(false)}
                                </div>
                            </div>
                        </div>

                        <div class={classes!("hide-desktop", "column", (!self.detailed_expand).then_some("hide-mobile"))}>
                            <button class="btn-success" onclick={&on_ask_mark} title="Mark watched">
                                <span class="icon check" />
                                <span>{"Mark watched"}</span>
                            </button>

                            {toggle_pending(false)}
                        </div>
                    </div>
                </div>
            }
        };

        html! {
            <>
            <div class={classes!("desktop-row-fill", "mobile-column", "actions", (!self.actions_expanded).then_some("hide-mobile"))}>
                <div class="desktop-row mobile-column fill start">
                    <LanguagePicker
                        current={movie.language.clone()}
                        placeholder="Default"
                        on_change={link.callback(Msg::SetLanguage)}
                    />

                    if movie.images.iter().any(|i| matches!(i.kind, api::ImageKind::Poster)) {
                        <button class="btn" onclick={link.callback(|_| Msg::OpenImageModal(api::ImageKind::Poster))} title="Change poster">
                            <span class="icon-inline"><span class="icon photo" /></span>
                            <span>{"Poster"}</span>
                        </button>
                    }

                    if movie.images.iter().any(|i| matches!(i.kind, api::ImageKind::Backdrop)) {
                        <button class="btn" onclick={link.callback(|_| Msg::OpenImageModal(api::ImageKind::Backdrop))} title="Change backdrop">
                            <span class="icon-inline"><span class="icon photo" /></span>
                            <span>{"Backdrop"}</span>
                        </button>
                    }

                    <div class="input-group">
                        <div class="input-label">{"Sync"}</div>

                        <RemoteSourceSelect
                            kind={RemoteSourceKind::Movie}
                            remotes={movie.remotes.clone()}
                            current_source={movie.effective_sync_source()}
                            on_change={link.callback(Msg::SetSyncSource)}
                        />

                        if let Some(ts) = movie.last_synced_at {
                            <div class="input-text fill" title="Last synced at">
                                <span>{ts.display(self.tz.clone())}</span>
                            </div>
                        } else {
                            <div class="input-text fill text-muted" title="Never synced">
                                <span>{"Never synced"}</span>
                            </div>
                        }

                        if !movie.remotes.is_empty() {
                            <button class="btn" onclick={link.callback(|_| Msg::SyncMovie)} title="Sync now">
                                <span class="icon-inline"><span class={classes!("icon", "arrow-path", self.syncing.then_some("spin"))} /></span>
                            </button>
                        }
                    </div>
                </div>

                <div class="desktop-row mobile-column end desktop-input-group">
                    <Tracked tracked={movie.tracked} ontoggle={link.callback(Msg::SetTracked)} />

                    if self.confirm_remove {
                        <ConfirmDanger
                            prompt="Remove movie"
                            label={movie.title.clone()}
                            on_confirm={link.callback(|_| Msg::RemoveMovie)}
                            on_cancel={link.callback(|_| Msg::CancelRemove)}
                        />
                    } else {
                        <button class="btn btn-danger" onclick={link.callback(|_| Msg::ConfirmRemove)} title="Remove movie">
                            <span class="icon-inline"><span class="icon trash" /></span>
                            <span class="hide-desktop">{"Remove"}</span>
                        </button>
                    }

                    if !movie.remotes.is_empty() {
                        <div class="hide-desktop row">
                            {for movie.remotes.iter().filter_map(|r| {
                                let url = r.movie_url()?;
                                let label = r.source().as_str().to_uppercase();

                                Some(html! {
                                    <a class="btn" href={url} target="_blank" rel="noopener noreferrer" title={format!("Open on {label}")}>
                                        <span class="icon-inline"><span class="icon arrow-top-right-on-square" /></span>
                                        <span>{label}</span>
                                    </a>
                                })
                            })}
                        </div>
                    }
                </div>
            </div>

            <div class="detail-layout">
                <img class="banner hide-desktop" src={movie.banner.as_ref().map(|p| p.proxy_url())} />

                <div class="detail-sidebar">
                    <img class="poster hide-mobile" src={movie.poster.as_ref().map(|p| p.proxy_url())} />
                </div>

                <div class="detail-content">
                    if let Some(date) = movie.release_date {
                        <div class="text-muted">{date.display(self.tz.clone())}</div>
                    }

                    if let Some(ref overview) = movie.overview {
                        <p class="overview">{overview}</p>
                    }

                    {actions}

                    if !self.watched.is_empty() {
                        <div class="column">
                            <h4>{"Watch history"}</h4>

                            <div class="column">
                                { for self.watched.iter().map(|w| {
                                    let wid = w.id;
                                    let kind = api::WatchedKind::Movie { movie: movie_id };

                                    if self.confirm_remove_watch == Some(wid) {
                                        html! {
                                            <ConfirmDanger
                                                prompt="Remove watch at"
                                                label={w.timestamp.display(self.tz.clone())}
                                                on_confirm={link.callback(move |_| Msg::RemoveWatched(wid, kind))}
                                                on_cancel={link.callback(|_| Msg::CancelRemoveWatch)}
                                            />
                                        }
                                    } else {
                                        html! {
                                            <div class="row-fill">
                                                <div class="row fill">
                                                    <span>{w.timestamp.display(self.tz.clone())}</span>
                                                </div>

                                                <button class="btn-danger end" onclick={link.callback(move |_| Msg::ConfirmRemoveWatch(wid))} title="Remove">
                                                    <span class="icon trash" />
                                                </button>
                                            </div>
                                        }
                                    }
                                }) }
                            </div>
                        </div>
                    }
                </div>
            </div>

            if let Some(kind) = self.image_modal {
                { self.view_image_modal(ctx, movie, kind) }
            }
            </>
        }
    }

    fn view_image_modal(
        &self,
        ctx: &Context<Self>,
        movie: &api::Movie,
        kind: api::ImageKind,
    ) -> Html {
        let selected_for_kind = match kind {
            api::ImageKind::Poster => movie.poster.as_ref(),
            api::ImageKind::Backdrop => movie.backdrop.as_ref(),
            _ => None,
        };
        let items: Vec<ImageItem> = movie
            .images
            .iter()
            .map(|img| ImageItem {
                id: img.id,
                kind: img.kind,
                source: img.source,
                image: img.image.clone(),
                selected: Some(&img.image) == selected_for_kind,
            })
            .collect();

        let link = ctx.link();

        html! {
            <ImageGallery
                {items}
                {kind}
                on_select={link.callback(Msg::SelectImage)}
                on_clear={if kind == api::ImageKind::Backdrop {
                    Some(link.callback(move |_| Msg::ClearSelectedImage(kind)))
                } else {
                    None
                }}
                on_close={link.callback(|_| Msg::CloseImageModal)}
            />
        }
    }
}
