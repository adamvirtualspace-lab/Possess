//! A transient "that happened — undo?" bar.
//!
//! For the operations that change files on disk. Text editing already has
//! CodeMirror's own undo stack; this covers what that stack cannot reach, where
//! the only alternative is a confirm dialog and hope.
//!
//! The action is a typed description of what to reverse rather than a stored
//! closure. Undoing a rename is another rename; undoing a delete is a restore —
//! both are better written down than captured.

use crate::state::AppState;
use leptos::prelude::*;

/// How long the offer stands before it disappears.
const TIMEOUT_MS: i32 = 12_000;

/// What an offered undo would do.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum UndoAction {
    /// Put a renamed entry back under its old name.
    Rename {
        kind: String,
        /// Where it is now.
        at: String,
        /// What it was called before.
        was: String,
    },
    /// Pull a deleted entry back out of the trash.
    Restore {
        token: String,
        kind: String,
        path: String,
        /// Whatever was open when it was deleted, so undo puts the editor back
        /// as well as the file.
        was_open: Option<String>,
    },
}

/// An offer currently on screen.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Offer {
    pub message: String,
    pub action: UndoAction,
}

/// Put an offer up, replacing any that is already there.
pub fn offer(state: &AppState, message: impl Into<String>, action: UndoAction) {
    state.undo.set(Some(Offer { message: message.into(), action }));

    let state = state.clone();
    state.clone().undo_debounce(TIMEOUT_MS, move || state.undo.set(None));
}

#[component]
pub fn UndoBar() -> impl IntoView {
    let state = expect_context::<AppState>();
    let undo = state.undo;
    let failed = RwSignal::new(None::<String>);

    let run = {
        let state = state.clone();
        move || {
            let Some(current) = undo.get_untracked() else { return };
            let state = state.clone();

            leptos::task::spawn_local(async move {
                match reverse(&state, &current.action).await {
                    Ok(()) => {
                        failed.set(None);
                        state.undo.set(None);
                    }
                    // The undo itself failed — say why and leave the bar up,
                    // rather than hiding it and letting the user believe it
                    // worked.
                    Err(err) => failed.set(Some(err)),
                }
            });
        }
    };

    // Ctrl/Cmd+Z reaches here only when focus is outside the editor —
    // CodeMirror handles its own keystrokes and stops them bubbling, so this
    // cannot hijack undo while you are typing.
    {
        let run = run.clone();
        let handler = wasm_bindgen::closure::Closure::wrap(Box::new(
            move |event: web_sys::KeyboardEvent| {
                if undo.get_untracked().is_none() || event.key() != "z" {
                    return;
                }
                if !(event.ctrl_key() || event.meta_key()) {
                    return;
                }
                event.prevent_default();
                run();
            },
        ) as Box<dyn FnMut(_)>);

        if let Some(document) = web_sys::window().and_then(|w| w.document()) {
            let _ = document
                .add_event_listener_with_callback("keydown", handler.as_ref().unchecked_ref());
        }
        handler.forget();
    }

    view! {
        <div class="undo-bar" id="undo-bar" role="status" hidden=move || undo.with(Option::is_none)>
            <span class="undo-message" id="undo-message">
                {move || {
                    failed
                        .get()
                        .or_else(|| undo.get().map(|o| o.message))
                        .unwrap_or_default()
                }}
            </span>
            <button
                class="btn active"
                id="undo-action"
                on:click=move |_| run()
            >
                "Undo"
            </button>
        </div>
    }
}

/// Carry out an undo.
async fn reverse(state: &AppState, action: &UndoAction) -> Result<(), String> {
    match action {
        UndoAction::Rename { kind, at, was } => {
            // Renaming back is just another rename with the original name.
            let back = crate::api::rename(at, was).await?;
            crate::notes::after_rename(state, kind, at, &back.path).await;
            Ok(())
        }
        UndoAction::Restore { token, kind, path, was_open } => {
            crate::api::restore(token).await?;
            state.load_notes().await?;

            // Reopen whatever was on screen when it was deleted.
            let reopen = if kind == "note" {
                Some(path.clone())
            } else {
                was_open
                    .as_ref()
                    .filter(|open| open.starts_with(&format!("{path}/")))
                    .cloned()
            };

            if let Some(reopen) = reopen {
                state.open_and_reveal(&reopen).await?;
            }
            Ok(())
        }
    }
}

use wasm_bindgen::JsCast;
