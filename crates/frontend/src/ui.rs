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

// ── ImageGallery ──────────────────────────────────────────────────────────────

/// A single entry shown in the image gallery.
#[derive(Clone, PartialEq)]
pub(super) struct ImageItem {
    pub(super) id: u64,
    pub(super) kind: api::ImageKind,
    pub(super) source: api::ImageSource,
    pub(super) image: api::Image,
    pub(super) selected: bool,
}

#[derive(Properties, PartialEq)]
pub(super) struct ImageGalleryProps {
    pub(super) items: Vec<ImageItem>,
    pub(super) on_select: Callback<u64>,
}

#[function_component]
pub(super) fn ImageGallery(props: &ImageGalleryProps) -> Html {
    if props.items.is_empty() {
        return Html::default();
    }

    let on_select = props.on_select.clone();

    html! {
        <div class="section">
            <div class="section text-muted">{"Images"}</div>
            { for [api::ImageKind::Poster, api::ImageKind::Banner, api::ImageKind::Fanart, api::ImageKind::Backdrop].iter().filter_map(|&kind| {
                let imgs: Vec<_> = props.items.iter().filter(|img| img.kind == kind).collect();
                if imgs.is_empty() { return None; }
                let on_select = on_select.clone();
                Some(html! {
                    <div class="section">
                        <div class="row text-muted">
                            <span class="fill">{kind.to_string()}</span>
                        </div>
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
                    </div>
                })
            })}
        </div>
    }
}
