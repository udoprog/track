use gloo::events::EventListener;
use web_sys::HtmlImageElement;
use yew::prelude::*;

enum State {
    Loading,
    Loaded(String),
    Error,
    Empty,
}

pub(super) struct Image {
    state: State,
    _img: Option<HtmlImageElement>,
    _load: Option<EventListener>,
    _error: Option<EventListener>,
}

pub(super) enum Msg {
    Loaded(String),
    Error,
}

#[derive(Properties, PartialEq)]
pub(super) struct Props {
    #[prop_or_default]
    pub(super) placeholder: bool,
    #[prop_or_default]
    pub(super) src: Option<api::Image>,
    #[prop_or_default]
    pub(super) title: Option<String>,
    #[prop_or_default]
    pub(super) class: Classes,
    #[prop_or_default]
    pub(super) style: Option<String>,
    #[prop_or_default]
    pub(super) alt: AttrValue,
    #[prop_or_default]
    pub(super) onclick: Callback<MouseEvent>,
}

impl Component for Image {
    type Message = Msg;
    type Properties = Props;

    fn create(ctx: &Context<Self>) -> Self {
        let mut this = Self {
            state: State::Loading,
            _img: None,
            _load: None,
            _error: None,
        };
        this.begin_load(ctx);
        this
    }

    fn changed(&mut self, ctx: &Context<Self>, old_props: &Self::Properties) -> bool {
        if ctx.props().src != old_props.src {
            self.state = State::Loading;
            self.begin_load(ctx);
        }

        true
    }

    fn update(&mut self, _ctx: &Context<Self>, msg: Self::Message) -> bool {
        self._img = None;
        self._load = None;
        self._error = None;

        self.state = match msg {
            Msg::Loaded(src) => State::Loaded(src),
            Msg::Error => State::Error,
        };

        true
    }

    fn view(&self, ctx: &Context<Self>) -> Html {
        let props = ctx.props();

        let class = classes!(
            props.class.clone(),
            matches!(self.state, State::Loading).then_some("loading")
        );

        match self.state {
            State::Loading => html! {
                <image {class} style={props.style.clone()} onclick={props.onclick.clone()} title={props.title.clone()}>
                    <span class="icon arrow-path spin" />
                </image>
            },
            State::Loaded(ref src) => html! {
                <image {class} style={props.style.clone()} onclick={props.onclick.clone()} title={props.title.clone()}>
                    <img src={src.clone()} alt={props.alt.clone()} />
                </image>
            },
            State::Error => html! {
                <image {class} style={props.style.clone()} onclick={props.onclick.clone()} title={props.title.clone()}>
                    <span class="icon exclamation-triangle" />
                </image>
            },
            State::Empty if props.placeholder => html! {
                <image {class} style={props.style.clone()} onclick={props.onclick.clone()} title={props.title.clone()}>
                    <span class="icon question-mark-circle" />
                </image>
            },
            _ => html!(),
        }
    }
}

impl Image {
    fn begin_load(&mut self, ctx: &Context<Self>) {
        let Some(src) = &ctx.props().src else {
            self.state = State::Empty;
            return;
        };

        let Ok(img) = HtmlImageElement::new() else {
            return;
        };

        let url = src.proxy_url();

        let link = ctx.link().clone();
        let load = EventListener::new(&img, "load", {
            let url = url.clone();

            move |_| {
                link.send_message(Msg::Loaded(url.clone()));
            }
        });

        let link = ctx.link().clone();
        let error = EventListener::new(&img, "error", move |_| {
            link.send_message(Msg::Error);
        });

        img.set_src(&url);

        self._img = Some(img);
        self._load = Some(load);
        self._error = Some(error);
    }
}
