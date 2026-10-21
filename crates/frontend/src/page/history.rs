use std::collections::HashSet;

use api::TimeInfo;
use gloo::timers::callback::Timeout;
use musli_web::web03::prelude::*;
use musli_web::yew023::ContextExt;
use yew::prelude::*;

use crate::background::Background;
use crate::error::{CustomContext, Error, Message};
use crate::router::{MediaSelection, Route, ShowDetailQuery};
use crate::ui::{Button, Image, Link, PaginationButtons};

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
    filter: String,
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
    Filter(String),
    Toggle(api::WatchedId),
}

#[derive(Properties, PartialEq)]
pub(crate) struct Props {
    pub(crate) page: usize,
    pub(crate) filter: String,
    pub(crate) selection: MediaSelection,
    pub(crate) on_set_page: Callback<usize>,
    pub(crate) on_filter: Callback<String>,
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
            filter: ctx.props().filter.clone(),
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
        self.filter = props.filter.clone();

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
        let link = ctx.link();

        // Only the pages up to the next one are known.
        let page = ctx.props().page;
        let total_pages = page + 1 + usize::from(self.has_more);
        let on_page = ctx.props().on_set_page.clone();

        let on_filter = link.callback(|e: InputEvent| {
            let input: web_sys::HtmlInputElement = e.target_unchecked_into();
            Msg::Filter(input.value())
        });

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
                <div class="list-controls">
                    <div class="search-field">
                        <span class="icon magnifying-glass" aria-hidden="true" />
                        <input type="text" placeholder="Filter" aria-label="Filter history" value={self.filter.clone()} oninput={on_filter} />

                        if !self.filter.is_empty() {
                            <Button icon="x-mark" title="Clear filter" onclick={link.callback(|_| Msg::Filter(String::new()))} />
                        }
                    </div>

                    if self.loaded {
                        <PaginationButtons {page} {total_pages} on_page={on_page.clone()} />
                    }
                </div>

                if !self.loaded || self.list_req.is_pending() {
                    <div class="row center">
                        <span class="item-inline-more"><span class="icon arrow-path spin" aria-hidden="true" /></span>
                    </div>
                } else if self.groups.is_empty() {
                    <div class="row center">
                        <span class="item-inline-more">{"Nothing to show."}</span>
                    </div>
                } else {
                    <div class="agenda history">
                        { for days.iter().map(|(day, groups)| html! {
                            <section key={day.to_string()} class="agenda-day">
                                { view_day_heading(*day, today) }

                                <div class="agenda-items">
                                    { for groups.iter().map(|g| self.view_group(ctx, g, *day)) }
                                </div>
                            </section>
                        }) }
                    </div>

                    <div class="row desktop-align-end">
                        <PaginationButtons {page} {total_pages} {on_page} />
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
            Msg::Filter(filter) => {
                self.filter = filter.clone();
                ctx.props().on_filter.emit(filter);
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
                filter: self.filter.clone(),
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
                        season: code.map(|c| c.season).unwrap_or_default(),
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
                    <span class="schedule-time">
                        <span>{span}</span>
                        <span class="badge">{range}</span>
                    </span>

                    <Button
                        icon={if expanded { "chevron-up" } else { "chevron-down" }}
                        label={label}
                        title={toggle_title}
                        class="history-toggle"
                        expanded={Some(expanded)}
                        onclick={ctx.link().callback(move |_| Msg::Toggle(key))}
                    />
                </>
            }
        };

        html! {
            <div key={key.to_string()} class="schedule-item history-item">
                <Link to={route.clone()} class="schedule-poster" decorative=true>
                    <Image src={group.poster.clone()} />
                </Link>
                <Link to={route} class="schedule-title" title={format!("Open {title}")}><>{title.clone()}</></Link>
                <span class="schedule-times">{detail}</span>

                if group.watches.len() > 1 && self.expanded.contains(&key) {
                    <ul class="history-episodes" aria-label={format!("Episodes of {title}")}>
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
                        season: code.season,
                        episode: Some(code),
                        orphaned: false,
                    },
                );

                html! {
                    <Link {to} class="schedule-time history-watch" title={format!("Open {code}")}>
                        <span>{when}</span>
                        <span class="badge">{code.to_string()}</span>
                        if let Some(title) = &episode.title {
                            <span class="history-episode-title">{title.clone()}</span>
                        }
                    </Link>
                }
            }
            _ => html! {
                <span class="schedule-time history-watch">
                    <span>{when}</span>
                    <span class="badge">{"Movie"}</span>
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
