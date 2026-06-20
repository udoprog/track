use std::rc::Rc;

use gloo::events::EventListener;
use wasm_bindgen::JsCast as _;
use web_sys::{Element, HtmlElement, PointerEvent, WheelEvent};
use yew::prelude::*;

use crate::error::{CustomContext, Error, Message};

/// A single navigable entry in the outline. `code` is the `id` of the rendered
/// episode element (see [`api::Episode::code`]) so the outline can scroll it
/// into view; `label` is shown on the sampled marker.
#[derive(Clone, PartialEq)]
pub(super) struct OutlineEntry {
    pub(super) code: AttrValue,
    pub(super) label: AttrValue,
}

/// Context value handed to consumers (e.g. the show detail view) so they can
/// populate the shared outline rail owned by [`crate::app`].
///
/// Call [`OutlineControl::attach`] to begin showing entries; the returned
/// [`OutlineHandle`] keeps the outline alive and tears it down when dropped.
#[derive(Clone, PartialEq)]
pub(super) struct OutlineControl {
    set: Callback<Option<Rc<[OutlineEntry]>>>,
}

impl OutlineControl {
    pub(super) fn new(set: Callback<Option<Rc<[OutlineEntry]>>>) -> Self {
        Self { set }
    }

    /// Show `entries` in the outline. The returned handle clears the outline
    /// when it is dropped.
    pub(super) fn attach(&self, entries: Rc<[OutlineEntry]>) -> OutlineHandle {
        self.set.emit(Some(entries));
        OutlineHandle {
            set: self.set.clone(),
        }
    }
}

/// RAII handle that owns the current outline contents. Dropping it removes the
/// outline (e.g. when the owning component is destroyed or navigated away).
pub(super) struct OutlineHandle {
    set: Callback<Option<Rc<[OutlineEntry]>>>,
}

impl OutlineHandle {
    /// Replace the displayed entries, e.g. when the selected season changes.
    pub(super) fn set(&self, entries: Rc<[OutlineEntry]>) {
        self.set.emit(Some(entries));
    }
}

impl Drop for OutlineHandle {
    fn drop(&mut self) {
        self.set.emit(None);
    }
}

/// A sampled outline marker: which episode it jumps to (`code`/`label`) and its
/// vertical position as a percentage of the scrollable content, matching the
/// coordinate space of the highlight band.
#[derive(Clone, PartialEq)]
struct OutlineMark {
    label: AttrValue,
    top: f64,
}

#[derive(Properties, PartialEq)]
pub(super) struct OutlineProps {
    /// The page scroll container the outline reflects and drives.
    pub(super) page: NodeRef,
    /// Entries to show; `None` hides the outline entirely.
    pub(super) entries: Option<Rc<[OutlineEntry]>>,
    /// Surfaces failures to the application, as elsewhere.
    pub(super) onerror: Callback<Option<Error>>,
}

pub(super) enum OutlineMsg {
    /// The page scrolled; reposition the highlight band.
    Scrolled,
    /// The window resized; re-measure and re-sample.
    Resized,
    PointerDown(PointerEvent),
    PointerMove(PointerEvent),
    PointerUp(PointerEvent),
    /// Wheel/scroll over the rail; forwarded to the page scroll.
    Wheel(WheelEvent),
}

/// The shared outline rail: a minimap of the page's scroll content with a
/// highlight band for the current viewport and sampled episode markers.
pub(super) struct Outline {
    /// The rail element; used to map pointer drags onto the page scroll.
    outline: NodeRef,
    /// The moving highlight band, positioned imperatively.
    mark: NodeRef,
    /// One marker per episode, positioned (in page-scroll fractions) to line up
    /// with the highlight band. Measured imperatively after layout.
    marks: Vec<OutlineMark>,
    /// Parallel to `marks`: whether each marker is culled (hidden) because it
    /// would overlap the previous visible one. Computed from real geometry.
    hidden: Vec<bool>,
    /// Reusable scratch buffers for the per-render measure/cull passes, kept so
    /// the work amortizes to zero allocations once warmed up.
    marks_buf: Vec<OutlineMark>,
    hidden_buf: Vec<bool>,
    /// Whether a pointer drag on the rail is in progress.
    dragging: bool,
    _resize: EventListener,
    /// Scroll listener on the page element, attached once it is available.
    _scroll: Option<EventListener>,
}

impl Component for Outline {
    type Message = OutlineMsg;
    type Properties = OutlineProps;

    fn create(ctx: &Context<Self>) -> Self {
        let on_resize = ctx.link().callback(|_| OutlineMsg::Resized);
        let window = web_sys::window().expect("Expected a window");
        let _resize = EventListener::new(&window, "resize", move |_| on_resize.emit(()));

        Self {
            outline: NodeRef::default(),
            mark: NodeRef::default(),
            marks: Vec::new(),
            hidden: Vec::new(),
            marks_buf: Vec::new(),
            hidden_buf: Vec::new(),
            dragging: false,
            _resize,
            _scroll: None,
        }
    }

    fn update(&mut self, ctx: &Context<Self>, msg: Self::Message) -> bool {
        match self.try_update(ctx, msg) {
            Ok(render) => render,
            Err(e) => {
                ctx.props().onerror.emit(Some(e));
                false
            }
        }
    }

    fn rendered(&mut self, ctx: &Context<Self>, _first_render: bool) {
        // Attach the scroll listener once the page element exists. The page is
        // owned by `App` (a sibling), so we listen on the DOM node directly.
        if self._scroll.is_none()
            && let Some(page) = ctx.props().page.cast::<Element>()
        {
            let link = ctx.link().clone();
            self._scroll = Some(EventListener::new(&page, "scroll", move |_| {
                link.send_message(OutlineMsg::Scrolled);
            }));
        }

        // Marker positions depend on post-layout geometry, so they are measured
        // here. Stabilize the positions first; once they settle, cull markers
        // that would overlap using their real rendered geometry. Each step only
        // re-renders when something actually changed, so this converges. The
        // measure/cull results are built into reused buffers and swapped in on
        // change, so steady-state passes allocate nothing.
        let mut marks = std::mem::take(&mut self.marks_buf);
        Self::measure_into(ctx, &mut marks);

        if marks != self.marks {
            std::mem::swap(&mut self.marks, &mut marks);
            ctx.link().send_message(OutlineMsg::Resized);
        } else {
            let mut hidden = std::mem::take(&mut self.hidden_buf);
            self.cull_into(&mut hidden);

            if hidden != self.hidden {
                std::mem::swap(&mut self.hidden, &mut hidden);
                ctx.link().send_message(OutlineMsg::Resized);
            }

            self.hidden_buf = hidden;
        }

        self.marks_buf = marks;

        // The band's geometry depends on the page's scroll metrics, which shift
        // on resize and as content reflows — refresh it after every render.
        if let Err(e) = self.update_mark(ctx) {
            ctx.props().onerror.emit(Some(e));
        }
    }

    fn view(&self, ctx: &Context<Self>) -> Html {
        let link = ctx.link();

        html! {
            <div id="outline"
                ref={self.outline.clone()}
                class={classes!(ctx.props().entries.is_some().then_some("visible"))}
                onpointerdown={link.callback(OutlineMsg::PointerDown)}
                onpointermove={link.callback(OutlineMsg::PointerMove)}
                onpointerup={link.callback(OutlineMsg::PointerUp)}
                onwheel={link.callback(OutlineMsg::Wheel)}>
                <div id="outline-mark" ref={self.mark.clone()} />
                { for self.marks.iter().enumerate().map(|(i, mark)| {
                    let hidden = self.hidden.get(i).copied().unwrap_or(false);

                    html! {
                        <div class={classes!("outline-sample", hidden.then_some("hidden"))}
                            style={format!("top: {}%;", mark.top)}
                            title={mark.label.clone()}>
                            {mark.label.clone()}
                        </div>
                    }
                }) }
            </div>
        }
    }
}

impl Outline {
    fn try_update(&mut self, ctx: &Context<Self>, msg: OutlineMsg) -> Result<bool, Error> {
        match msg {
            OutlineMsg::Scrolled => {
                self.update_mark(ctx)?;
                Ok(false)
            }
            OutlineMsg::Resized => Ok(true),
            OutlineMsg::PointerDown(e) => {
                // Only the primary button (left mouse / touch / pen) drags the
                // rail; ignore right- and middle-clicks.
                if e.button() != 0 {
                    return Ok(false);
                }

                if let Some(outline) = self.outline.cast::<Element>() {
                    e.prevent_default();
                    outline
                        .set_pointer_capture(e.pointer_id())
                        .context(Message::CapturingOutlinePointer)?;
                    self.dragging = true;
                    self.scroll_to_pointer(ctx, &e);
                }

                Ok(false)
            }
            OutlineMsg::PointerMove(e) => {
                if self.dragging {
                    self.scroll_to_pointer(ctx, &e);
                }

                Ok(false)
            }
            OutlineMsg::PointerUp(e) => {
                if self.dragging {
                    if let Some(outline) = self.outline.cast::<Element>() {
                        outline
                            .release_pointer_capture(e.pointer_id())
                            .context(Message::ReleasingOutlinePointer)?;
                    }

                    self.dragging = false;
                }

                Ok(false)
            }
            OutlineMsg::Wheel(e) => {
                if let Some(page) = ctx.props().page.cast::<Element>() {
                    e.prevent_default();

                    // Normalize the delta to pixels regardless of the wheel's
                    // delta mode (lines/pages), then apply it to the page scroll.
                    let delta = match e.delta_mode() {
                        WheelEvent::DOM_DELTA_LINE => e.delta_y() * 16.0,
                        WheelEvent::DOM_DELTA_PAGE => e.delta_y() * page.client_height() as f64,
                        _ => e.delta_y(),
                    };

                    page.set_scroll_top((page.scroll_top() as f64 + delta) as i32);
                }

                Ok(false)
            }
        }
    }

    /// Build a marker for every episode into `out`, positioned by its `offsetTop`
    /// within the page so it lines up with the highlight band. Overlapping
    /// markers are later hidden by [`Outline::cull_into`].
    fn measure_into(ctx: &Context<Self>, out: &mut Vec<OutlineMark>) {
        out.clear();

        let Some(entries) = &ctx.props().entries else {
            return;
        };

        let Some(page) = ctx.props().page.cast::<Element>() else {
            return;
        };

        let scroll_height = page.scroll_height() as f64;

        if entries.is_empty() || scroll_height <= 0.0 {
            return;
        }

        let Some(document) = web_sys::window().and_then(|w| w.document()) else {
            return;
        };

        for entry in entries.iter() {
            let Some(el) = document
                .get_element_by_id(&entry.code)
                .and_then(|el| el.dyn_into::<HtmlElement>().ok())
            else {
                continue;
            };

            // Round to keep equality stable across identical layouts (avoids a
            // re-render loop from float jitter).
            let top = ((offset_within_page(&el) / scroll_height * 100.0) * 1000.0).round() / 1000.0;

            out.push(OutlineMark {
                label: entry.label.clone(),
                top,
            });
        }
    }

    /// Decide which markers to hide so they do not overlap, reading their actual
    /// rendered geometry. Walking top to bottom, a marker is kept when it starts
    /// at or below the bottom of the last kept marker; otherwise it is hidden.
    /// Hidden markers keep their layout (`visibility: hidden`), so re-running on
    /// the result is stable.
    fn cull_into(&self, out: &mut Vec<bool>) {
        out.clear();

        let Some(outline) = self.outline.cast::<Element>() else {
            return;
        };

        let samples = outline.get_elements_by_class_name("outline-sample");
        let rail_top = outline.get_bounding_client_rect().top();

        let mut last_bottom = f64::NEG_INFINITY;

        for i in 0..samples.length() {
            let Some(sample) = samples.item(i) else {
                out.push(false);
                continue;
            };

            let rect = sample.get_bounding_client_rect();
            let top = rect.top() - rail_top;

            if top >= last_bottom {
                out.push(false);
                last_bottom = top + rect.height();
            } else {
                out.push(true);
            }
        }
    }

    /// Position the highlight band to reflect the page's current scroll
    /// viewport. Its top and height depend on the page's scroll metrics, which
    /// change on scroll, on resize, and as content reflows.
    fn update_mark(&self, ctx: &Context<Self>) -> Result<(), Error> {
        let (Some(page), Some(mark)) = (
            ctx.props().page.cast::<Element>(),
            self.mark.cast::<Element>(),
        ) else {
            return Ok(());
        };

        let scroll_height = page.scroll_height() as f64;

        if scroll_height <= 0.0 {
            return Ok(());
        }

        let top = page.scroll_top() as f64 / scroll_height * 100.0;
        let height = page.client_height() as f64 / scroll_height * 100.0;

        mark.set_attribute("style", &format!("top: {top}%; height: {height}%;"))
            .context(Message::SetOutlineStyle)?;

        Ok(())
    }

    /// Map the pointer's vertical position over the rail onto the page scroll
    /// offset, so dragging the rail scrolls the content.
    fn scroll_to_pointer(&self, ctx: &Context<Self>, e: &PointerEvent) {
        let (Some(outline), Some(page)) = (
            self.outline.cast::<Element>(),
            ctx.props().page.cast::<Element>(),
        ) else {
            return;
        };

        let rect = outline.get_bounding_client_rect();

        if rect.height() <= 0.0 {
            return;
        }

        let rel = ((e.client_y() as f64 - rect.top()) / rect.height()).clamp(0.0, 1.0);
        let max = (page.scroll_height() - page.client_height()).max(0) as f64;
        page.set_scroll_top((rel * max) as i32);
    }
}

/// Sum an element's `offsetTop` up the offset-parent chain until reaching the
/// `#page` scroll container, giving its vertical offset within the scrollable
/// content (independent of the current scroll position).
fn offset_within_page(el: &HtmlElement) -> f64 {
    let mut offset = el.offset_top() as f64;
    let mut current = el.offset_parent();

    while let Some(parent) = current {
        if parent.id() == "page" {
            break;
        }

        let Ok(parent) = parent.dyn_into::<HtmlElement>() else {
            break;
        };

        offset += parent.offset_top() as f64;
        current = parent.offset_parent();
    }

    offset
}
