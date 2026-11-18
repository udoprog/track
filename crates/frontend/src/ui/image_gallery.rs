use yew::prelude::*;

use crate::class;
use crate::ui::Pager;

use super::{Button, Image, Variant};

/// The gallery's modifier class for a kind of image, if it has one.
fn kind_class(kind: api::ImageKind) -> Option<&'static str> {
    match kind {
        api::ImageKind::Poster => Some(class::POSTER),
        api::ImageKind::Banner => Some(class::BANNER),
        api::ImageKind::Backdrop => Some(class::BACKDROP),
        api::ImageKind::Screenshot => Some(class::SCREENSHOT),
        api::ImageKind::Profile | api::ImageKind::Unknown => None,
    }
}

/// How many images of a kind fit a page of the gallery.
fn page_size(kind: api::ImageKind) -> usize {
    match kind {
        api::ImageKind::Poster | api::ImageKind::Profile => 8,
        api::ImageKind::Banner => 3,
        _ => 4,
    }
}

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

    let size = page_size(props.kind);
    let total_pages = props.items.len().div_ceil(size);
    let this_page = (*page).min(total_pages.saturating_sub(1));
    let start = this_page * size;
    let end = (start + size).min(props.items.len());
    let page_images = props.items.get(start..end).unwrap_or_default();

    let on_page = {
        let page = page.clone();
        Callback::from(move |p| page.set(p))
    };

    let on_select = props.on_select.clone();
    let on_clear = props.on_clear.reform(|_| ());

    html! {
        <>
            <div class={classes!(sockel::class::CLUSTER, sockel::class::BETWEEN, sockel::class::LOOSE)}>
                <h2>{props.kind.title()}</h2>

                <div class={classes!(sockel::class::CLUSTER, sockel::class::NOWRAP, sockel::class::LOOSE)}>
                    if let Some(on_pick_best) = &props.on_pick_best {
                        <Button icon={class::ICON_SPARKLES} variant={Variant::Primary} title={format!("Pick best {}", props.kind)} label="Pick best" onclick={on_pick_best.reform(|_| ())} />
                    }
                    if props.user_selected {
                        if let Some(on_reset) = &props.on_reset {
                            <Button icon={class::ICON_ARROW_UTURN_LEFT} title={format!("Clear custom {}", props.kind)} label="Clear custom" onclick={on_reset.reform(|_| ())} />
                        }
                    }
                    <Button icon={class::ICON_X_MARK} title={format!("Clear {}", props.kind)} label="Clear" onclick={on_clear} />
                </div>
            </div>

            <div class={classes!(class::DESKTOP_ALIGN_END, sockel::class::CLUSTER, sockel::class::NOWRAP, sockel::class::LOOSE)}>
                <Pager page={this_page} pages={total_pages} on_page={on_page} />
            </div>

            if props.items.is_empty() {
                <div class={class::TEXT_MUTED}>{"No images"}</div>
            } else {
                <div class={classes!(class::IMAGE_GALLERY, kind_class(props.kind))}>
                    { for page_images.iter().map(|img| {
                        let id = img.id;
                        let selected = img.selected;
                        let on_select = on_select.clone();
                        let source = img.source.as_str();
                        let title = img.source.to_string();

                        html! {
                            <div class={class::GALLERY_CELL}>
                                <Button class={class::GALLERY_PICK} title={format!("Use this {} from {title}", props.kind)} pressed={Some(selected)} onclick={Callback::from(move |_| on_select.emit(id))}>
                                    <Image class={classes!(selected.then_some(class::SELECTED))} src={img.image.clone()} />
                                </Button>
                                <div class={class::GALLERY_META}>
                                    <span class={classes!(class::LOGO, class::logo(source))} title={title} />
                                    if selected {
                                        <span class={class::BADGE}>{"Current"}</span>
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
