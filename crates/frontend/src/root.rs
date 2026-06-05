use gloo::events::EventListener;
use yew::prelude::*;

use crate::App;
use crate::error::{Error, RcError};
use crate::router::{Route, RouterState};

pub(super) struct Root {
    router: Option<RouterState>,
    _history_listener: Option<EventListener>,
    error: Option<RcError>,
}

pub(super) enum Msg {
    Navigate(Route),
    PopState,
    Error(Error),
    ClearError,
}

impl Component for Root {
    type Message = Msg;
    type Properties = ();

    fn create(ctx: &Context<Self>) -> Self {
        match RouterState::new() {
            Ok(r) => {
                let _history_listener = r.on_change(ctx.link().callback(|()| Msg::PopState));
                Self {
                    router: Some(r),
                    _history_listener: Some(_history_listener),
                    error: None,
                }
            }
            Err(e) => Self {
                router: None,
                _history_listener: None,
                error: Some(RcError::from(e)),
            },
        }
    }

    fn update(&mut self, _ctx: &Context<Self>, msg: Self::Message) -> bool {
        match msg {
            Msg::Navigate(route) => {
                if let Some(ref mut r) = self.router {
                    if let Err(e) = r.navigate(&route) {
                        self.error = Some(RcError::from(e));
                    }
                }
                true
            }
            Msg::PopState => {
                if let Some(ref mut r) = self.router {
                    if let Err(e) = r.on_pop() {
                        self.error = Some(RcError::from(e));
                    }
                }
                true
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

        if let Some(ref err) = self.error {
            let msgs: Vec<String> = err.sources().map(|e| e.to_string()).collect();
            return html! {
                <div class="error-page">
                    <div class="error-box">
                        { for msgs.iter().map(|m| html! { <p>{m}</p> }) }
                        <button onclick={link.callback(|_| Msg::ClearError)} class="btn">
                            {"Dismiss"}
                        </button>
                    </div>
                </div>
            };
        }

        let Some(ref router) = self.router else {
            return html! { <div class="loading">{"Loading…"}</div> };
        };

        let route = router.route.clone();
        let onerror = link.callback(Msg::Error);
        let on_navigate = link.callback(Msg::Navigate);

        html! {
            <App {route} {onerror} {on_navigate} />
        }
    }
}
