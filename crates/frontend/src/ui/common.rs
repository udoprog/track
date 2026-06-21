use web_sys::MouseEvent;
use yew::prelude::*;

use crate::error::RcError;
use crate::router::MediaSelection;

use super::LOADING;

/// Loading indicator placed inside the shared page container (rendered by
/// `App`). Use [`LoadingPage`] for standalone, full-page loading screens.
#[function_component]
pub(crate) fn Loading() -> Html {
    html! {
        <div class="box info">
            <span class="item-inline">
                <span class="icon arrow-path spin" />
            </span>

            <span>{LOADING}</span>
        </div>
    }
}

#[derive(Properties, PartialEq)]
pub(crate) struct MediaKindToggleProps {
    pub(crate) selection: MediaSelection,
    pub(crate) on_change: Callback<MediaSelection>,
}

/// The shows/movies checkbox pair shared by the media list and search. Renders
/// as two `input-checkbox` spans (no wrapper) so it drops into an existing
/// `input-group`.
#[function_component]
pub(crate) fn MediaKindToggle(props: &MediaKindToggleProps) -> Html {
    let selection = props.selection;

    let on_shows = props.on_change.reform(move |_| MediaSelection {
        shows: !selection.shows,
        ..selection
    });

    let on_movies = props.on_change.reform(move |_| MediaSelection {
        movies: !selection.movies,
        ..selection
    });

    html! {
        <>
            <span
                class={classes!("input-checkbox", selection.shows.then_some("checked"))}
                title="Show series"
                onclick={on_shows}>
                <span class="icon tv" />
                <span class="mark" />
            </span>

            <span
                class={classes!("input-checkbox", selection.movies.then_some("checked"))}
                title="Show movies"
                onclick={on_movies}>
                <span class="icon film" />
                <span class="mark" />
            </span>
        </>
    }
}

#[derive(Properties, PartialEq)]
pub(crate) struct ErrorBoxProps {
    pub(crate) error: RcError,
    pub(crate) onclearerror: Callback<()>,
}

#[function_component]
pub(crate) fn ErrorBox(props: &ErrorBoxProps) -> Html {
    html! {
        <>
            <div class="column fill">
                { for props.error.sources().map(|e| html! { <p>{e.to_string()}</p> }) }
            </div>

            <button class="btn-danger" onclick={props.onclearerror.reform(|_| ())}>
                <span class="icon x-mark" />
            </button>
        </>
    }
}

#[derive(Properties, PartialEq)]
pub(crate) struct TrackedProps {
    pub(crate) tracked: bool,
    pub(crate) ontoggle: Callback<bool>,
}

#[function_component]
pub(crate) fn Tracked(props: &TrackedProps) -> Html {
    let tracked = props.tracked;

    html! {
        <button class="btn" onclick={props.ontoggle.reform(move |_| !tracked)} title="Track movie">
            <span class={classes!("icon", if tracked { "eye" } else { "eye-slash" })} />
            <span class="hide-desktop">{if tracked { "Tracking" } else { "Not tracking" }}</span>
        </button>
    }
}

#[derive(Properties, PartialEq)]
pub(crate) struct PaginationButtonsProps {
    pub(crate) page: usize,
    pub(crate) total_pages: usize,
    pub(crate) on_page: Callback<usize>,
}

#[function_component]
pub(crate) fn PaginationButtons(props: &PaginationButtonsProps) -> Html {
    let page = props.page.min(props.total_pages.saturating_sub(1));
    let prev = page.checked_sub(1);
    let next = page.checked_add(1).filter(|&v| v < props.total_pages);

    let on_back = prev.map(|prev| props.on_page.reform(move |_: MouseEvent| prev));
    let on_next = next.map(|next| props.on_page.reform(move |_: MouseEvent| next));

    html! {
        <>
            <button class={classes!("btn", prev.is_none().then_some("disabled"))} onclick={on_back}>
                <span class="icon chevron-left" />
            </button>

            <span class="input-text">
                {format!("{} / {}", page.saturating_add(1), props.total_pages)}
            </span>

            <button class={classes!("btn", next.is_none().then_some("disabled"))} onclick={on_next}>
                <span class="icon chevron-right" />
            </button>
        </>
    }
}

#[derive(Properties, PartialEq)]
pub(crate) struct ConfirmDangerProps {
    pub(crate) prompt: AttrValue,
    #[prop_or_default]
    pub(crate) icon: Option<AttrValue>,
    #[prop_or_default]
    pub(crate) label: Option<AttrValue>,
    pub(crate) on_confirm: Callback<()>,
    pub(crate) on_cancel: Callback<()>,
    #[prop_or_default]
    pub(crate) btn_class: Classes,
}

#[function_component]
pub(crate) fn ConfirmDanger(props: &ConfirmDangerProps) -> Html {
    let on_confirm = props.on_confirm.reform(move |e: MouseEvent| {
        e.stop_propagation();
    });

    let on_cancel = props.on_cancel.reform(move |e: MouseEvent| {
        e.stop_propagation();
    });

    html! {
        <div class="row-fill fill">
            <div class="row">
                if let Some(ref icon) = props.icon {
                    <span class="item-inline">
                        <span class={classes!("icon", icon)} />
                    </span>
                }

                <span>{&props.prompt}</span>

                if let Some(ref label) = props.label {
                    <span>{label}{"?"}</span>
                }
            </div>

            <div class="input-group end">
                <button onclick={on_cancel} class={classes!("btn", &props.btn_class)} title="No">
                    <span class="icon x-mark" />
                </button>

                <button onclick={on_confirm} class={classes!("btn-danger", &props.btn_class)} title="Yes">
                    <span class="icon check" />
                </button>
            </div>
        </div>
    }
}
