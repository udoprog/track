use std::collections::HashSet;

use api::TimeInfo;
use gloo::timers::callback::Timeout;
use musli_web::web03::prelude::*;
use musli_web::yew023::ContextExt;
use yew::prelude::*;

use crate::background::Background;
use crate::class;
use crate::error::{CustomContext, Error, Message};
use crate::router::{MediaSelection, Route, ShowDetailQuery};
use crate::ui::{Button, Image, Link, Pager};

use super::schedule_item::view_day_heading;

/// Watch groups per page.
const PAGE_SIZE: usize = 30;

/// How long the filter waits for typing to pause before asking for history.
const FILTER_DELAY_MS: u32 = 250;

/// The dashboard's History tab: every watch, newest first, under the day it
/// was watched, with episodes of a show watched back to back folded into one
/// entry.
pub(crate) struct History {
    channel: ws::Channel,
    /// The groups on the current page.
    groups: Vec<api::HistoryGroup>,
    /// Whether more groups match past the current page.
    has_more: bool,
    /// Unfolded groups, by their newest watch.
    expanded: HashSet<api::WatchedId>,
    time: TimeInfo,
    _time_handle: ContextHandle<TimeInfo>,
    background: Background,
    _broadcast: ws::Listener,
    /// The request for the current page; replacing it drops the answer to an
    /// earlier one.
    list_req: ws::Request,
    /// Holds back loading while the filter is being typed.
    filter_delay: Option<Timeout>,
    /// Whether the history has arrived, so an empty page is not shown as
    /// nothing watched while it loads.
    loaded: bool,
}

pub(crate) enum Msg {
    Channel(ws::ChannelEvent),
    AppBroadcast(Result<ws::Packet<api::AppBroadcast>, ws::Error>),
    SetTime(TimeInfo),
    Load,
    Loaded(Result<ws::Packet<api::ListHistory>, ws::Error>),
    Toggle(api::WatchedId),
}

#[derive(Properties, PartialEq)]
pub(crate) struct Props {
    pub(crate) page: usize,
    pub(crate) filter: String,
    pub(crate) selection: MediaSelection,
    pub(crate) on_set_page: Callback<usize>,
}

impl Component for History {
    type Message = Msg;
    type Properties = Props;

    fn create(ctx: &Context<Self>) -> Self {
        let ws = ctx.handle();

        let channel = ws
            .channel()
            .on_change(ctx.link().callback(Msg::Channel))
            .open();
        let _broadcast = ws.on_broadcast(ctx.link().callback(Msg::AppBroadcast));

        let (background, _) = ctx
            .link()
            .context::<Background>(Callback::noop())
            .expect("Expected background handle in context");

        let (time, _time_handle) = ctx
            .link()
            .context::<TimeInfo>(ctx.link().callback(Msg::SetTime))
            .expect("Expected TimeInfo in context");

        Self {
            channel,
            groups: Vec::new(),
            has_more: false,
            expanded: HashSet::new(),
            time,
            _time_handle,
            background,
            _broadcast,
            list_req: ws::Request::default(),
            filter_delay: None,
            loaded: false,
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

    fn changed(&mut self, ctx: &Context<Self>, old_props: &Self::Properties) -> bool {
        let props = ctx.props();

        if old_props.page != props.page || old_props.selection != props.selection {
            self.load(ctx);
        } else if old_props.filter != props.filter {
            let link = ctx.link().clone();
            self.filter_delay = Some(Timeout::new(FILTER_DELAY_MS, move || {
                link.send_message(Msg::Load)
            }));
        }

        true
    }

    fn view(&self, ctx: &Context<Self>) -> Html {
        // Only the pages up to the next one are known.
        let page = ctx.props().page;
        let total_pages = page + 1 + usize::from(self.has_more);
        let on_page = ctx.props().on_set_page.clone();

        let today = self.time.date();

        // Consecutive groups under the day of their newest watch.
        let mut days = Vec::<(api::Date, Vec<&api::HistoryGroup>)>::new();

        for group in &self.groups {
            let Some(newest) = group.watches.first() else {
                continue;
            };

            let day = newest.timestamp.date(self.time.clone());

            match days.last_mut() {
                Some((d, groups)) if *d == day => groups.push(group),
                _ => days.push((day, vec![group])),
            }
        }

        html! {
            <>
                if self.loaded {
                    <div class={class::LIST_CONTROLS}>
                        <Pager {page} pages={total_pages} on_page={on_page.clone()} />
                    </div>
                }

                if !self.loaded || self.list_req.is_pending() {
                    <div class={classes!(sockel::class::CLUSTER, sockel::class::NOWRAP, sockel::class::LOOSE, sockel::class::CENTER)}>
                        <span class={class::ITEM_INLINE_MORE}><span class={classes!(class::ICON, class::ICON_ARROW_PATH, class::SPIN)} aria-hidden="true" /></span>
                    </div>
                } else if self.groups.is_empty() {
                    <div class={classes!(sockel::class::CLUSTER, sockel::class::NOWRAP, sockel::class::LOOSE, sockel::class::CENTER)}>
                        <span class={class::ITEM_INLINE_MORE}>{"Nothing to show."}</span>
                    </div>
                } else {
                    <div class={classes!("agenda", "history", sockel::class::STACK, sockel::class::LOOSE)}>
                        { for days.iter().map(|(day, groups)| html! {
                            <section key={day.to_string()} class={classes!("agenda-day", sockel::class::STACK, sockel::class::TIGHT)}>
                                { view_day_heading(*day, today) }

                                <div class={class::AGENDA_ITEMS}>
                                    { for groups.iter().map(|g| self.view_group(ctx, g, *day)) }
                                </div>
                            </section>
                        }) }
                    </div>

                    <div class={classes!(class::DESKTOP_ALIGN_END, sockel::class::CLUSTER, sockel::class::NOWRAP, sockel::class::LOOSE)}>
                        <Pager {page} pages={total_pages} {on_page} />
                    </div>
                }
            </>
        }
    }
}

impl History {
    fn try_update(&mut self, ctx: &Context<Self>, msg: Msg) -> Result<bool, Error> {
        match msg {
            Msg::Channel(event) => {
                if let ws::ChannelEvent::Failed(error) = event {
                    return Err(error.into());
                }

                if self.channel.is_open() {
                    self.load(ctx);
                } else {
                    self.groups.clear();
                    self.has_more = false;
                    self.loaded = false;
                }

                Ok(true)
            }
            Msg::AppBroadcast(packet) => {
                let event = packet?.decode_event()?;

                let relevant = matches!(
                    event.kind,
                    api::AppEventKind::WatchedChanged { .. }
                        | api::AppEventKind::ShowChanged { .. }
                        | api::AppEventKind::ShowDeleted { .. }
                        | api::AppEventKind::MovieChanged { .. }
                        | api::AppEventKind::MovieDeleted { .. }
                );

                if relevant && self.channel.is_open() {
                    self.load(ctx);
                }

                Ok(false)
            }
            Msg::SetTime(time) => {
                self.time = time;
                Ok(true)
            }
            Msg::Load => {
                self.load(ctx);
                Ok(true)
            }
            Msg::Loaded(result) => {
                let response = result
                    .context(Message::LoadingHistory)?
                    .decode()
                    .context(Message::LoadingHistory)?;
                self.groups = response.groups;
                self.has_more = response.has_more;
                self.loaded = true;
                Ok(true)
            }
            Msg::Toggle(id) => {
                if !self.expanded.remove(&id) {
                    self.expanded.insert(id);
                }

                Ok(true)
            }
        }
    }

    fn load(&mut self, ctx: &Context<Self>) {
        self.filter_delay = None;

        if !self.channel.is_open() {
            return;
        }

        let props = ctx.props();

        self.list_req = self
            .channel
            .request()
            .body(api::ListHistoryRequest {
                filter: props.filter.clone(),
                shows: props.selection.shows,
                movies: props.selection.movies,
                offset: props.page.saturating_mul(PAGE_SIZE),
                limit: PAGE_SIZE,
            })
            .on_packet(ctx.link().callback(Msg::Loaded))
            .send();
    }

    /// One group under the heading of `day`: its poster and title over when it
    /// was watched, a run of episodes folded behind a toggle listing them.
    fn view_group(&self, ctx: &Context<Self>, group: &api::HistoryGroup, day: api::Date) -> Html {
        let (Some(newest), Some(oldest)) = (group.watches.first(), group.watches.last()) else {
            return Html::default();
        };

        let title = group.title.clone().unwrap_or_else(|| "Unknown".to_owned());

        let route = match group.media {
            api::HistoryMedia::Show(show) => {
                let code = newest.episode.as_ref().map(|e| e.code);

                Route::ShowDetail(
                    show,
                    ShowDetailQuery {
                        season: code.map(|c| c.season),
                        episode: code,
                        orphaned: false,
                    },
                )
            }
            api::HistoryMedia::Movie(movie) => Route::MovieDetail(movie),
        };

        let key = newest.id;

        let detail = if let [watch] = group.watches.as_slice() {
            self.view_watch(group.media, watch, day)
        } else {
            let expanded = self.expanded.contains(&key);
            let count = group.watches.len();

            let mut codes = group
                .watches
                .iter()
                .filter_map(|w| w.episode.as_ref().map(|e| e.code));
            let first = codes.clone().min();
            let last = codes.by_ref().max();

            let range = match (first, last) {
                (Some(first), Some(last)) if first != last => format!("{first}–{last}"),
                (Some(first), _) => first.to_string(),
                _ => String::new(),
            };

            let span = format!(
                "{}–{}",
                self.when(oldest.timestamp, day),
                newest.timestamp.time_of_day(self.time.clone())
            );

            let label = format!("{count} episodes");
            let toggle_title = if expanded {
                "Hide episodes"
            } else {
                "Show episodes"
            };

            html! {
                <>
                    <span class={classes!(class::SCHEDULE_TIME, sockel::class::NUMERIC)}>
                        <span>{span}</span>
                        <span class={class::BADGE}>{range}</span>
                    </span>

                    <Button
                        icon={if expanded { class::ICON_CHEVRON_UP } else { class::ICON_CHEVRON_DOWN }}
                        label={label}
                        title={toggle_title}
                        class={class::HISTORY_TOGGLE}
                        expanded={Some(expanded)}
                        onclick={ctx.link().callback(move |_| Msg::Toggle(key))}
                    />
                </>
            }
        };

        html! {
            <div key={key.to_string()} class={classes!(class::SCHEDULE_ITEM, class::HISTORY_ITEM)}>
                <Link to={route.clone()} class={class::SCHEDULE_POSTER} decorative=true>
                    <Image src={group.poster.clone()} />
                </Link>
                <Link to={route} class={class::SCHEDULE_TITLE} title={format!("Open {title}")}><>{title.clone()}</></Link>
                <span class={class::SCHEDULE_TIMES}>{detail}</span>

                if group.watches.len() > 1 && self.expanded.contains(&key) {
                    <ul class={class::HISTORY_EPISODES} aria-label={format!("Episodes of {title}")}>
                        { for group.watches.iter().map(|w| html! {
                            <li key={w.id.to_string()}>{ self.view_watch(group.media, w, day) }</li>
                        }) }
                    </ul>
                }
            </div>
        }
    }

    /// One watch: when, and of what episode, which it links to.
    fn view_watch(
        &self,
        media: api::HistoryMedia,
        watch: &api::HistoryWatch,
        day: api::Date,
    ) -> Html {
        let when = self.when(watch.timestamp, day);

        match (media, &watch.episode) {
            (api::HistoryMedia::Show(show), Some(episode)) => {
                let code = episode.code;
                let to = Route::ShowDetail(
                    show,
                    ShowDetailQuery {
                        season: Some(code.season),
                        episode: Some(code),
                        orphaned: false,
                    },
                );

                html! {
                    <Link {to} class={classes!(class::SCHEDULE_TIME, class::HISTORY_WATCH, sockel::class::NUMERIC)} title={format!("Open {code}")}>
                        <span>{when}</span>
                        <span class={class::BADGE}>{code.to_string()}</span>
                        if let Some(title) = &episode.title {
                            <span class={classes!("history-episode-title", sockel::class::TRUNCATE)}>{title.clone()}</span>
                        }
                    </Link>
                }
            }
            _ => html! {
                <span class={classes!(class::SCHEDULE_TIME, class::HISTORY_WATCH, sockel::class::NUMERIC)}>
                    <span>{when}</span>
                    <span class={class::BADGE}>{"Movie"}</span>
                </span>
            },
        }
    }

    /// The time of day a watch under the heading of `day` happened, with its
    /// date when that is another day.
    fn when(&self, timestamp: api::Timestamp, day: api::Date) -> String {
        let time = timestamp.time_of_day(self.time.clone());
        let date = timestamp.date(self.time.clone());

        if date == day {
            time.to_string()
        } else {
            format!("{} {} {time}", date.day(), date.month_name())
        }
    }
}
