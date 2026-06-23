//! A reusable anchored popover shell. Hosts supply an open/position
//! [`ContextMenuState`] and the body content; this component renders a
//! full-screen click "catcher" plus a fixed popover positioned just below the
//! trigger and clamped to the viewport, revealed only once measured to avoid a
//! first-frame flash. Used by [`MarkTimeMenu`](crate::ui::MarkTimeMenu) and the
//! dashboard's skip-episode confirm.

use web_sys::{HtmlElement, MouseEvent};
use yew::prelude::*;

use crate::error::{CustomContext, Error, Message};

#[derive(Properties, PartialEq)]
pub(crate) struct Props {
    #[prop_or_default]
    pub(crate) icon: Option<AttrValue>,
    #[prop_or_default]
    pub(crate) prompt: Option<AttrValue>,
    #[prop_or_default]
    pub(crate) label: Option<AttrValue>,
    /// The trigger element to position against. The host wires this `NodeRef`
    /// to the exact element it wants the popover anchored to — measured
    /// directly, so positioning never depends on event targets (unreliable
    /// under Yew's delegated dispatch) or DOM selectors.
    pub(crate) anchor: NodeRef,
    /// Invoked when the backdrop is clicked, so the host can close the menu.
    pub(crate) on_close: Callback<()>,
    /// Popover content.
    pub(crate) children: Children,
    /// Surfaces a positioning failure to the host's error handler.
    pub(crate) onerror: Callback<Error>,
}

pub(crate) struct ContextMenu {
    /// The popover element, measured after render to clamp it on-screen.
    menu: NodeRef,
    /// The popover's `(width, height)` at its last placement. Compared after each
    /// render to decide whether to re-place: positioning is driven entirely by
    /// the body's own size, so a change here is the only thing that can shift it.
    placed: Option<(f64, f64)>,
}

impl ContextMenu {
    /// Position the popover just below the trigger, clamped to the viewport so it
    /// stays on-screen without flipping to the far side. We measure the *actual*
    /// rendered menu rather than guessing its size, so wide layouts are placed
    /// correctly. Mobile presents it full-page via CSS, which ignores these
    /// variables. The coordinates are written into CSS custom properties, and the
    /// menu is revealed only once positioned to avoid a first-frame flash in the
    /// top-left corner.
    fn place(&self, anchor: &HtmlElement) -> Result<(), Error> {
        let menu = self
            .menu
            .cast::<HtmlElement>()
            .context(Message::PositioningMenu)?;

        let style = menu.style();

        style
            .set_property("visibility", "visible")
            .context(Message::PositioningMenu)?;

        let win = web_sys::window().context(Message::MissingWindow)?;

        let vw = win
            .inner_width()
            .ok()
            .and_then(|v| v.as_f64())
            .context(Message::ReadingViewport)?;

        let vh = win
            .inner_height()
            .ok()
            .and_then(|v| v.as_f64())
            .context(Message::ReadingViewport)?;

        let trig = anchor.get_bounding_client_rect();
        let m = menu.get_bounding_client_rect();

        const GAP: f64 = 4.0;
        const MARGIN: f64 = 8.0;

        // Line up with the trigger's left edge, then slide left only as much as
        // needed to keep the whole menu on-screen.
        let left = trig.left().min(vw - m.width() - MARGIN).max(MARGIN);

        // Prefer below the trigger; flip above if it would overflow the bottom.
        let top = if trig.bottom() + m.height() + GAP + MARGIN <= vh {
            trig.bottom() + GAP
        } else if trig.top() - m.height() - GAP >= MARGIN {
            trig.top() - m.height() - GAP
        } else {
            (vh - m.height() - MARGIN).max(MARGIN)
        };

        style
            .set_property("--cm-left", &format!("{left}px"))
            .context(Message::PositioningMenu)?;

        style
            .set_property("--cm-top", &format!("{top}px"))
            .context(Message::PositioningMenu)?;

        Ok(())
    }
}

impl Component for ContextMenu {
    type Message = ();
    type Properties = Props;

    fn create(_ctx: &Context<Self>) -> Self {
        Self {
            menu: NodeRef::default(),
            placed: None,
        }
    }

    fn rendered(&mut self, ctx: &Context<Self>, _first_render: bool) {
        let Some(menu) = self.menu.cast::<HtmlElement>() else {
            return;
        };

        let Some(anchor) = ctx.props().anchor.cast::<HtmlElement>() else {
            return;
        };

        let rect = menu.get_bounding_client_rect();
        let size = (rect.width(), rect.height());

        // Re-place only when the body's size changes — on open, or when its
        // content grows/shrinks. Position is what `place` itself writes, so
        // comparing size (not position) avoids a re-place feedback loop while
        // still catching every reshape. Steady re-renders such as clock-drag
        // frames keep the same size and are skipped after this cheap read.
        if self.placed == Some(size) {
            return;
        }

        self.placed = Some(size);

        if let Err(e) = self.place(&anchor) {
            ctx.props().onerror.emit(e);
        }
    }

    fn view(&self, ctx: &Context<Self>) -> Html {
        let props = ctx.props();
        let on_close = props.on_close.clone();

        html! {
            <div class="context-catcher" onclick={Callback::from(move |_| on_close.emit(()))}>
                <div class="context-menu" ref={self.menu.clone()} onclick={Callback::from(|e: MouseEvent| e.stop_propagation())}>
                    if let Some(prompt) = &props.prompt {
                        <div class="context-menu-header">
                            if let Some(ref icon) = props.icon {
                                <span class="item-inline">
                                    <span class={classes!("icon", icon)} />
                                </span>
                            }

                            <span>{prompt}</span>

                            if let Some(ref label) = props.label {
                                <span>
                                    <span class="focus">{label}</span>
                                    <span>{"?"}</span>
                                </span>
                            }
                        </div>
                    }

                    { for props.children.iter() }
                </div>
            </div>
        }
    }
}
