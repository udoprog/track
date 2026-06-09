use api::TimeZone;
use musli_web::web03::prelude::*;
use yew::prelude::*;

use crate::error::{CustomContext, Error, Message};
use crate::router::{PagedQuery, Route};
use crate::ui::{
    ConfirmDanger, ImageGallery, ImageItem, LanguagePicker, MarkWatchedPicker, RemoteSourceKind,
    RemoteSourceSelect,
};

pub(super) struct MovieDetail {
    channel: ws::Channel,
    movie: Option<api::Movie>,
    watched: Vec<api::Watched>,
    confirm_remove: bool,
    confirm_mark_watch: bool,
    confirm_remove_watch: Option<api::WatchedId>,
    syncing: bool,
    image_modal: Option<api::ImageKind>,
    tz: TimeZone,
    _tz_handle: ContextHandle<TimeZone>,
    _setup: crate::SetupChannel,
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
    AddPending,
    AddPendingDone(Result<ws::Packet<api::AddPending>, ws::Error>),
    RemovePending,
    RemovePendingDone(Result<ws::Packet<api::RemovePending>, ws::Error>),
    Back,
    SetTz(TimeZone),
}

#[derive(Properties, PartialEq)]
pub(super) struct Props {
    pub(super) movie_id: api::MovieId,
    pub(super) onerror: Callback<Error>,
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

        let _setup = crate::SetupChannel::new(ws.clone(), ctx.link().callback(Msg::Channel));
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
                ctx.props().onerror.emit(e);
                false
            }
        }
    }

    fn view(&self, ctx: &Context<Self>) -> Html {
        let Some(ref movie) = self.movie else {
            return html! {
                <div class="page">
                    <div class="empty text-muted">{"Loading…"}</div>
                </div>
            };
        };

        let url = movie
            .images
            .iter()
            .find(|i| matches!(i.kind, api::ImageKind::Backdrop))
            .map(|image| image.image.proxy_url());

        let style = url
            .as_ref()
            .map(|url| format!("--background: url('{}')", url))
            .unwrap_or_default();

        html! {
            <div class="page-container" {style}>
                <div class="page">
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
            Msg::AddPending => {
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
            Msg::RemovePending => {
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
            Msg::Back => {
                ctx.props()
                    .on_navigate
                    .emit(Route::Movies(PagedQuery::default()));
                Ok(false)
            }
            Msg::SetTz(tz) => {
                self.tz = tz;
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
            <div class="row page-title">
                <button class="btn" onclick={link.callback(|_| Msg::Back)}>
                    <span class="icon-inline"><span class="icon arrow-left" /></span>
                    {"Movies"}
                </button>

                if let Some(ref title) = movie.title {
                    <span class="fill">{title}</span>
                } else {
                    <span class="fill text-muted">{"Untitled Movie"}</span>
                }

                { for movie.remotes.iter().filter_map(|r| {
                    let url = r.movie_url()?;
                    let label = r.source().as_str().to_uppercase();
                    Some(html! {
                        <a class="btn" href={url} target="_blank" rel="noopener noreferrer" title={format!("Open on {label}")}>
                            <span class="icon-inline"><span class="icon arrow-top-right-on-square" /></span>
                            <span class="hide-mobile">{label}</span>
                        </a>
                    })
                }) }
            </div>
        }
    }

    fn view_body(&self, ctx: &Context<Self>, movie: &api::Movie) -> Html {
        let last_watched_id = self.watched.first().map(|w| w.id);
        let movie_id = ctx.props().movie_id;
        let link = ctx.link();

        let watched_count = self.watched.len();

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
                <div class="row actions">
                    if watched_count > 0 {
                        <span class="icon-inline" title="Watched"><span class="icon check-circle" /></span>

                        <span class="text-muted fill">
                            if watched_count == 1 {
                                {"Watched once"}
                            } else {
                                {format!("Watched {} times", watched_count)}
                            }
                        </span>

                        <button class="btn" onclick={link.callback(|_| Msg::AskMarkWatched)} title="Watch again">
                            <span class="icon-inline"><span class="icon check" /></span>
                            {"Watch again"}
                        </button>

                        if let Some(last_watched_id) = last_watched_id {
                            <button class="btn btn-danger" onclick={link.callback(move |_| Msg::ConfirmRemoveWatch(last_watched_id))} title="Remove last watch">
                                <span class="icon-inline"><span class="icon x-mark" /></span>
                                {"Remove watch"}
                            </button>
                        }

                        if movie.pending {
                            <button class="btn" onclick={link.callback(|_| Msg::RemovePending)} title="Remove from pending">
                                <span class="icon-inline"><span class="icon bookmark-slash" /></span>
                                {"Remove pending"}
                            </button>
                        } else {
                            <button class="btn" onclick={link.callback(|_| Msg::AddPending)} title="Mark as pending">
                                <span class="icon-inline"><span class="icon bookmark" /></span>
                                {"Mark pending"}
                            </button>
                        }
                    } else {
                        <button class="btn btn-success" onclick={link.callback(|_| Msg::AskMarkWatched)} title="Mark watched">
                            <span class="icon-inline"><span class="icon check" /></span>
                            {"Mark watched"}
                        </button>
                    }
                </div>
            }
        };

        html! {
            <>
            <div class="row-fill actions">
                <div class="row fill start">
                    <RemoteSourceSelect
                        kind={RemoteSourceKind::Movie}
                        remotes={movie.remotes.clone()}
                        current_source={movie.effective_sync_source()}
                        on_change={link.callback(Msg::SetSyncSource)}
                    />

                    <LanguagePicker
                        current={movie.language.clone()}
                        placeholder="Default"
                        on_change={link.callback(Msg::SetLanguage)}
                    />

                    if movie.images.iter().any(|i| matches!(i.kind, api::ImageKind::Banner | api::ImageKind::Fanart | api::ImageKind::Backdrop)) {
                        <button class="btn" onclick={link.callback(|_| Msg::OpenImageModal(api::ImageKind::Backdrop))} title="Change backdrop">
                            <span class="icon-inline"><span class="icon photo" /></span>
                            <span class="hide-mobile">{"Background"}</span>
                        </button>
                    }

                    if let Some(ts) = movie.last_synced_at {
                        <span class="text-muted hide-mobile" title="Last synced at">
                            {ts.display(self.tz.clone())}
                        </span>
                    }
                </div>

                <div class="row end">
                    if movie.tracked {
                        <button class="btn" onclick={link.callback(|_| Msg::SetTracked(false))} title="Untrack movie">
                            <span class="icon-inline"><span class="icon eye-slash" /></span>
                            <span class="hide-mobile">{"Untrack"}</span>
                        </button>
                    } else {
                        <button class="btn" onclick={link.callback(|_| Msg::SetTracked(true))} title="Track movie">
                            <span class="icon-inline"><span class="icon eye" /></span>
                            <span class="hide-mobile">{"Track"}</span>
                        </button>
                    }

                    if !movie.remotes.is_empty() {
                        <button class="btn" onclick={link.callback(|_| Msg::SyncMovie)} title="Sync from remote">
                            <span class="icon-inline"><span class={classes!("icon", "arrow-path", self.syncing.then_some("spin"))} /></span>
                            <span class="hide-mobile">{"Sync"}</span>
                        </button>
                    }

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
                            <span class="hide-mobile">{"Remove"}</span>
                        </button>
                    }
                </div>
            </div>

            <div class="detail-layout">
                <div class="detail-sidebar section">
                    if let Some(poster) = movie.selected_image(api::ImageKind::Poster) {
                        <img class="poster hide-mobile" src={poster.proxy_url()} />
                    }
                </div>

                <div class="detail-content section">
                    if let Some(date) = movie.release_date {
                        <div class="section text-muted">{date.display(self.tz.clone())}</div>
                    }

                    if let Some(ref overview) = movie.overview {
                        <p class="overview">{overview}</p>
                    }

                    {actions}

                    if !self.watched.is_empty() {
                        <div class="section">
                            <h4>{"Watch history"}</h4>

                            <div class="table">
                                { for self.watched.iter().map(|w| {
                                    let wid = w.id;
                                    let kind = api::WatchedKind::Movie { movie: movie_id };

                                    if self.confirm_remove_watch == Some(wid) {
                                        html! {
                                            <div class="table-entry">
                                                <ConfirmDanger
                                                    prompt="Remove watch"
                                                    label={w.timestamp.display(self.tz.clone())}
                                                    on_confirm={link.callback(move |_| Msg::RemoveWatched(wid, kind))}
                                                    on_cancel={link.callback(|_| Msg::CancelRemoveWatch)}
                                                />
                                            </div>
                                        }
                                    } else {
                                        html! {
                                            <div class="table-entry row">
                                                <div class="row fill">
                                                    <span>{w.timestamp.display(self.tz.clone())}</span>
                                                </div>

                                                <button class="btn-icon end" onclick={link.callback(move |_| Msg::ConfirmRemoveWatch(wid))} title="Remove">
                                                    <span class="icon x-mark" />
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
        let items: Vec<ImageItem> = movie
            .images
            .iter()
            .map(|img| ImageItem {
                id: img.id,
                kind: img.kind,
                source: img.source,
                image: img.image.clone(),
                selected: img.selected,
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
