use musli_web::web03::prelude::*;
use yew::prelude::*;

use crate::SetupChannel;
use crate::background::Background;
use crate::error::{CustomContext, Error, Message};
use crate::router::{PersonQuery, PersonSort, Route, Router};
use crate::ui::{Button, Image, Link, PaginationButtons};

const PAGE_SIZE: usize = 24;

pub(crate) struct PersonList {
    channel: ws::Channel,
    persons: Vec<api::PersonItem>,
    /// Indices into `persons` for the current filter in the active sort order.
    /// Maintained on every input change so `view` neither reallocates nor
    /// re-sorts per render.
    order: Vec<usize>,
    filter: String,
    page: usize,
    sort: PersonSort,
    desc: bool,
    background: Background,
    router: Router,
    _setup: SetupChannel,
    _broadcast: ws::Listener,
    list_req: ws::Request,
    /// Whether the list has arrived, so an empty list is not shown as zero
    /// people while it loads.
    loaded: bool,
}

pub(crate) enum Msg {
    Channel(Result<ws::Channel, ws::Error>),
    AppBroadcast(Result<ws::Packet<api::AppBroadcast>, ws::Error>),
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
        let (ws, _) = ctx
            .link()
            .context::<ws::Handle>(Callback::noop())
            .expect("Expected ws::Handle in context");

        let _setup = SetupChannel::new(ws.clone(), ctx.link().callback(Msg::Channel));
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
            channel: ws::Channel::default(),
            persons: Vec::new(),
            order: Vec::new(),
            filter: ctx.props().filter.clone(),
            page: ctx.props().page,
            sort: ctx.props().sort,
            desc: ctx.props().desc,
            background,
            router,
            _setup,
            _broadcast,
            list_req: ws::Request::default(),
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

        // Only the order-affecting inputs warrant a rebuild; a bare page change
        // (e.g. from pagination) leaves the order untouched.
        if old_props.filter != props.filter
            || old_props.sort != props.sort
            || old_props.desc != props.desc
        {
            self.rebuild_order();
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

        let total = self.order.len();
        let total_pages = total.div_ceil(PAGE_SIZE).max(1);
        let page = self.page.min(total_pages - 1);

        let persons = self.ordered().skip(page * PAGE_SIZE).take(PAGE_SIZE);

        let on_filter = link.callback(|e: InputEvent| {
            let input: web_sys::HtmlInputElement = e.target_unchecked_into();
            Msg::Filter(input.value())
        });

        let on_sort = link.callback(|e: Event| {
            let select: web_sys::HtmlSelectElement = e.target_unchecked_into();
            Msg::SetSort(match select.value().as_str() {
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
                <div class="row-split">
                    <h1>{"People"}</h1>
                    if self.loaded {
                        <h4 class="text-muted">{total}</h4>
                    }
                </div>

                <div class="list-controls">
                    <div class="search-field">
                        <span class="icon magnifying-glass" />
                        <input type="text" placeholder="Filter" value={self.filter.clone()} oninput={on_filter} />

                        if !self.filter.is_empty() {
                            <Button icon="x-mark" title="Clear filter" class="ghost" onclick={link.callback(|_| Msg::Filter(String::new()))} />
                        }
                    </div>

                    <div class="chips">
                        <label class="chip-select" title="Sort by">
                            <span class="icon arrows-up-down" />

                            <select onchange={on_sort}>
                                <option value="name" selected={matches!(self.sort, PersonSort::Name)}>
                                    {"Name"}
                                </option>
                                <option value="credits" selected={matches!(self.sort, PersonSort::Credits)}>
                                    {"Credits"}
                                </option>
                            </select>
                        </label>

                        <Button icon={dir_icon} title={dir_title} class="chip" onclick={link.callback(|_| Msg::ToggleDir)} />
                    </div>

                    if self.loaded {
                        <PaginationButtons {page} {total_pages} on_page={link.callback(Msg::SetPage)} />
                    }
                </div>

                if !self.loaded || self.list_req.is_pending() {
                    <div class="row center">
                        <span class="item-inline-more"><span class="icon arrow-path spin" /></span>
                    </div>
                } else if persons.len() == 0 {
                    <div class="row center">
                        <span class="item-inline-more">{"Nothing to show."}</span>
                    </div>
                } else {
                    <div class="person-grid">
                        { for persons.into_iter().map(|p| self.view_card(p)) }
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
            Msg::Channel(result) => {
                self.channel = result?;

                if self.channel.id() != ws::ChannelId::NONE {
                    self.load(ctx);
                } else {
                    self.persons.clear();
                    self.loaded = false;
                    self.rebuild_order();
                }

                Ok(true)
            }
            Msg::AppBroadcast(packet) => {
                let event = packet?.decode_event()?;

                if event.channel == self.channel.id() {
                    return Ok(false);
                }

                // People are seeded/pruned by credit sync and updated by their own
                // sync, so react to both.
                let relevant = matches!(
                    event.kind,
                    api::AppEventKind::PersonChanged { .. }
                        | api::AppEventKind::CreditsChanged { .. }
                );

                if relevant && self.channel.id() != ws::ChannelId::NONE {
                    self.load(ctx);
                }

                Ok(false)
            }
            Msg::Loaded(result) => {
                self.persons = result
                    .context(Message::LoadingPersons)?
                    .decode()
                    .context(Message::LoadingPersons)?
                    .persons;
                self.loaded = true;
                self.rebuild_order();
                Ok(true)
            }
            Msg::Filter(filter) => {
                self.filter = filter;
                self.page = 0;
                self.rebuild_order();
                self.emit_navigate();
                Ok(true)
            }
            Msg::SetSort(sort) => {
                self.sort = sort;
                self.desc = sort.default_desc();
                self.rebuild_order();
                self.emit_navigate();
                Ok(true)
            }
            Msg::ToggleDir => {
                self.desc = !self.desc;
                self.rebuild_order();
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

    /// Rebuild `order` for the current filter and active sort. Called whenever
    /// `persons` or any ordering input changes.
    fn rebuild_order(&mut self) {
        let filter = self.filter.to_lowercase();

        self.order.clear();
        self.order.extend(
            self.persons
                .iter()
                .enumerate()
                .filter(|(_, p)| {
                    filter.is_empty()
                        || p.name
                            .texts(api::StringKind::Title)
                            .any(|t| t.to_lowercase().contains(&filter))
                })
                .map(|(i, _)| i),
        );

        match self.sort {
            PersonSort::Name => {
                // People without any name go last whichever way names sort.
                self.order.sort_by_cached_key(|&i| {
                    let name = display_name(&self.persons[i]);
                    (name.is_none(), name.map(str::to_lowercase))
                });

                if self.desc {
                    let named = self
                        .order
                        .iter()
                        .take_while(|&&i| display_name(&self.persons[i]).is_some())
                        .count();

                    self.order[..named].reverse();
                }
            }
            PersonSort::Credits => {
                self.order.sort_by_key(|&i| self.persons[i].credit_count);

                if self.desc {
                    self.order.reverse();
                }
            }
        }
    }

    /// People matching the current filter, ordered by the active sort, as
    /// maintained in `order`.
    fn ordered(&self) -> impl ExactSizeIterator<Item = &api::PersonItem> {
        self.order.iter().map(|&i| &self.persons[i])
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
        if self.channel.id() == ws::ChannelId::NONE {
            return;
        }

        self.list_req = self
            .channel
            .request()
            .body(api::ListPersonsRequest)
            .on_packet(ctx.link().callback(Msg::Loaded))
            .send();
    }

    fn view_card(&self, p: &api::PersonItem) -> Html {
        let name = display_name(p).unwrap_or("Unknown").to_owned();
        let route = Route::PersonDetail(p.id);

        html! {
            <Link to={route} class="person-card lift">
                <Image class="person-photo artwork" placeholder={true} placeholder_icon="user" src={p.profile.clone()} alt={name.clone()} />

                <div class="person-info">
                    <div class="person-name">{ name }</div>

                    if let Some(department) = &p.department {
                        <div class="person-department text-muted">{ department.clone() }</div>
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
    p.name
        .title()
        .or_else(|| p.name.texts(api::StringKind::Title).next())
}
