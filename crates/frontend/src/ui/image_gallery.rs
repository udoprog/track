use yew::prelude::*;

use super::{Image, PaginationButtons};

const GALLERY_PAGE_SIZE: usize = 4;

/// A single entry shown in the image gallery.
#[derive(Clone, PartialEq)]
pub(crate) struct ImageItem {
    pub(crate) selected: bool,
    pub(crate) id: api::ImageId,
    pub(crate) kind: api::ImageKind,
    pub(crate) source: api::ImageSource,
    pub(crate) image: api::Image,
}

#[derive(Properties, PartialEq)]
pub(crate) struct Props {
    pub(crate) items: Vec<ImageItem>,
    pub(crate) kind: api::ImageKind,
    pub(crate) on_select: Callback<api::ImageId>,
    #[prop_or_default]
    pub(crate) on_clear: Callback<()>,
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
                    <button class="btn-danger" onclick={on_clear}>
                        <span class="icon x-mark" />
                        {format!("Clear {}", props.kind)}
                    </button>
                </div>
            </div>

            if props.items.is_empty() {
                <div class="text-muted">{"No images"}</div>
            } else {
                if total_pages > 1 {
                    <PaginationButtons page={this_page} {total_pages} on_page={on_page} />
                }

                <div class={classes!("image-gallery", props.kind.as_str())}>
                    { for page_images.iter().map(|img| {
                        let id = img.id;
                        let selected = img.selected;
                        let on_select = on_select.clone();
                        let title = img.source.to_string();

                        html! {
                            <Image class={classes!("clickable", selected.then_some("selected"))} onclick={Callback::from(move |_| on_select.emit(id))} {title} src={img.image.clone()} />
                        }
                    })}
                </div>
            }
        </>
    }
}
