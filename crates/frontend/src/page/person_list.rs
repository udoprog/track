use gloo::timers::callback::Timeout;
use musli_web::web03::prelude::*;
use musli_web::yew023::ContextExt;
use yew::prelude::*;

use crate::background::Background;
use crate::error::{CustomContext, Error, Message};
use crate::router::{PersonQuery, PersonSort, Route, Router};
use crate::ui::{Button, Image, Link, PaginationButtons, SortMenu};

/// The fields the list can be sorted by, as `(value, label)`.
const SORTS: &[(&str, &str)] = &[("name", "Name"), ("credits", "Credits")];

const PAGE_SIZE: usize = 24;

/// How long the filter waits for typing to pause before asking for people.
const FILTER_DELAY_MS: u32 = 250;

pub(crate) struct PersonList {
    channel: ws::Channel,
    /// The people on the current page.
    persons: Vec<api::PersonItem>,
    /// Whether more people match past the current page.
    has_more: bool,
    filter: String,
    page: usize,
    sort: PersonSort,
    desc: bool,
    background: Background,
    router: Router,
    _broadcast: ws::Listener,
    /// The request for the current page; replacing it drops the answer to an
    /// earlier one.
    list_req: ws::Request,
    /// Holds back loading while the filter is being typed.
    filter_delay: Option<Timeout>,
    /// Whether the list has arrived, so an empty list is not shown as zero
    /// people while it loads.
    loaded: bool,
}

pub(crate) enum Msg {
    Channel(ws::ChannelEvent),
    AppBroadcast(Result<ws::Packet<api::AppBroadcast>, ws::Error>),
    Load,
    Loaded(Result<ws::Packet<api::ListPersons>, ws::Error>),
    Filter(String),
    SetSort(PersonSort),
    ToggleDir,
    SetPage(usize),
}

#[derive(Properties, PartialEq)]
pub(crate) struct Props {
    pub(crate) page: usize,
    pub(crate) filter: String,
    pub(crate) sort: PersonSort,
    pub(crate) desc: bool,
}

impl Component for PersonList {
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

        let (router, _) = ctx
            .link()
            .context::<Router>(Callback::noop())
            .expect("Expected router in context");

        Self {
            channel,
            persons: Vec::new(),
            has_more: false,
            filter: ctx.props().filter.clone(),
            page: ctx.props().page,
            sort: ctx.props().sort,
            desc: ctx.props().desc,
            background,
            router,
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

        self.page = props.page;
        self.filter = props.filter.clone();
        self.sort = props.sort;
        self.desc = props.desc;

        if old_props.page != props.page
            || old_props.sort != props.sort
            || old_props.desc != props.desc
        {
            self.load(ctx);
        } else if old_props.filter != props.filter {
            let link = ctx.link().clone();
            self.filter_delay = Some(Timeout::new(FILTER_DELAY_MS, move || {
                link.send_message(Msg::Load)
            }));
        }

        true
    }

    fn rendered(&mut self, _ctx: &Context<Self>, first_render: bool) {
        if first_render {
            self.background.title(Some("People".to_string()));
        }
    }

    fn destroy(&mut self, _ctx: &Context<Self>) {
        self.background.title(None);
    }

    fn view(&self, ctx: &Context<Self>) -> Html {
        let link = ctx.link();

        // Only the pages up to the next one are known.
        let page = self.page;
        let total_pages = page + 1 + usize::from(self.has_more);

        let on_filter = link.callback(|e: InputEvent| {
            let input: web_sys::HtmlInputElement = e.target_unchecked_into();
            Msg::Filter(input.value())
        });

        let on_sort = link.callback(|value: &'static str| {
            Msg::SetSort(match value {
                "credits" => PersonSort::Credits,
                _ => PersonSort::Name,
            })
        });

        let dir_icon = if self.desc {
            "bars-arrow-down"
        } else {
            "bars-arrow-up"
        };
        let dir_title = if self.desc { "Descending" } else { "Ascending" };

        html! {
            <>
                // The app bar already says which page this is.
                <h1 class="visually-hidden">{"People"}</h1>

                <div class="list-controls">
                    <div class="search-field">
                        <span class="icon magnifying-glass" aria-hidden="true" />
                        <input type="text" placeholder="Filter" aria-label="Filter people" value={self.filter.clone()} oninput={on_filter} />

                        if !self.filter.is_empty() {
                            <Button icon="x-mark" title="Clear filter" onclick={link.callback(|_| Msg::Filter(String::new()))} />
                        }
                    </div>

                    <div class="chips">
                        <SortMenu
                            options={SORTS}
                            current={match self.sort { PersonSort::Name => "name", PersonSort::Credits => "credits" }}
                            on_change={on_sort}
                        />

                        <Button icon={dir_icon} title={dir_title} class="chip" onclick={link.callback(|_| Msg::ToggleDir)} />
                    </div>

                    if self.loaded {
                        <PaginationButtons {page} {total_pages} on_page={link.callback(Msg::SetPage)} />
                    }
                </div>

                if !self.loaded || self.list_req.is_pending() {
                    <div class="row center">
                        <span class="item-inline-more"><span class="icon arrow-path spin" aria-hidden="true" /></span>
                    </div>
                } else if self.persons.is_empty() {
                    <div class="row center">
                        <span class="item-inline-more">{"Nothing to show."}</span>
                    </div>
                } else {
                    <div class="person-grid">
                        { for self.persons.iter().map(|p| self.view_card(p)) }
                    </div>

                    <div class="row desktop-align-end">
                        <PaginationButtons {page} {total_pages} on_page={link.callback(Msg::SetPage)} />
                    </div>
                }
            </>
        }
    }
}

impl PersonList {
    fn try_update(&mut self, ctx: &Context<Self>, msg: Msg) -> Result<bool, Error> {
        match msg {
            Msg::Channel(event) => {
                if let ws::ChannelEvent::Failed(error) = event {
                    return Err(error.into());
                }

                if self.channel.is_open() {
                    self.load(ctx);
                } else {
                    self.persons.clear();
                    self.has_more = false;
                    self.loaded = false;
                }

                Ok(true)
            }
            Msg::AppBroadcast(packet) => {
                let packet = packet?;
                let event = packet.decode_event()?;

                if packet.is_from(self.channel.id()) {
                    return Ok(false);
                }

                // People are seeded/pruned by credit sync and updated by their own
                // sync, so react to both.
                let relevant = matches!(
                    event.kind,
                    api::AppEventKind::PersonChanged { .. }
                        | api::AppEventKind::CreditsChanged { .. }
                );

                if relevant && self.channel.is_open() {
                    self.load(ctx);
                }

                Ok(false)
            }
            Msg::Load => {
                self.load(ctx);
                Ok(true)
            }
            Msg::Loaded(result) => {
                let response = result
                    .context(Message::LoadingPersons)?
                    .decode()
                    .context(Message::LoadingPersons)?;
                self.persons = response.persons;
                self.has_more = response.has_more;
                self.loaded = true;
                Ok(true)
            }
            Msg::Filter(filter) => {
                self.filter = filter;
                self.page = 0;
                self.emit_navigate();
                Ok(true)
            }
            Msg::SetSort(sort) => {
                self.sort = sort;
                self.desc = sort.default_desc();
                self.emit_navigate();
                Ok(true)
            }
            Msg::ToggleDir => {
                self.desc = !self.desc;
                self.emit_navigate();
                Ok(true)
            }
            Msg::SetPage(page) => {
                self.page = page;
                self.emit_navigate();
                Ok(true)
            }
        }
    }

    fn emit_navigate(&self) {
        self.router.push(Route::People(PersonQuery {
            page: self.page,
            filter: self.filter.clone(),
            sort: self.sort,
            desc: self.desc,
        }));
    }

    fn load(&mut self, ctx: &Context<Self>) {
        self.filter_delay = None;

        if !self.channel.is_open() {
            return;
        }

        self.list_req = self
            .channel
            .request()
            .body(api::ListPersonsRequest {
                filter: self.filter.clone(),
                sort: match self.sort {
                    PersonSort::Name => api::PersonSort::Name,
                    PersonSort::Credits => api::PersonSort::Credits,
                },
                desc: self.desc,
                offset: self.page.saturating_mul(PAGE_SIZE),
                limit: PAGE_SIZE,
            })
            .on_packet(ctx.link().callback(Msg::Loaded))
            .send();
    }

    fn view_card(&self, p: &api::PersonItem) -> Html {
        let name = display_name(p).unwrap_or("Unknown").to_owned();
        let route = Route::PersonDetail(p.id);

        html! {
            <Link key={p.id.to_string()} to={route} class="person-card lift">
                <Image class="person-photo artwork" placeholder={true} placeholder_icon="user" src={p.profile.clone()} alt={name.clone()} />

                <div class="person-info">
                    <div class="person-name">{ name }</div>

                    if let Some(department) = p.department.department() {
                        <div class="person-department text-muted">{ department.to_owned() }</div>
                    }

                    <div class="person-credits text-muted">
                        { format!("{} credit{}", p.credit_count, if p.credit_count == 1 { "" } else { "s" }) }
                    </div>
                </div>
            </Link>
        }
    }
}

/// A person's name in the display language, or in any language they have one
/// in: a list entry is better named in another language than not at all.
fn display_name(p: &api::PersonItem) -> Option<&str> {
    p.name.title_or_any()
}
