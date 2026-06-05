use musli_web::web03::prelude::*;
use yew::prelude::*;

use crate::error::{CustomContext, Error, Message};

pub(super) struct Dashboard {
    channel: ws::Channel,
    pending: Vec<api::Pending>,
    schedule: Vec<api::ScheduledDay>,
    _setup: crate::SetupChannel,
    _broadcast: ws::Listener,
    _pending_req: ws::Request,
    _schedule_req: ws::Request,
    _mark_req: ws::Request,
}

pub(super) enum Msg {
    Channel(Result<ws::Channel, ws::Error>),
    AppBroadcast(Result<ws::Packet<api::AppBroadcast>, ws::Error>),
    PendingLoaded(Result<ws::Packet<api::ListPending>, ws::Error>),
    ScheduleLoaded(Result<ws::Packet<api::ListSchedule>, ws::Error>),
    MarkWatched(api::WatchedKind),
    MarkWatchedDone(Result<ws::Packet<api::MarkWatched>, ws::Error>),
}

#[derive(Properties, PartialEq)]
pub(super) struct Props {
    pub(super) onerror: Callback<Error>,
}

impl Component for Dashboard {
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
            pending: Vec::new(),
            schedule: Vec::new(),
            _setup,
            _broadcast,
            _pending_req: ws::Request::default(),
            _schedule_req: ws::Request::default(),
            _mark_req: ws::Request::default(),
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
        html! {
            <div class="dashboard">
                { self.view_pending(ctx) }
                { self.view_schedule() }
            </div>
        }
    }
}

impl Dashboard {
    fn try_update(&mut self, ctx: &Context<Self>, msg: Msg) -> Result<bool, Error> {
        match msg {
            Msg::Channel(result) => {
                self.channel = result?;
                if self.channel.id() != ws::ChannelId::NONE {
                    self.load_pending(ctx);
                    self.load_schedule(ctx);
                } else {
                    self.pending.clear();
                    self.schedule.clear();
                }
                Ok(true)
            }
            Msg::AppBroadcast(packet) => {
                let event = packet?.decode_event()?;
                if event.channel == self.channel.id() {
                    return Ok(false);
                }
                match event.kind {
                    api::AppEventKind::PendingChanged
                    | api::AppEventKind::WatchedChanged { .. }
                    | api::AppEventKind::SeriesCreated { .. }
                    | api::AppEventKind::SeriesDeleted { .. }
                    | api::AppEventKind::MovieCreated { .. }
                    | api::AppEventKind::MovieDeleted { .. } => {
                        if self.channel.id() != ws::ChannelId::NONE {
                            self.load_pending(ctx);
                            self.load_schedule(ctx);
                        }
                        Ok(false)
                    }
                    _ => Ok(false),
                }
            }
            Msg::PendingLoaded(result) => {
                self.pending = result
                    .context(Message::LoadingPending)?
                    .decode()
                    .context(Message::LoadingPending)?
                    .pending;
                Ok(true)
            }
            Msg::ScheduleLoaded(result) => {
                self.schedule = result
                    .context(Message::LoadingSchedule)?
                    .decode()
                    .context(Message::LoadingSchedule)?
                    .days;
                Ok(true)
            }
            Msg::MarkWatched(kind) => {
                self._mark_req = self
                    .channel
                    .request()
                    .body(api::MarkWatchedRequest {
                        kind,
                        timestamp: None,
                    })
                    .on_packet(ctx.link().callback(Msg::MarkWatchedDone))
                    .send();
                Ok(false)
            }
            Msg::MarkWatchedDone(result) => {
                result.context(Message::MarkingWatched)?;
                if self.channel.id() != ws::ChannelId::NONE {
                    self.load_pending(ctx);
                }
                Ok(false)
            }
        }
    }

    fn load_pending(&mut self, ctx: &Context<Self>) {
        self._pending_req = self
            .channel
            .request()
            .body(api::ListPendingRequest)
            .on_packet(ctx.link().callback(Msg::PendingLoaded))
            .send();
    }

    fn load_schedule(&mut self, ctx: &Context<Self>) {
        self._schedule_req = self
            .channel
            .request()
            .body(api::ListScheduleRequest { days: 7 })
            .on_packet(ctx.link().callback(Msg::ScheduleLoaded))
            .send();
    }

    fn view_pending(&self, ctx: &Context<Self>) -> Html {
        html! {
            <div class="section">
                <div class="row"><h2>{"Up Next"}</h2></div>
                if self.pending.is_empty() {
                    <p class="text-muted">{"Nothing pending."}</p>
                } else {
                    <div class="pending-grid">
                        { for self.pending.iter().map(|p| self.view_pending_item(ctx, p)) }
                    </div>
                }
            </div>
        }
    }

    fn view_pending_item(&self, ctx: &Context<Self>, p: &api::Pending) -> Html {
        let kind = p.kind.clone();
        let on_mark = ctx.link().callback(move |_| {
            Msg::MarkWatched(match kind {
                api::PendingKind::Episode { series, episode } => {
                    api::WatchedKind::Episode { series, episode }
                }
                api::PendingKind::Movie { movie } => api::WatchedKind::Movie { movie },
            })
        });

        html! {
            <div class="pending-item">
                if let Some(ref poster) = p.poster {
                    <img class="pending-poster" src={poster.proxy_url()} alt="" />
                } else {
                    <div class="pending-poster pending-poster-placeholder" />
                }
                <div class="pending-info">
                    if let Some(ref title) = p.series_title {
                        <span class="pending-series">{title}</span>
                    }
                    <span class="pending-label">{&p.label}</span>
                    if let Some(date) = p.aired {
                        <span class="pending-date">{date.to_string()}</span>
                    }
                    <button class="btn-icon-success" onclick={on_mark} title="Mark watched">
                        <span class="icon check" />
                    </button>
                </div>
            </div>
        }
    }

    fn view_schedule(&self) -> Html {
        html! {
            <div class="section">
                <div class="row"><h2>{"Coming Up"}</h2></div>
                if self.schedule.is_empty() {
                    <p class="text-muted">{"Nothing scheduled in the next 7 days."}</p>
                } else {
                    { for self.schedule.iter().map(|d| self.view_schedule_day(d)) }
                }
            </div>
        }
    }

    fn view_schedule_day(&self, day: &api::ScheduledDay) -> Html {
        html! {
            <div class="schedule-day">
                <h3 class="schedule-date">{day.date.to_string()}</h3>
                { for day.entries.iter().map(|e| self.view_schedule_entry(e)) }
            </div>
        }
    }

    fn view_schedule_entry(&self, entry: &api::ScheduledEntry) -> Html {
        html! {
            <div class="schedule-entry">
                <span class="schedule-series">{&entry.series_title}</span>
                <div class="schedule-episodes">
                    { for entry.episodes.iter().map(|ep| html! {
                        <span class="schedule-ep">
                            { format!("S{:02}E{:02}", ep.season.to_i64(), ep.number) }
                            if let Some(ref name) = ep.name {
                                {" "}{name}
                            }
                        </span>
                    }) }
                </div>
            </div>
        }
    }
}
