use yew::prelude::*;

use super::{Button, Image, PaginationButtons, Variant};

const GALLERY_PAGE_SIZE: usize = 4;

/// A single entry shown in the image gallery.
#[derive(Clone, PartialEq)]
pub(crate) struct ImageItem {
    pub(crate) selected: bool,
    pub(crate) id: api::ImageId,
    pub(crate) kind: api::ImageKind,
    pub(crate) source: api::ImageSource,
    pub(crate) image: api::Image,
    /// Raw remote score, sorted within a single remote. Absent for owners we
    /// don't score (seasons/episodes).
    pub(crate) score: Option<f64>,
}

#[derive(Properties, PartialEq)]
pub(crate) struct Props {
    /// Items arrive grouped by remote and sorted best-first (backend `rank`
    /// order), so they render grouped-by-remote with the top score first.
    pub(crate) items: Vec<ImageItem>,
    pub(crate) kind: api::ImageKind,
    pub(crate) on_select: Callback<api::ImageId>,
    #[prop_or_default]
    pub(crate) on_clear: Callback<()>,
    /// Whether the current selection for this kind is an explicit user pick.
    #[prop_or_default]
    pub(crate) user_selected: bool,
    /// Select the best stored graphic for this kind (configured remote order).
    #[prop_or_default]
    pub(crate) on_pick_best: Option<Callback<()>>,
    /// Clear the user override, handing the kind back to automatic management.
    #[prop_or_default]
    pub(crate) on_reset: Option<Callback<()>>,
}

#[function_component]
pub(crate) fn ImageGallery(props: &Props) -> Html {
    let page = use_state(|| 0usize);

    let total_pages = props.items.len().div_ceil(GALLERY_PAGE_SIZE);
    let this_page = (*page).min(total_pages.saturating_sub(1));
    let start = this_page * GALLERY_PAGE_SIZE;
    let end = (start + GALLERY_PAGE_SIZE).min(props.items.len());
    let page_images = props.items.get(start..end).unwrap_or_default();

    let on_page = {
        let page = page.clone();
        Callback::from(move |p| page.set(p))
    };

    let on_select = props.on_select.clone();
    let on_clear = props.on_clear.reform(|_| ());

    html! {
        <>
            <div class="row-split">
                <h2>{props.kind.title()}</h2>

                <div class="row">
                    if let Some(on_pick_best) = &props.on_pick_best {
                        <Button icon="sparkles" variant={Variant::Primary} title={format!("Pick best {}", props.kind)} text="Pick best" onclick={on_pick_best.reform(|_| ())} />
                    }
                    if props.user_selected {
                        if let Some(on_reset) = &props.on_reset {
                            <Button icon="arrow-uturn-left" title={format!("Clear custom {}", props.kind)} text="Clear custom" onclick={on_reset.reform(|_| ())} />
                        }
                    }
                    <Button icon="x-mark" variant={Variant::Danger} title={format!("Clear {}", props.kind)} text={format!("Clear {}", props.kind)} onclick={on_clear} />
                </div>
            </div>

            <div class="row desktop-align-end">
                <PaginationButtons page={this_page} {total_pages} on_page={on_page} />
            </div>

            if props.items.is_empty() {
                <div class="text-muted">{"No images"}</div>
            } else {
                <div class={classes!("image-gallery", props.kind.as_str())}>
                    { for page_images.iter().map(|img| {
                        let id = img.id;
                        let selected = img.selected;
                        let on_select = on_select.clone();
                        let source = img.source.as_str();
                        let title = img.source.to_string();

                        html! {
                            <div class="gallery-cell">
                                <Image class={classes!("clickable", selected.then_some("selected"))} onclick={Callback::from(move |_| on_select.emit(id))} title={title.clone()} src={img.image.clone()} />
                                <div class="gallery-meta">
                                    <span class={classes!("logo", source)} title={title} />
                                    if let Some(score) = img.score {
                                        <span class="text-muted">{format!("{score:.1}")}</span>
                                    }
                                </div>
                            </div>
                        }
                    })}
                </div>
            }
        </>
    }
}
