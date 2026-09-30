//! Drag-and-drop reordering. A [`Reorder`] lays out its children as rows, and
//! each row renders a [`DragHandle`] for its index. Dragging a handle, by mouse
//! or touch, marks where the row would land and reports the move on release;
//! a focused handle moves its row with the arrow keys.

use wasm_bindgen::JsCast as _;
use web_sys::{Element, HtmlElement, KeyboardEvent, PointerEvent};
use yew::prelude::*;

const DRAGGING: &str = "dragging";
const DROP_BEFORE: &str = "drop-before";
const DROP_AFTER: &str = "drop-after";

#[derive(Properties, PartialEq)]
pub(crate) struct HandleProps {
    /// The index of the row this handle moves.
    pub(crate) index: usize,
}

/// The grip a row is dragged by.
#[function_component]
pub(crate) fn DragHandle(props: &HandleProps) -> Html {
    html! {
        <span class="drag-handle" data-index={props.index.to_string()} tabindex="0" role="button" title="Drag to reorder" aria-label="Drag to reorder">
            <span class="icon bars-2" aria-hidden="true" />
        </span>
    }
}

#[derive(Properties, PartialEq)]
pub(crate) struct Props {
    /// A row moved from the first index to the second.
    pub(crate) on_move: Callback<(usize, usize)>,
    #[prop_or_default]
    pub(crate) class: Classes,
    pub(crate) children: Children,
}

pub(crate) enum Msg {
    Down(PointerEvent),
    Move(PointerEvent),
    Up(PointerEvent),
    Cancel(PointerEvent),
    Key(KeyboardEvent),
}

struct Drag {
    from: usize,
    /// Where the row would be inserted, in `0..=rows`.
    slot: usize,
}

pub(crate) struct Reorder {
    list: NodeRef,
    drag: Option<Drag>,
    /// The handle to focus after a keyboard move re-renders the rows.
    focus: Option<usize>,
}

impl Component for Reorder {
    type Message = Msg;
    type Properties = Props;

    fn create(_: &Context<Self>) -> Self {
        Self {
            list: NodeRef::default(),
            drag: None,
            focus: None,
        }
    }

    fn update(&mut self, ctx: &Context<Self>, msg: Self::Message) -> bool {
        let Some(list) = self.list.cast::<Element>() else {
            return false;
        };

        match msg {
            Msg::Down(e) => {
                let Some(from) = handle_index(e.target()) else {
                    return false;
                };

                if e.button() != 0 {
                    return false;
                }

                e.prevent_default();
                _ = list.set_pointer_capture(e.pointer_id());
                self.drag = Some(Drag { from, slot: from });
                mark(&list, Some(from), None);
            }
            Msg::Move(e) => {
                let Some(drag) = &mut self.drag else {
                    return false;
                };

                drag.slot = slot_at(&list, e.client_y() as f64);
                mark(&list, Some(drag.from), Some(drag.slot));
            }
            Msg::Up(e) => {
                let Some(drag) = self.drag.take() else {
                    return false;
                };

                _ = list.release_pointer_capture(e.pointer_id());
                mark(&list, None, None);

                // Taking the row out shifts every later slot up by one.
                let to = if drag.slot > drag.from {
                    drag.slot - 1
                } else {
                    drag.slot
                };

                if to != drag.from {
                    ctx.props().on_move.emit((drag.from, to));
                }
            }
            Msg::Cancel(e) => {
                if self.drag.take().is_some() {
                    _ = list.release_pointer_capture(e.pointer_id());
                    mark(&list, None, None);
                }
            }
            Msg::Key(e) => {
                let Some(from) = handle_index(e.target()) else {
                    return false;
                };

                let rows = list.children().length() as usize;

                let to = match e.key().as_str() {
                    "ArrowUp" if from > 0 => from - 1,
                    "ArrowDown" if from + 1 < rows => from + 1,
                    _ => return false,
                };

                e.prevent_default();
                self.focus = Some(to);
                ctx.props().on_move.emit((from, to));
            }
        }

        false
    }

    fn rendered(&mut self, _: &Context<Self>, _: bool) {
        let (Some(index), Some(list)) = (self.focus.take(), self.list.cast::<Element>()) else {
            return;
        };

        let selector = format!(".drag-handle[data-index='{index}']");

        if let Ok(Some(handle)) = list.query_selector(&selector)
            && let Ok(handle) = handle.dyn_into::<HtmlElement>()
        {
            _ = handle.focus();
        }
    }

    fn view(&self, ctx: &Context<Self>) -> Html {
        let link = ctx.link();
        let props = ctx.props();

        html! {
            <div ref={self.list.clone()} class={classes!("reorder", props.class.clone())}
                onpointerdown={link.callback(Msg::Down)}
                onpointermove={link.callback(Msg::Move)}
                onpointerup={link.callback(Msg::Up)}
                onpointercancel={link.callback(Msg::Cancel)}
                onkeydown={link.callback(Msg::Key)}>
                { for props.children.iter() }
            </div>
        }
    }
}

/// The row index of the drag handle an event started on, if any.
fn handle_index(target: Option<web_sys::EventTarget>) -> Option<usize> {
    let element = target?.dyn_into::<Element>().ok()?;
    let handle = element.closest(".drag-handle").ok()??;
    handle.get_attribute("data-index")?.parse().ok()
}

/// The slot a pointer at `y` points at: before the first row whose middle is
/// below it, or after the last row.
fn slot_at(list: &Element, y: f64) -> usize {
    let rows = list.children();

    for i in 0..rows.length() {
        if let Some(row) = rows.item(i) {
            let rect = row.get_bounding_client_rect();

            if y < rect.top() + rect.height() / 2.0 {
                return i as usize;
            }
        }
    }

    rows.length() as usize
}

/// Mark the dragged row and the insertion point, clearing any earlier marks.
fn mark(list: &Element, dragging: Option<usize>, slot: Option<usize>) {
    let rows = list.children();
    let count = rows.length() as usize;

    for i in 0..count {
        let Some(row) = rows.item(i as u32) else {
            continue;
        };

        let classes = row.class_list();
        _ = classes.toggle_with_force(DRAGGING, dragging == Some(i));
        _ = classes.toggle_with_force(DROP_BEFORE, slot == Some(i));
        _ = classes.toggle_with_force(DROP_AFTER, slot == Some(count) && i + 1 == count);
    }
}
