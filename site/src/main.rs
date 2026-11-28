//! track's site: what track is for, and its manual rendered from the app's
//! built-in help (`crates/frontend/help/`).

mod home;
mod manual;
mod route;

/// The site's own classes (`style/`), from sockel-build. Those of the
/// manual's samples are written into its HTML by `manual.rs` instead.
#[allow(dead_code)]
mod class {
    include!(concat!(env!("OUT_DIR"), "/classes.rs"));
}

/// sockel's CSS, then `style/`'s.
const STYLESHEET: &str = include_str!(concat!(env!("OUT_DIR"), "/style.css"));

pub const GITHUB: &str = "https://github.com/udoprog/track";

use wasm_bindgen::JsCast;
use wasm_bindgen::closure::Closure;
use yew::prelude::*;

use sockel::{Cluster, Gap, Justify};

use route::Route;

fn current_route() -> Route {
    let hash = web_sys::window()
        .and_then(|w| w.location().hash().ok())
        .unwrap_or_default();
    Route::parse(&hash)
}

/// Names the page in the window's title and shows its anchor, else its top.
fn show(route: &Route) {
    let Some(window) = web_sys::window() else {
        return;
    };

    let document = window.document();

    if let Some(document) = &document {
        document.set_title(&route.title());
    }

    let target = route
        .anchor()
        .and_then(|id| document?.get_element_by_id(id));

    match target {
        Some(target) => target.scroll_into_view(),
        None => window.scroll_to_with_x_and_y(0.0, 0.0),
    }
}

#[function_component]
fn App() -> Html {
    let route = use_state(current_route);

    {
        let route = route.clone();

        use_effect_with((), move |()| {
            let listener = Closure::<dyn Fn()>::new(move || route.set(current_route()));
            let window = web_sys::window();

            if let Some(window) = &window {
                let _ = window.add_event_listener_with_callback(
                    "hashchange",
                    listener.as_ref().unchecked_ref(),
                );
            }

            move || {
                if let Some(window) = window {
                    let _ = window.remove_event_listener_with_callback(
                        "hashchange",
                        listener.as_ref().unchecked_ref(),
                    );
                }
            }
        });
    }

    use_effect_with((*route).clone(), show);

    let page = match &*route {
        Route::Home { .. } => html! { <home::Home /> },
        Route::Manual { page, .. } => html! { <manual::Manual route={page.clone()} /> },
    };

    let in_manual = matches!(&*route, Route::Manual { .. });

    html! {
        <>
            <sockel::Styles css={STYLESHEET} />
            <header class={class::BAR}>
                <Cluster justify={Justify::Between} class={class::BAR_INNER}>
                    <a href="#/" class={class::BRAND}>
                        <span class={class::BRAND_MARK} aria-hidden="true" />
                        { "track" }
                    </a>
                    <Cluster tag="nav" gap={Gap::Loose}>
                        <a href="#/manual/" class={class::BAR_LINK}
                            aria-current={in_manual.then_some("page")}>{ "Manual" }</a>
                        <a href="#install" class={class::BAR_LINK}>{ "Install" }</a>
                        <a href={GITHUB} class={class::BAR_LINK}>{ "GitHub" }</a>
                    </Cluster>
                </Cluster>
            </header>
            { page }
            <footer class={class::FOOTER}>
                <Cluster gap={Gap::Loose} justify={Justify::Center}>
                    <a href="#/manual/">{ "Manual" }</a>
                    <a href={GITHUB}>{ "GitHub" }</a>
                    <a href={format!("{GITHUB}/releases")}>{ "Releases" }</a>
                </Cluster>
            </footer>
        </>
    }
}

fn main() {
    yew::Renderer::<App>::new().render();
}
