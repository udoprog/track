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
        <header class={classes!("detail-hero", backdrop.is_some().then_some("has-backdrop"))}>
            if let Some(url) = backdrop {
                <div class="detail-hero-image" style={format!("background-image: url('{url}')")} />
            }

            <div class="detail-hero-text">
                <h1 class="detail-title">{props.title.clone()}</h1>

                if let Some(meta) = &props.meta {
                    <span class="detail-meta">{meta.clone()}</span>
                }
            </div>
        </header>
    }
}
