use musli_web::web03::prelude::*;
use yew::prelude::*;

use crate::error::{CustomContext, Error, Message};
use crate::router::Route;
use crate::ui::{ConfirmDanger, ImageGallery, ImageItem, RemoteSourceKind, RemoteSourceSelect};

pub(super) struct MovieDetail {
    channel: ws::Channel,
    movie: Option<api::Movie>,
    watched: Vec<api::Watched>,
    confirm_remove: bool,
    confirm_remove_watch: bool,
    syncing: bool,
    image_modal: Option<api::ImageKind>,
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
    _set_sync_source_req: ws::Request,
}

pub(super) enum Msg {
    Channel(Result<ws::Channel, ws::Error>),
    AppBroadcast(Result<ws::Packet<api::AppBroadcast>, ws::Error>),
    MovieLoaded(Result<ws::Packet<api::GetMovie>, ws::Error>),
    WatchedLoaded(Result<ws::Packet<api::ListWatched>, ws::Error>),
    MarkWatched,
    MarkWatchedDone(Result<ws::Packet<api::MarkWatched>, ws::Error>),
    RemoveWatched(api::WatchedId, api::WatchedKind),
    RemoveWatchedDone(Result<ws::Packet<api::RemoveWatched>, ws::Error>),
    ConfirmRemoveWatch,
    CancelRemoveWatch,
    SelectImage(api::ImageId),
    SelectImageDone(Result<ws::Packet<api::SelectImage>, ws::Error>),
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
    SetTracked(bool),
    SetTrackedDone(bool, Result<ws::Packet<api::UntrackMovie>, ws::Error>),
    AddPending,
    AddPendingDone(Result<ws::Packet<api::AddPending>, ws::Error>),
    RemovePending,
    RemovePendingDone(Result<ws::Packet<api::RemovePending>, ws::Error>),
    Back,
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

        Self {
            channel: ws::Channel::default(),
            movie: None,
            watched: Vec::new(),
            confirm_remove: false,
            confirm_remove_watch: false,
            syncing: false,
            image_modal: None,
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
            _set_sync_source_req: ws::Request::default(),
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
            self.confirm_remove_watch = false;

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
                        ctx.props().on_navigate.emit(Route::Movies);
                        Ok(false)
                    }
                    api::AppEventKind::WatchedChanged { kind } => {
                        let relevant = matches!(
                            kind,
                            api::WatchedKind::Movie { movie } if *movie == ctx.props().movie_id
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
            Msg::MarkWatched => {
                let movie = ctx.props().movie_id;
                self._mark_req = self
                    .channel
                    .request()
                    .body(api::MarkWatchedRequest {
                        kind: api::WatchedKind::Movie { movie },
                        timestamp: None,
                    })
                    .on_packet(ctx.link().callback(Msg::MarkWatchedDone))
                    .send();
                Ok(false)
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
                self.confirm_remove_watch = false;
                self.load_movie(ctx);
                self.load_watched(ctx);
                Ok(false)
            }
            Msg::ConfirmRemoveWatch => {
                self.confirm_remove_watch = true;
                Ok(true)
            }
            Msg::CancelRemoveWatch => {
                self.confirm_remove_watch = false;
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
                ctx.props().on_navigate.emit(Route::Movies);
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
                Ok(false)
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
            Msg::SelectImageDone(result) => {
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
                ctx.props().on_navigate.emit(Route::Movies);
                Ok(false)
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

                <span class="fill">{&movie.title}</span>
                { for movie.remotes.iter().filter_map(|r| {
                    let url = r.movie_url()?;
                    let label = r.source().to_uppercase();
                    Some(html! {
                        <a class="btn" href={url} target="_blank" rel="noopener noreferrer" title={format!("Open on {label}")}>
                            <span class="icon-inline"><span class="icon arrow-top-right-on-square" /></span>
                            <span class="hide-mobile">{label}</span>
                        </a>
                    })
                }) }

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
        }
    }

    fn view_body(&self, ctx: &Context<Self>, movie: &api::Movie) -> Html {
        let last_watched_id = self.watched.first().map(|w| w.id);
        let movie_id = ctx.props().movie_id;
        let link = ctx.link();

        let tz = ctx
            .link()
            .context::<crate::SystemTz>(Callback::noop())
            .map(|(t, _)| t.get().clone())
            .unwrap_or(jiff::tz::TimeZone::UTC);

        let actions = 'actions: {
            if let Some(wid) = last_watched_id
                && self.confirm_remove_watch
            {
                break 'actions html! {
                    <div class="row actions">
                        <ConfirmDanger
                            prompt="Remove watch for"
                            label={movie.title.clone()}
                            on_confirm={link.callback(move |_| Msg::RemoveWatched(wid, api::WatchedKind::Movie { movie: movie_id }))}
                            on_cancel={link.callback(|_| Msg::CancelRemoveWatch)}
                        />
                    </div>
                };
            }

            html! {
                <div class="row actions">
                    if movie.watched {
                        <span class="icon-inline" title="Watched"><span class="icon check-circle" /></span>

                        <span class="text-muted fill">
                            if movie.watched_count == 1 {
                                {"Watched once"}
                            } else {
                                {format!("Watched {} times", movie.watched_count)}
                            }
                        </span>

                        <button class="btn" onclick={link.callback(|_| Msg::MarkWatched)} title="Watch again">
                            <span class="icon-inline"><span class="icon check" /></span>
                            {"Watch again"}
                        </button>

                        <button class="btn btn-danger" onclick={link.callback(|_| Msg::ConfirmRemoveWatch)} title="Remove last watch">
                            <span class="icon-inline"><span class="icon x-mark" /></span>
                            {"Remove watch"}
                        </button>

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
                        <button class="btn btn-success" onclick={link.callback(|e: MouseEvent| { e.prevent_default(); Msg::MarkWatched })} title="Mark watched">
                            <span class="icon-inline"><span class="icon check" /></span>
                            {"Mark watched"}
                        </button>
                    }
                </div>
            }
        };

        html! {
            <>
            <div class="row actions">
                <RemoteSourceSelect
                    kind={RemoteSourceKind::Movie}
                    remotes={movie.remotes.clone()}
                    current_source={movie.effective_sync_source()}
                    on_change={link.callback(Msg::SetSyncSource)}
                />

                if movie.images.iter().any(|i| matches!(i.kind, api::ImageKind::Banner | api::ImageKind::Fanart | api::ImageKind::Backdrop)) {
                    <button class="btn" onclick={link.callback(|_| Msg::OpenImageModal(api::ImageKind::Backdrop))} title="Change backdrop">
                        <span class="icon-inline"><span class="icon photo" /></span>
                        {"Background"}
                    </button>
                }

                if let Some(ts) = movie.last_synced_at {
                    <span class="text-muted">
                        {"Synced "}
                        {ts.display(&tz)}
                    </span>
                }
            </div>

            <div class="detail-layout">
                <div class="detail-sidebar section">
                    if let Some(poster) = movie.selected_image(api::ImageKind::Poster) {
                        <img class="poster hide-mobile" src={poster.proxy_url()} />
                    }
                </div>

                <div class="detail-content section">
                    if let Some(date) = movie.release_date {
                        <div class="section text-muted">{date.to_string()}</div>
                    }

                    if !movie.overview.is_empty() {
                        <p class="overview">{&movie.overview}</p>
                    }

                    {actions}

                    if !self.watched.is_empty() {
                        <div class="section">
                            <div class="section text-muted">{"Watch history"}</div>
                            {
                                for self.watched.iter().map(|w| html! {
                                    <div class="section text-muted">{w.timestamp.display(&tz)}</div>
                                })
                            }
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
                on_close={link.callback(|_| Msg::CloseImageModal)}
            />
        }
    }
}
