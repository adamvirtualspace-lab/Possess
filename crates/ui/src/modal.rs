//! What the two modals have in common.
//!
//! Not a wrapper component: the markup differs enough between them that sharing
//! it would mean a prop per difference. What is genuinely shared is the
//! behaviour — Escape closes, and a click on the dimmed backdrop closes, but a
//! click inside the dialog does not.

use leptos::prelude::*;
use wasm_bindgen::JsCast;

/// Close `open` when Escape is pressed while it is set.
///
/// Registered once on the document rather than on the dialog, because a modal
/// that has not been opened yet has no element to listen on.
pub fn close_on_escape(open: RwSignal<bool>) {
    let handler = wasm_bindgen::closure::Closure::wrap(Box::new(
        move |event: web_sys::KeyboardEvent| {
            if event.key() == "Escape" && open.get_untracked() {
                open.set(false);
            }
        },
    ) as Box<dyn FnMut(_)>);

    if let Some(document) = web_sys::window().and_then(|w| w.document()) {
        let _ = document
            .add_event_listener_with_callback("keydown", handler.as_ref().unchecked_ref());
    }
    handler.forget();
}

/// A backdrop click handler that ignores clicks inside the dialog.
///
/// The event target is the backdrop itself only when the click missed every
/// child, which is what separates "clicked away" from "clicked the dialog".
pub fn close_on_backdrop(open: RwSignal<bool>) -> impl Fn(web_sys::MouseEvent) + Clone {
    move |event: web_sys::MouseEvent| {
        let on_backdrop = event
            .target()
            .and_then(|t| t.dyn_into::<web_sys::Element>().ok())
            .map(|el| el.class_list().contains("modal-backdrop"))
            .unwrap_or(false);

        if on_backdrop {
            open.set(false);
        }
    }
}
