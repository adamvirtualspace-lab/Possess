//! The sidebar's right-click menu.
//!
//! Opened by a context-menu event on a tree row, which supplies what was
//! clicked; the menu itself only decides which actions that kind of thing
//! offers, and where the menu fits on screen.

use crate::notes;
use crate::state::AppState;
use leptos::prelude::*;

/// Keeps the menu clear of the window edge.
const EDGE_MARGIN: f64 = 8.0;

/// What was right-clicked.
#[derive(Debug, Clone, PartialEq)]
pub struct Target {
    /// "note" or "folder".
    pub kind: String,
    pub path: String,
    pub name: String,
    pub x: f64,
    pub y: f64,
}

/// Open the menu for a tree row.
pub fn open_for(state: &AppState, event: &web_sys::MouseEvent, kind: &str, path: &str, name: &str) {
    event.prevent_default();
    event.stop_propagation();

    state.context_menu.set(Some(Target {
        kind: kind.to_string(),
        path: path.to_string(),
        name: name.to_string(),
        x: event.client_x() as f64,
        y: event.client_y() as f64,
    }));
}

pub fn close(state: &AppState) {
    if state.context_menu.with_untracked(Option::is_some) {
        state.context_menu.set(None);
    }
}

/// One entry in the menu.
struct Item {
    label: &'static str,
    danger: bool,
    run: Box<dyn Fn(AppState, Target)>,
}

fn items_for(kind: &str) -> Vec<Item> {
    fn spawn<F, Fut>(f: F) -> Box<dyn Fn(AppState, Target)>
    where
        F: Fn(AppState, Target) -> Fut + 'static,
        Fut: std::future::Future<Output = ()> + 'static,
    {
        Box::new(move |state, target| {
            let future = f(state, target);
            leptos::task::spawn_local(future);
        })
    }

    if kind == "folder" {
        vec![
            Item {
                label: "New note here",
                danger: false,
                run: spawn(|state, target| async move {
                    notes::create_note(&state, &target.path).await
                }),
            },
            Item {
                label: "New folder here",
                danger: false,
                run: spawn(|state, target| async move {
                    notes::create_folder(&state, &target.path).await
                }),
            },
            Item {
                label: "Rename folder",
                danger: false,
                run: spawn(|state, target| async move {
                    notes::rename(&state, "folder", &target.path, &target.name).await
                }),
            },
            Item {
                label: "Delete folder",
                danger: true,
                run: spawn(|state, target| async move {
                    notes::remove(&state, "folder", &target.path).await
                }),
            },
        ]
    } else {
        vec![
            Item {
                label: "Open",
                danger: false,
                run: spawn(|state, target| async move {
                    if let Err(err) = state.open_and_reveal(&target.path).await {
                        leptos::logging::error!("Could not open note: {err}");
                    }
                }),
            },
            Item {
                label: "Rename note",
                danger: false,
                run: spawn(|state, target| async move {
                    notes::rename(&state, "note", &target.path, &target.name).await
                }),
            },
            Item {
                label: "Delete note",
                danger: true,
                run: spawn(|state, target| async move {
                    notes::remove(&state, "note", &target.path).await
                }),
            },
        ]
    }
}

/// Put the menu where it was asked for, pulled back inside the window when it
/// would otherwise be clipped.
fn place(element: &web_sys::HtmlElement, x: f64, y: f64) {
    let Some(window) = web_sys::window() else { return };

    let viewport_width = window.inner_width().ok().and_then(|v| v.as_f64()).unwrap_or(0.0);
    let viewport_height = window.inner_height().ok().and_then(|v| v.as_f64()).unwrap_or(0.0);
    let max_x = viewport_width - element.offset_width() as f64 - EDGE_MARGIN;
    let max_y = viewport_height - element.offset_height() as f64 - EDGE_MARGIN;

    let _ = element.style().set_property("left", &format!("{}px", x.min(max_x)));
    let _ = element.style().set_property("top", &format!("{}px", y.min(max_y)));
}

#[component]
pub fn ContextMenu() -> impl IntoView {
    let state = expect_context::<AppState>();
    let target = state.context_menu;

    // Any click elsewhere dismisses it. Captured, so the menu closes even when
    // the click lands on a handler that stops propagation — but a click on the
    // menu itself is skipped: closing removes the buttons, and the capture
    // phase runs before the button's own handler, so dismissing here would
    // delete the item before it could act.
    {
        let state = state.clone();
        for event in ["click", "contextmenu"] {
            let state = state.clone();
            let handler = wasm_bindgen::closure::Closure::wrap(Box::new(
                move |event: web_sys::Event| {
                    let inside = event
                        .target()
                        .and_then(|t| t.dyn_into::<web_sys::Element>().ok())
                        .map(|el| el.closest(".context-menu").ok().flatten().is_some())
                        .unwrap_or(false);
                    if !inside {
                        close(&state);
                    }
                },
            ) as Box<dyn FnMut(_)>);

            if let Some(document) = web_sys::window().and_then(|w| w.document()) {
                let _ = document.add_event_listener_with_callback_and_bool(
                    event,
                    handler.as_ref().unchecked_ref(),
                    true,
                );
            }
            handler.forget();
        }
    }

    // Measured after it is shown, so a menu opened near an edge flips back
    // inside the window instead of being clipped.
    let menu: NodeRef<leptos::html::Div> = NodeRef::new();
    Effect::new(move |_| {
        let Some(at) = target.get() else { return };
        let Some(element) = menu.get() else { return };
        let Some(window) = web_sys::window() else { return };

        let _ = window;
        place(&element, at.x, at.y);
    });

    view! {
        <div
            class="context-menu"
            id="context-menu"
            node_ref=menu
            hidden=move || target.with(Option::is_none)
        >
            {move || {
                let Some(at) = target.get() else { return Vec::new() };
                items_for(&at.kind)
                    .into_iter()
                    .map(|item| {
                        let state = state.clone();
                        let at = at.clone();
                        let class = if item.danger {
                            "context-item danger"
                        } else {
                            "context-item"
                        };
                        view! {
                            <button
                                class=class
                                on:click=move |event: web_sys::MouseEvent| {
                                    event.stop_propagation();
                                    close(&state);
                                    (item.run)(state.clone(), at.clone());
                                }
                            >
                                {item.label}
                            </button>
                        }
                    })
                    .collect::<Vec<_>>()
            }}
        </div>
    }
}

use wasm_bindgen::JsCast;
