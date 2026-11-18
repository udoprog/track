use crate::class;
use yew::prelude::*;

use sockel::{Ratio, Skeleton, SkeletonShape};

/// Structural placeholder for a show or movie detail page shown while the record
/// loads. Mirrors the `detail-layout` shell (title header, poster sidebar,
/// content lines) so the page keeps its shape instead of collapsing to a spinner.
#[function_component]
pub(crate) fn DetailSkeleton() -> Html {
    html! {
        <>
            <div class={sockel::class::STACK}>
                <Skeleton shape={SkeletonShape::Line} width="14rem" />

                <div class={classes!(sockel::class::CLUSTER, sockel::class::NOWRAP, sockel::class::LOOSE)}>
                    <Skeleton width="6rem" />
                    <Skeleton width="6rem" />
                </div>
            </div>

            <div class={class::DETAIL_LAYOUT}>
                <div class={classes!(class::DETAIL_SIDEBAR, sockel::class::STACK, sockel::class::LOOSE)}>
                    <Skeleton shape={SkeletonShape::Image(Ratio::Poster)} class={class::DESKTOP_ONLY} />
                </div>

                <div class={classes!(sockel::class::STACK, sockel::class::LOOSE)} data-testid="detail-content">
                    <Skeleton shape={SkeletonShape::Line} />
                    <Skeleton shape={SkeletonShape::Line} width="90%" />
                    <Skeleton shape={SkeletonShape::Line} width="80%" />
                    <Skeleton shape={SkeletonShape::Line} width="60%" />
                </div>
            </div>
        </>
    }
}
