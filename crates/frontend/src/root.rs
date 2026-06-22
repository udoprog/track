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
    Replace(Route),
    PopState,
    SetBackground(String),
    SetTitle(Option<String>),
    Error(Error),
    ClearError,
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
            Msg::Replace(route) => {
                if let Err(e) = self.router.replace(&route) {
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
                self.background.set_background(background);
                true
            }
            Msg::SetTitle(title) => {
                self.background.set_title(title);
                false
            }
            Msg::Error(e) => {
                self.error = Some(RcError::from(e));
                true
            }
            Msg::ClearError => {
                self.error = None;
                true
            }
        }
    }

    fn view(&self, ctx: &Context<Self>) -> Html {
        let link = ctx.link();

        let route = self.router.route.clone();
        let onerror = link.callback(Msg::Error);
        let onclearerror = link.callback(|()| Msg::ClearError);
        let on_navigate = link.callback(Msg::Navigate);
        let on_replace = link.callback(Msg::Replace);
        let on_background = link.callback(Msg::SetBackground);
        let on_title = link.callback(Msg::SetTitle);

        html! {
            <>
                <div class="background">
                    if let Some(url) = self.background.url() {
                        <div
                            key={url.to_owned()}
                            class="background-image"
                            style={format!("background-image: url('{url}')")}
                        />
                    }
                </div>

                <App error={self.error.clone()} {route} {onerror} {onclearerror} {on_navigate} {on_replace} {on_background} {on_title} />
            </>
        }
    }
}
