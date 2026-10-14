//! Creating, renaming and deleting notes and folders.
//!
//! Every action ends by reloading the tree from disk, so the sidebar always
//! reflects what actually happened on the filesystem rather than what was
//! asked for. Renames and deletes offer an undo; both are reversible on the
//! server, which is the only reason deleting a folder is a single click.

use crate::state::AppState;
use leptos::prelude::*;
use crate::undo::{self, UndoAction};

/// Where a new note or folder goes by default.
///
/// Beside the open note, which is nearly always what is meant; with nothing
/// open, the vault root.
pub fn current_folder(state: &AppState) -> String {
    state
        .current_file
        .get_untracked()
        .and_then(|open| open.rfind('/').map(|at| open[..at].to_string()))
        .unwrap_or_default()
}

fn prompt(message: &str, default: &str) -> Option<String> {
    web_sys::window()?
        .prompt_with_message_and_default(message, default)
        .ok()
        .flatten()
}

fn confirm(message: &str) -> bool {
    web_sys::window()
        .and_then(|window| window.confirm_with_message(message).ok())
        .unwrap_or(false)
}

fn alert(message: &str) {
    if let Some(window) = web_sys::window() {
        let _ = window.alert_with_message(message);
    }
}

fn describe(parent: &str) -> &str {
    if parent.is_empty() {
        "vault root"
    } else {
        parent
    }
}

pub async fn create_note(state: &AppState, parent: &str) {
    let Some(name) = prompt(&format!("New note in {}:", describe(parent)), "Untitled.md") else {
        return;
    };

    match crate::api::create("note", parent, name.trim()).await {
        Ok(created) => {
            if let Err(err) = state.load_notes().await {
                leptos::logging::error!("Could not reload notes: {err}");
            }
            if let Err(err) = state.open_and_reveal(&created.path).await {
                leptos::logging::error!("Could not open new note: {err}");
            }
        }
        Err(err) => alert(&err),
    }
}

pub async fn create_folder(state: &AppState, parent: &str) {
    let Some(name) = prompt(&format!("New folder in {}:", describe(parent)), "New folder") else {
        return;
    };

    match crate::api::create("folder", parent, name.trim()).await {
        Ok(_) => {
            if let Err(err) = state.load_notes().await {
                leptos::logging::error!("Could not reload notes: {err}");
            }
        }
        Err(err) => alert(&err),
    }
}

pub async fn rename(state: &AppState, kind: &str, path: &str, current_name: &str) {
    let Some(name) = prompt(&format!("Rename {kind}:"), current_name) else {
        return;
    };
    let name = name.trim().to_string();
    if name == current_name {
        return;
    }

    match crate::api::rename(path, &name).await {
        Ok(result) => {
            after_rename(state, kind, path, &result.path).await;
            undo::offer(
                state,
                format!("Renamed to {}", result.path),
                UndoAction::Rename {
                    kind: kind.to_string(),
                    at: result.path,
                    was: current_name.to_string(),
                },
            );
        }
        Err(err) => alert(&err),
    }
}

/// Follow a rename: the open note may have moved, or been carried along inside
/// a renamed folder.
pub async fn after_rename(state: &AppState, kind: &str, from: &str, to: &str) {
    if let Some(open) = state.current_file.get_untracked() {
        let moved = if kind == "note" && open == from {
            Some(to.to_string())
        } else if kind == "folder" && open.starts_with(&format!("{from}/")) {
            Some(format!("{to}{}", &open[from.len()..]))
        } else {
            None
        };

        if let Some(moved) = moved {
            // The file is already where it needs to be; only the path the
            // editor and the sync poll point at has to follow it.
            state.current_file.set(Some(moved));
        }
    }

    if let Err(err) = state.load_notes().await {
        leptos::logging::error!("Could not reload notes: {err}");
    }
}

pub async fn remove(state: &AppState, kind: &str, path: &str) {
    let warning = if kind == "folder" {
        format!("Delete folder \"{path}\" and everything inside it?")
    } else {
        format!("Delete note \"{path}\"?")
    };
    if !confirm(&warning) {
        return;
    }

    let was_open = state.current_file.get_untracked();

    match crate::api::remove(path).await {
        Ok(deleted) => {
            // Deleting the open note leaves the editor pointing at a path that
            // no longer exists; clear it so autosave does not recreate the file
            // that was just removed.
            let affected = was_open.as_ref().is_some_and(|open| {
                (kind == "note" && open == path)
                    || (kind == "folder" && open.starts_with(&format!("{path}/")))
            });
            if affected {
                state.close();
            }

            if let Err(err) = state.load_notes().await {
                leptos::logging::error!("Could not reload notes: {err}");
            }

            undo::offer(
                state,
                format!("Deleted {path}"),
                UndoAction::Restore {
                    token: deleted.token,
                    kind: kind.to_string(),
                    path: path.to_string(),
                    was_open,
                },
            );
        }
        Err(err) => alert(&err),
    }
}
