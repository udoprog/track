//! A reusable anchored popover shell. Hosts supply an open/position
//! [`ContextMenuState`] and the body content; this component renders a
//! full-screen click "catcher" plus a fixed popover positioned just below the
//! trigger and clamped to the viewport, revealed only once measured to avoid a
//! first-frame flash. Used by [`MarkTimeMenu`](crate::ui::MarkTimeMenu) and the
//! dashboard's skip-episode confirm.
//!
//! It holds keyboard focus while open like a modal does: focus moves to its
//! first control, Tab stays inside, and Escape closes it and hands focus back.
//! A body with a `role="menu"` list gets arrow-key movement between its
//! `menuitem`s.

use web_sys::{HtmlElement, KeyboardEvent, MouseEvent};
use yew::prelude::*;

use crate::background::Background;
use crate::error::{CustomContext, Error, Message};
use crate::ui::focus;

#[derive(Properties, PartialEq)]
pub(crate) struct Props {
    #[prop_or_default]
    pub(crate) icon: Option<AttrValue>,
    #[prop_or_default]
    pub(crate) prompt: Option<AttrValue>,
    #[prop_or_default]
    pub(crate) label: Option<AttrValue>,
    /// The trigger element to position against. The host wires this `NodeRef`
    /// to the exact element it wants the popover anchored to - measured
    /// directly, so positioning never depends on event targets (unreliable
    /// under Yew's delegated dispatch) or DOM selectors.
    pub(crate) anchor: NodeRef,
    /// Invoked when the backdrop is clicked, so the host can close the menu.
    pub(crate) on_close: Callback<()>,
    /// Popover content.
    pub(crate) children: Children,
}

pub(crate) struct ContextMenu {
    /// The popover element, measured after render to clamp it on-screen.
    menu: NodeRef,
    /// The popover's `(width, height)` at its last placement. Compared after each
    /// render to decide whether to re-place: positioning is driven entirely by
    /// the body's own size, so a change here is the only thing that can shift it.
    placed: Option<(f64, f64)>,
    /// Whether the popover opened below its trigger. Decided once, so content
    /// growing later (e.g. a picker expanding) never flips it over the trigger.
    below: Option<bool>,
    /// Renders waited for the anchor to be attached. A trigger rendered by a
    /// child component can attach its ref after this menu's first render.
    waited: u8,
    /// The background context, so we can report errors to the user.
    background: Background,
    /// What had focus before the popover opened, to give it back on close.
    opener: Option<HtmlElement>,
    /// Whether focus has been moved into the popover yet.
    focused: bool,
}

impl ContextMenu {
    /// Position the popover just below the trigger, clamped to the viewport so it
    /// stays on-screen without flipping to the far side. We measure the *actual*
    /// rendered menu rather than guessing its size, so wide layouts are placed
    /// correctly. Mobile presents it full-page via CSS, which ignores these
    /// variables. The coordinates are written into CSS custom properties, and the
    /// menu is revealed only once positioned to avoid a first-frame flash in the
    /// top-left corner.
    fn place(&mut self, anchor: &HtmlElement) -> Result<(), Error> {
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

        // Prefer below the trigger, unless it only fits above. Once decided the
        // side sticks, and the menu scrolls within the room on that side.
        let below = *self.below.get_or_insert_with(|| {
            trig.bottom() + m.height() + GAP + MARGIN <= vh
                || trig.top() - GAP - MARGIN < vh - trig.bottom() - GAP - MARGIN
        });

        let (top, room) = if below {
            let top = trig.bottom() + GAP;
            (top, vh - top - MARGIN)
        } else {
            let room = trig.top() - GAP - MARGIN;
            ((trig.top() - GAP - m.height()).max(MARGIN), room)
        };

        style
            .set_property("--cm-max-height", &format!("{}px", room.max(0.0)))
            .context(Message::PositioningMenu)?;

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

    fn create(ctx: &Context<Self>) -> Self {
        let (background, _) = ctx
            .link()
            .context::<Background>(Callback::noop())
            .expect("background context");

        Self {
            menu: NodeRef::default(),
            placed: None,
            below: None,
            waited: 0,
            background,
            opener: focus::active(),
            focused: false,
        }
    }

    fn destroy(&mut self, _: &Context<Self>) {
        focus::restore(self.opener.take());
    }

    fn rendered(&mut self, ctx: &Context<Self>, _first_render: bool) {
        let Some(menu) = self.menu.cast::<HtmlElement>() else {
            return;
        };

        let Some(anchor) = ctx.props().anchor.cast::<HtmlElement>() else {
            if self.waited < 3 {
                self.waited += 1;
                ctx.link().send_message(());
            }

            return;
        };

        let rect = menu.get_bounding_client_rect();
        let size = (rect.width(), rect.height());

        // Re-place only when the body's size changes - on open, or when its
        // content grows/shrinks. Position is what `place` itself writes, so
        // comparing size (not position) avoids a re-place feedback loop while
        // still catching every reshape. Steady re-renders such as clock-drag
        // frames keep the same size and are skipped after this cheap read.
        if self.placed == Some(size) {
            return;
        }

        self.placed = Some(size);

        if let Err(e) = self.place(&anchor) {
            self.background.error(e);
        }

        if !self.focused {
            self.focused = true;
            focus::focus_first(&menu, None);
        }
    }

    fn view(&self, ctx: &Context<Self>) -> Html {
        let props = ctx.props();
        let on_close = props.on_close.clone();

        let onkeydown = {
            let menu = self.menu.clone();
            let on_close = props.on_close.clone();

            Callback::from(move |e: KeyboardEvent| {
                if e.key() == "Escape" {
                    e.stop_propagation();
                    on_close.emit(());
                    return;
                }

                if let Some(menu) = menu.cast::<HtmlElement>()
                    && !focus::move_in_menu(&e, &menu)
                {
                    focus::trap_tab(&e, &menu);
                }
            })
        };

        // A prompt makes the popover a small dialog named by it; a bare body
        // (such as a menu) names itself.
        let (role, aria_label) = match (&props.prompt, &props.label) {
            (Some(prompt), Some(label)) => (Some("dialog"), Some(format!("{prompt} {label}"))),
            (Some(prompt), None) => (Some("dialog"), Some(prompt.to_string())),
            _ => (None, None),
        };

        html! {
            <div class="context-catcher" onclick={Callback::from(move |_| on_close.emit(()))}>
                <div class="context-menu" ref={self.menu.clone()} {role} aria-label={aria_label} tabindex="-1" {onkeydown} onclick={Callback::from(|e: MouseEvent| e.stop_propagation())}>
                    if let Some(prompt) = &props.prompt {
                        <div class="context-menu-header">
                            if let Some(ref icon) = props.icon {
                                <span class="item-inline" aria-hidden="true">
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
