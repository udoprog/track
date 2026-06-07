use yew::prelude::*;

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
    pub(super) current_source: Option<String>,
    pub(super) kind: RemoteSourceKind,
    pub(super) on_change: Callback<String>,
}

#[function_component]
pub(super) fn RemoteSourceSelect(props: &RemoteSourceSelectProps) -> Html {
    let mut options: Vec<String> = Vec::new();

    for remote in &props.remotes {
        let source = remote.source();

        if source == "tmdb" || (matches!(props.kind, RemoteSourceKind::Series) && source == "tvdb") {
            let source = source.to_owned();

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
        .as_deref()
        .filter(|source| options.iter().any(|option| option == source))
        .map(str::to_owned)
        .unwrap_or_else(|| options[0].clone());

    let disabled = options.len() <= 1;

    let on_change = {
        let cb = props.on_change.clone();
        Callback::from(move |e: Event| {
            let input: web_sys::HtmlSelectElement = e.target_unchecked_into();
            cb.emit(input.value());
        })
    };

    html! {
        <select class="input-select" value={selected} onchange={on_change} {disabled} title="Select remote source">
            {
                for options.into_iter().map(|source| {
                    let label = source.to_uppercase();

                    html! {
                        <option value={source.clone()}>{label}</option>
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
