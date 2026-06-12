use gloo::events::EventListener;
use yew::prelude::*;

use crate::App;
use crate::background::BackgroundState;
use crate::error::{Error, RcError};
use crate::router::{Route, RouterState};

pub(super) struct Root {
    router: RouterState,
    background: BackgroundState,
    _history_listener: Option<EventListener>,
    error: Option<RcError>,
}

pub(super) enum Msg {
    Navigate(Route),
    PopState,
    SetBackground(String),
    Error(Option<Error>),
}

impl Component for Root {
    type Message = Msg;
    type Properties = ();

    fn create(ctx: &Context<Self>) -> Self {
        let background = BackgroundState::new();

        let router = RouterState::new().expect("Setting up router");
        let _history_listener = router.on_change(ctx.link().callback(|()| Msg::PopState));

        Self {
            router,
            background,
            _history_listener: Some(_history_listener),
            error: None,
        }
    }

    fn update(&mut self, _ctx: &Context<Self>, msg: Self::Message) -> bool {
        match msg {
            Msg::Navigate(route) => {
                if let Err(e) = self.router.navigate(&route) {
                    self.error = Some(RcError::from(e));
                }

                true
            }
            Msg::PopState => {
                if let Err(e) = self.router.on_pop() {
                    self.error = Some(RcError::from(e));
                }

                true
            }
            Msg::SetBackground(background) => {
                self.background.set(background);
                true
            }
            Msg::Error(e) => {
                self.error = e.map(RcError::from);
                true
            }
        }
    }

    fn view(&self, ctx: &Context<Self>) -> Html {
        let link = ctx.link();

        let route = self.router.route.clone();
        let onerror = link.callback(Msg::Error);
        let on_navigate = link.callback(Msg::Navigate);
        let on_background = link.callback(Msg::SetBackground);
        let style = self.background.style();

        html! {
            <>
                <div class="background" {style} />
                <App error={self.error.clone()} {route} {onerror} {on_navigate} {on_background} />
            </>
        }
    }
}
