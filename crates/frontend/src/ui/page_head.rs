use yew::prelude::*;

#[derive(Properties, PartialEq)]
pub(crate) struct Props {
    /// The page's title, the one `h1` on the screen.
    pub(crate) title: AttrValue,
    /// Keep the title for assistive technology only, for pages whose content
    /// speaks for itself. No actions, crumb or sub are drawn then.
    #[prop_or_default]
    pub(crate) hidden: bool,
    /// A line above the head, such as where the page sits.
    #[prop_or_default]
    pub(crate) crumb: Option<Html>,
    /// A line under the head, such as a department or year.
    #[prop_or_default]
    pub(crate) sub: Option<AttrValue>,
    /// The actions, placed at the end of the title's row.
    #[prop_or_default]
    pub(crate) children: Children,
}

/// The class of the page's `h1`.
fn title_class(hidden: bool) -> &'static str {
    if hidden {
        "visually-hidden"
    } else {
        "page-head-title"
    }
}

/// A page's heading: its title with actions at the end, an optional crumb above
/// and sub line below. Every screen's head is drawn here so they look alike.
#[function_component]
pub(crate) fn PageHead(props: &Props) -> Html {
    if props.hidden {
        return html! { <h1 class={title_class(true)}>{props.title.clone()}</h1> };
    }

    html! {
        <div class="plinth-stack plinth-tight">
            if let Some(crumb) = &props.crumb {
                <div class="text-muted">{crumb.clone()}</div>
            }

            <div class="page-head">
                <h1 class={title_class(false)}>{props.title.clone()}</h1>

                if !props.children.is_empty() {
                    <div class="plinth-push-end plinth-cluster plinth-nowrap plinth-tight">{props.children.clone()}</div>
                }
            </div>

            if let Some(sub) = &props.sub {
                <div class="text-muted">{sub.clone()}</div>
            }
        </div>
    }
}

#[cfg(test)]
mod tests {
    use super::title_class;

    #[test]
    fn title_class_follows_visibility() {
        assert_eq!(title_class(false), "page-head-title");
        assert_eq!(title_class(true), "visually-hidden");
    }
}
