use yew::prelude::*;

use plinth::{Ratio, Skeleton, SkeletonShape};

/// Structural placeholder for a show or movie detail page shown while the record
/// loads. Mirrors the `detail-layout` shell (title header, poster sidebar,
/// content lines) so the page keeps its shape instead of collapsing to a spinner.
#[function_component]
pub(crate) fn DetailSkeleton() -> Html {
    html! {
        <>
            <div class="plinth-stack">
                <Skeleton shape={SkeletonShape::Line} width="14rem" />

                <div class="plinth-cluster plinth-nowrap plinth-loose">
                    <Skeleton width="6rem" />
                    <Skeleton width="6rem" />
                </div>
            </div>

            <div class="detail-layout">
                <div class="detail-sidebar plinth-stack plinth-loose">
                    <Skeleton shape={SkeletonShape::Image(Ratio::Poster)} class="desktop-only" />
                </div>

                <div class="shrink plinth-stack plinth-loose" data-testid="detail-content">
                    <Skeleton shape={SkeletonShape::Line} />
                    <Skeleton shape={SkeletonShape::Line} width="90%" />
                    <Skeleton shape={SkeletonShape::Line} width="80%" />
                    <Skeleton shape={SkeletonShape::Line} width="60%" />
                </div>
            </div>
        </>
    }
}
