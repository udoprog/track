use yew::prelude::*;

use super::Skeleton;

/// Structural placeholder for a show or movie detail page shown while the record
/// loads. Mirrors the `detail-layout` shell (title header, poster sidebar,
/// content lines) so the page keeps its shape instead of collapsing to a spinner.
#[function_component]
pub(crate) fn DetailSkeleton() -> Html {
    html! {
        <>
            <div class="column">
                <Skeleton class="line" style="width: 14rem" />

                <div class="row">
                    <Skeleton style="width: 6rem" />
                    <Skeleton style="width: 6rem" />
                </div>
            </div>

            <div class="detail-layout">
                <div class="mobile-only">
                    <Skeleton class="backdrop" />
                </div>

                <div class="detail-sidebar">
                    <Skeleton class="poster desktop-only" />
                </div>

                <div class="detail-content">
                    <Skeleton class="line" />
                    <Skeleton class="line" style="width: 90%" />
                    <Skeleton class="line" style="width: 80%" />
                    <Skeleton class="line" style="width: 60%" />
                </div>
            </div>
        </>
    }
}
