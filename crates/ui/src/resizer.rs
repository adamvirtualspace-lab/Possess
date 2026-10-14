//! Dragging the sidebar edge and the editor/preview split.
//!
//! Both positions are CSS custom properties on `<html>`, so the layout follows
//! them without anything else knowing a drag happened, and both are remembered
//! across reloads.
//!
//! The split handle only exists in SimpleMDE's side-by-side mode, which can be
//! reached from the toolbar, from F9, or from SimpleMDE's own internal
//! coupling. Rather than hook each route, a `MutationObserver` watches the one
//! class CodeMirror flips on its wrapper — which catches all of them.

use crate::editor::editor;
use crate::store;
use std::cell::Cell;
use std::rc::Rc;
use wasm_bindgen::prelude::*;
use wasm_bindgen::JsCast;

const SIDEBAR_KEY: &str = "possess:sidebar-width";
const SPLIT_KEY: &str = "possess:split-pos";

const SIDEBAR_MIN: f64 = 180.0;
const SIDEBAR_MAX: f64 = 560.0;
const SPLIT_MIN: f64 = 15.0;
const SPLIT_MAX: f64 = 85.0;

fn document() -> Option<web_sys::Document> {
    web_sys::window()?.document()
}

fn root_style() -> Option<web_sys::CssStyleDeclaration> {
    let root = document()?.document_element()?;
    root.dyn_into::<web_sys::HtmlElement>().ok().map(|el| el.style())
}

fn by_id(id: &str) -> Option<web_sys::HtmlElement> {
    document()?
        .get_element_by_id(id)?
        .dyn_into::<web_sys::HtmlElement>()
        .ok()
}

fn set_sidebar_width(px: f64) -> f64 {
    let width = px.clamp(SIDEBAR_MIN, SIDEBAR_MAX);
    if let Some(style) = root_style() {
        let _ = style.set_property("--sidebar-width", &format!("{width}px"));
    }
    width
}

fn set_split_pos(percent: f64) -> f64 {
    let pos = percent.clamp(SPLIT_MIN, SPLIT_MAX);
    if let Some(style) = root_style() {
        let _ = style.set_property("--split-pos", &format!("{pos}%"));
    }
    pos
}

/// Put both handles back where they were left, and start watching for the
/// editor's own mode changes.
pub fn init() {
    if let Some(width) = store::get_f64(SIDEBAR_KEY) {
        set_sidebar_width(width);
    }
    if let Some(pos) = store::get_f64(SPLIT_KEY) {
        set_split_pos(pos);
    }

    wire_sidebar();
    wire_split();
    observe_editor_modes();
}

/// Ask CodeMirror to re-measure, at most once a frame.
///
/// It caches its own dimensions, so moving the panes leaves the cursor landing
/// in the wrong place until it re-measures — but `refresh` is heavy and
/// pointermove fires far more often than a frame.
fn refresh_editor(queued: &Rc<Cell<bool>>) {
    if queued.replace(true) {
        return;
    }

    let queued = queued.clone();
    let callback = Closure::once_into_js(move || {
        queued.set(false);
        editor().refresh();
    });

    if let Some(window) = web_sys::window() {
        let _ = window.request_animation_frame(callback.unchecked_ref());
    }
}

/// Pointer plumbing shared by both handles.
///
/// Pointer capture keeps events coming to the handle even when the cursor
/// outruns it mid-drag, which is what stops a fast drag from dropping the
/// handle somewhere behind the pointer.
///
/// The listeners are attached once and gated on a flag rather than added and
/// removed per drag: with the pointer captured, the handle is the only thing
/// receiving these events anyway, and nothing has to hold a reference to itself
/// in order to unregister later.
fn on_drag(
    handle: web_sys::HtmlElement,
    on_move: impl Fn(&web_sys::PointerEvent) + 'static,
    on_end: impl Fn() + 'static,
) {
    let dragging = Rc::new(Cell::new(false));

    let down = {
        let handle = handle.clone();
        let dragging = dragging.clone();
        listener(move |event: web_sys::PointerEvent| {
            event.prevent_default();
            dragging.set(true);
            let _ = handle.set_pointer_capture(event.pointer_id());
            let _ = handle.class_list().add_1("dragging");
            if let Some(body) = document().and_then(|d| d.body()) {
                let _ = body.class_list().add_1("is-resizing");
            }
        })
    };

    let moving = {
        let dragging = dragging.clone();
        listener(move |event: web_sys::PointerEvent| {
            if dragging.get() {
                on_move(&event);
            }
        })
    };

    let ending = {
        let handle = handle.clone();
        let dragging = dragging.clone();
        listener(move |event: web_sys::PointerEvent| {
            if !dragging.replace(false) {
                return;
            }
            let _ = handle.release_pointer_capture(event.pointer_id());
            let _ = handle.class_list().remove_1("dragging");
            if let Some(body) = document().and_then(|d| d.body()) {
                let _ = body.class_list().remove_1("is-resizing");
            }
            on_end();
        })
    };

    let target: &web_sys::EventTarget = handle.as_ref();
    let _ = target.add_event_listener_with_callback("pointerdown", down.as_ref().unchecked_ref());
    let _ = target.add_event_listener_with_callback("pointermove", moving.as_ref().unchecked_ref());
    for name in ["pointerup", "pointercancel"] {
        let _ = target.add_event_listener_with_callback(name, ending.as_ref().unchecked_ref());
    }

    // Live for the life of the page, like the handles they are attached to.
    down.forget();
    moving.forget();
    ending.forget();
}

/// Wrap a pointer handler for the DOM.
fn listener(
    f: impl FnMut(web_sys::PointerEvent) + 'static,
) -> Closure<dyn FnMut(web_sys::PointerEvent)> {
    Closure::wrap(Box::new(f) as Box<dyn FnMut(web_sys::PointerEvent)>)
}

fn wire_sidebar() {
    let (Some(handle), Some(app)) = (by_id("sidebar-resizer"), by_id("app")) else {
        return;
    };

    let queued = Rc::new(Cell::new(false));
    let width = Rc::new(Cell::new(None::<f64>));

    let on_move = {
        let width = width.clone();
        let queued = queued.clone();
        move |event: &web_sys::PointerEvent| {
            let left = app.get_bounding_client_rect().left();
            width.set(Some(set_sidebar_width(event.client_x() as f64 - left)));
            refresh_editor(&queued);
        }
    };

    let on_end = move || {
        if let Some(width) = width.get() {
            store::set_f64(SIDEBAR_KEY, width);
        }
    };

    on_drag(handle, on_move, on_end);
}

fn wire_split() {
    let Some(handle) = by_id("split-resizer") else { return };
    let Some(viewport) = document()
        .and_then(|d| d.query_selector(".viewport").ok().flatten())
        .and_then(|el| el.dyn_into::<web_sys::HtmlElement>().ok())
    else {
        return;
    };

    let queued = Rc::new(Cell::new(false));
    let pos = Rc::new(Cell::new(None::<f64>));

    let on_move = {
        let pos = pos.clone();
        let queued = queued.clone();
        let handle = handle.clone();
        move |event: &web_sys::PointerEvent| {
            // In fullscreen the panes are fixed to the window rather than to
            // .viewport, so the percentage is measured against the window.
            let (left, width) = if handle.class_list().contains("is-fullscreen") {
                let width = web_sys::window()
                    .and_then(|w| w.inner_width().ok())
                    .and_then(|v| v.as_f64())
                    .unwrap_or(1.0);
                (0.0, width)
            } else {
                let rect = viewport.get_bounding_client_rect();
                (rect.left(), rect.width())
            };

            if width > 0.0 {
                let percent = ((event.client_x() as f64 - left) / width) * 100.0;
                pos.set(Some(set_split_pos(percent)));
                refresh_editor(&queued);
            }
        }
    };

    let on_end = move || {
        if let Some(pos) = pos.get() {
            store::set_f64(SPLIT_KEY, pos);
        }
    };

    on_drag(handle, on_move, on_end);
}

/// Mirror the editor's own modes onto the layout.
///
/// SimpleMDE flips `CodeMirror-sided` and `CodeMirror-fullscreen` on its
/// wrapper. Watching that one attribute catches every route into those modes
/// without hooking each entry point separately.
fn observe_editor_modes() {
    let wrapper = editor().wrapper.clone();
    let (Some(viewport), Some(handle)) = (
        document()
            .and_then(|d| d.query_selector(".viewport").ok().flatten())
            .and_then(|el| el.dyn_into::<web_sys::HtmlElement>().ok()),
        by_id("split-resizer"),
    ) else {
        return;
    };

    let apply = {
        let wrapper = wrapper.clone();
        move || {
            let classes = wrapper.class_list();
            let _ = viewport
                .class_list()
                .toggle_with_force("is-split", classes.contains("CodeMirror-sided"));
            let _ = handle
                .class_list()
                .toggle_with_force("is-fullscreen", classes.contains("CodeMirror-fullscreen"));
        }
    };
    apply();

    let callback = Closure::wrap(Box::new(move |_: js_sys::Array, _: JsValue| apply())
        as Box<dyn FnMut(js_sys::Array, JsValue)>);

    if let Ok(observer) = web_sys::MutationObserver::new(callback.as_ref().unchecked_ref()) {
        let options = web_sys::MutationObserverInit::new();
        options.set_attributes(true);
        options.set_attribute_filter(&js_sys::Array::of1(&"class".into()));
        let _ = observer.observe_with_options(&wrapper, &options);
    }
    callback.forget();
}
