use yew::prelude::*;

#[derive(Properties, PartialEq)]
pub(super) struct PaginationButtonsProps {
    pub(super) page: usize,
    pub(super) total_pages: usize,
    pub(super) on_page: Callback<usize>,
}

#[function_component]
pub(super) fn PaginationButtons(props: &PaginationButtonsProps) -> Html {
    let page = props.page.min(props.total_pages.saturating_sub(1));
    let prev = page.checked_sub(1);
    let next = (page + 1 < props.total_pages).then_some(page + 1);
    let on_page = props.on_page.clone();
    let on_page2 = props.on_page.clone();

    if props.total_pages <= 1 {
        return html! {};
    }

    html! {
        <>
            <button class="btn-icon" disabled={prev.is_none()}
                onclick={Callback::from(move |_| { if let Some(p) = prev { on_page.emit(p); } })}>
                <span class="icon arrow-left" />
            </button>
            <span class="text-muted">{format!("{} / {}", page + 1, props.total_pages)}</span>
            <button class="btn-icon" disabled={next.is_none()}
                onclick={Callback::from(move |_| { if let Some(p) = next { on_page2.emit(p); } })}>
                <span class="icon arrow-right" />
            </button>
        </>
    }
}

#[derive(Properties, PartialEq)]
pub(super) struct ConfirmDangerProps {
    pub(super) label: AttrValue,
    pub(super) on_confirm: Callback<()>,
    pub(super) on_cancel: Callback<()>,
    pub(super) prompt: AttrValue,
    #[prop_or_default]
    pub(super) btn_class: Classes,
}

#[function_component]
pub(super) fn ConfirmDanger(props: &ConfirmDangerProps) -> Html {
    let on_confirm = {
        let cb = props.on_confirm.clone();

        Callback::from(move |e: MouseEvent| {
            e.stop_propagation();
            cb.emit(());
        })
    };

    let on_cancel = {
        let cb = props.on_cancel.clone();
        Callback::from(move |e: MouseEvent| {
            e.stop_propagation();
            cb.emit(());
        })
    };

    html! {
        <div class="row-fill fill">
            <span class="fill">{&props.prompt}{" "}<strong>{&props.label}</strong>{"?"}</span>

            <div class="input-group end">
                <button onclick={on_cancel} class={classes!("btn-icon", &props.btn_class)} title="No">
                    <span class="icon x-mark" />
                </button>

                <button onclick={on_confirm} class={classes!("btn-icon-danger", &props.btn_class)} title="Yes">
                    <span class="icon check" />
                </button>
            </div>
        </div>
    }
}

// ── RemoteSourceSelect ───────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum RemoteSourceKind {
    Series,
    Movie,
}

#[derive(Properties, PartialEq)]
pub(super) struct RemoteSourceSelectProps {
    pub(super) remotes: Vec<api::RemoteId>,
    pub(super) current_source: Option<api::SyncSource>,
    pub(super) kind: RemoteSourceKind,
    pub(super) on_change: Callback<api::SyncSource>,
}

#[function_component]
pub(super) fn RemoteSourceSelect(props: &RemoteSourceSelectProps) -> Html {
    let mut options: Vec<api::SyncSource> = Vec::new();

    for remote in &props.remotes {
        let Some(source) = api::SyncSource::from_remote_source(remote.source()) else {
            continue;
        };

        if source == api::SyncSource::Tmdb
            || (matches!(props.kind, RemoteSourceKind::Series) && source == api::SyncSource::Tvdb)
        {
            if !options.iter().any(|existing| existing == &source) {
                options.push(source);
            }
        }
    }

    if options.is_empty() {
        return Html::default();
    }

    let selected = props
        .current_source
        .as_ref()
        .filter(|source| options.iter().any(|option| option == *source))
        .copied()
        .unwrap_or(options[0]);

    let disabled = options.len() <= 1;

    let on_change = {
        let cb = props.on_change.clone();

        Callback::from(move |e: Event| {
            let input: web_sys::HtmlSelectElement = e.target_unchecked_into();
            if let Some(source) = api::SyncSource::from_str(&input.value()) {
                cb.emit(source);
            }
        })
    };

    html! {
        <select class="input-select" value={selected.as_str()} onchange={on_change} {disabled} title="Select remote source">
            {
                for options.into_iter().map(|source| {
                    let label = source.as_str().to_uppercase();

                    html! {
                        <option value={source.as_str()}>{label}</option>
                    }
                })
            }
        </select>
    }
}

// ── ImageGallery ──────────────────────────────────────────────────────────────

/// A single entry shown in the image gallery.
#[derive(Clone, PartialEq)]
pub(super) struct ImageItem {
    pub(super) id: api::ImageId,
    pub(super) kind: api::ImageKind,
    pub(super) source: api::ImageSource,
    pub(super) image: api::Image,
    pub(super) selected: bool,
}

#[derive(Properties, PartialEq)]
pub(super) struct ImageGalleryProps {
    pub(super) items: Vec<ImageItem>,
    pub(super) kind: api::ImageKind,
    pub(super) on_select: Callback<api::ImageId>,
    pub(super) on_close: Callback<()>,
}

#[function_component]
pub(super) fn ImageGallery(props: &ImageGalleryProps) -> Html {
    let imgs: Vec<_> = props
        .items
        .iter()
        .filter(|img| img.kind == props.kind)
        .collect();
    let on_select = props.on_select.clone();
    let on_close = props.on_close.clone();

    html! {
        <div class="modal-background" onclick={Callback::from(move |_| on_close.emit(()))}>
            <div class="modal" onclick={Callback::from(|e: MouseEvent| e.stop_propagation())}>
                <div class="row">
                    <span class="fill">{format!("Select {}", props.kind)}</span>
                    <button class="btn-icon" onclick={props.on_close.reform(|_| ())}>
                        <span class="icon x-mark" />
                    </button>
                </div>

                if imgs.is_empty() {
                    <div class="empty text-muted">{"No images available."}</div>
                } else {
                    <div class="image-gallery">
                        { for imgs.iter().map(|img| {
                            let id = img.id;
                            let selected = img.selected;
                            let on_select = on_select.clone();
                            let title = img.source.to_string();
                            html! {
                                <div
                                    class={classes!("image-thumb", selected.then_some("image-thumb-selected"))}
                                    onclick={Callback::from(move |_| on_select.emit(id))}
                                    {title}
                                >
                                    <img src={img.image.proxy_url()} alt="" />
                                    if selected {
                                        <span class="image-thumb-check">{"✓"}</span>
                                    }
                                </div>
                            }
                        })}
                    </div>
                }
            </div>
        </div>
    }
}
