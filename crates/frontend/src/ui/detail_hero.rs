use crate::class;
use yew::prelude::*;

#[derive(Properties, PartialEq)]
pub(crate) struct Props {
    pub(crate) title: AttrValue,
    /// A line under the title, such as the year.
    #[prop_or_default]
    pub(crate) meta: Option<AttrValue>,
    #[prop_or_default]
    pub(crate) backdrop: Option<api::Image>,
}

/// The heading of a show or movie page: its title large over a faded backdrop.
#[function_component]
pub(crate) fn DetailHero(props: &Props) -> Html {
    let backdrop = props.backdrop.as_ref().map(|image| image.proxy_url());

    html! {
        <header class={classes!(class::DETAIL_HERO, sockel::class::CLUSTER, sockel::class::NOWRAP, sockel::class::LOOSE, sockel::class::BOTTOM, backdrop.is_some().then_some(class::HAS_BACKDROP))}>
            if let Some(url) = backdrop {
                <div class={class::DETAIL_HERO_IMAGE} style={format!("background-image: url('{url}')")} />
            }

            <div class={classes!(class::DETAIL_HERO_TEXT, sockel::class::STACK, sockel::class::SNUG)}>
                <h1 class={class::DETAIL_TITLE}>{props.title.clone()}</h1>

                if let Some(meta) = &props.meta {
                    <span class={class::DETAIL_META}>{meta.clone()}</span>
                }
            </div>
        </header>
    }
}
