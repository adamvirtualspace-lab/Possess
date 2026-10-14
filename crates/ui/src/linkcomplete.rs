//! Autocomplete for `[[` in the editor.
//!
//! Typing the brackets opens a filtered list of notes; Enter or Tab inserts
//! one. Backed by the note list the sidebar already holds, so there is no
//! request per keystroke.
//!
//! The ranking and the inserted text are decided in `possess_markdown`, which
//! is tested natively. What is here is the part that needs a browser: where the
//! popup sits, and which keys it borrows while it is open.

use crate::editor::editor;
use crate::state::AppState;
use leptos::prelude::*;
use possess_markdown::wikilinks::{self, MAX_COMPLETIONS};
use wasm_bindgen::JsCast;

/// Gap between the caret and the popup.
const CARET_GAP: f64 = 4.0;
/// Space the popup needs below the caret before it flips above it.
const FLIP_MARGIN: f64 = 8.0;
/// Keeps the popup clear of the right-hand edge.
const EDGE_MARGIN: f64 = 12.0;

/// Which keys the list borrows while it is open.
///
/// Registered only while it is up, so Enter and Tab behave normally the rest of
/// the time. The shim hands back the name of whichever one fired, so this table
/// exists on one side only.
const KEYS: &[&str] = &["Down", "Up", "Enter", "Tab", "Esc"];

#[component]
pub fn LinkComplete() -> impl IntoView {
    let state = expect_context::<AppState>();

    let matches = state.complete_matches;
    let active = state.complete_active;

    // Measured and placed after it is shown, which is the only way to know how
    // tall it is.
    let popup: NodeRef<leptos::html::Div> = NodeRef::new();
    Effect::new(move |_| {
        let count = matches.with(Vec::len);
        if count == 0 {
            return;
        }
        if let Some(element) = popup.get() {
            position(&element);
        }
    });

    // Paired with their position so the list can mark one active; computed
    // outside the view because a turbofish inside the macro will not parse.
    let indexed = Memo::new(move |_| {
        let found: Vec<(usize, String)> = matches.get().into_iter().enumerate().collect();
        found
    });
    let is_closed = move || matches.with(|found| found.is_empty());

    // Mousedown, not click: a click blurs the editor first, which closes the
    // list before the selection lands.
    let on_mousedown = move |event: web_sys::MouseEvent| {
        let clicked = event
            .target()
            .and_then(|t| t.dyn_into::<web_sys::Element>().ok())
            .and_then(|el| el.closest(".link-option").ok().flatten())
            .and_then(|el| el.get_attribute("data-index"))
            .and_then(|value| value.parse::<usize>().ok());

        if let Some(index) = clicked {
            event.prevent_default();
            choose(&state, index);
        }
    };

    view! {
        <div
            class="link-complete"
            id="link-complete"
            node_ref=popup
            hidden=is_closed
            on:mousedown=on_mousedown
        >
            <For
                each=move || indexed.get()
                key=|entry: &(usize, String)| format!("{}:{}", entry.0, entry.1)
                children=move |(index, path): (usize, String)| {
                    let folder = path
                        .rfind('/')
                        .map(|at| path[..at].to_string())
                        .unwrap_or_default();
                    let name = path
                        .rsplit('/')
                        .next()
                        .unwrap_or(&path)
                        .strip_suffix(".md")
                        .unwrap_or(&path)
                        .to_string();

                    let has_folder = !folder.is_empty();
                    view! {
                        <div
                            class="link-option"
                            class:is-active=move || active.get() == index
                            data-index=index.to_string()
                        >
                            <span class="link-option-name">{name}</span>
                            <Show when=move || has_folder>
                                <span class="link-option-path">{folder.clone()}</span>
                            </Show>
                        </div>
                    }
                }
            />
        </div>
    }
}

/// Re-evaluate the list for wherever the cursor now is.
///
/// Called on every cursor move, so it does nothing but read one line and filter
/// a list already in memory.
pub fn update(state: &AppState) {
    let editor = editor();
    let cursor = editor.cursor();
    let line = editor.line(cursor.line);

    let Some(context) = wikilinks::context_at(&line, cursor.ch as usize) else {
        close(state);
        return;
    };

    let notes = state.notes.get_untracked();
    let current = state.current_file.get_untracked();
    let found = wikilinks::complete(
        &context.typed,
        &notes,
        current.as_deref(),
        MAX_COMPLETIONS,
    );

    if found.is_empty() {
        close(state);
        return;
    }

    // Claimed on the way up, released on the way down, so the keys are only
    // ours while there is something to choose from.
    if state.complete_matches.with_untracked(Vec::is_empty) {
        let state = state.clone();
        editor.set_key_map(KEYS, move |key| on_key(&state, &key));
    }

    state.complete_matches.set(found);
    state.complete_active.set(0);
}

/// Close the list and give the keys back.
pub fn close(state: &AppState) {
    if state.complete_matches.with_untracked(Vec::is_empty) {
        return;
    }
    state.complete_matches.set(Vec::new());
    state.complete_active.set(0);
    editor().clear_key_map();
}

fn on_key(state: &AppState, key: &str) {
    match key {
        "Down" => move_active(state, 1),
        "Up" => move_active(state, -1),
        "Enter" | "Tab" => choose(state, state.complete_active.get_untracked()),
        _ => close(state),
    }
}

/// Move the selection, wrapping at either end.
fn move_active(state: &AppState, step: i32) {
    let count = state.complete_matches.with_untracked(Vec::len);
    if count == 0 {
        return;
    }
    state.complete_active.update(|index| {
        let next = *index as i32 + step;
        *index = next.rem_euclid(count as i32) as usize;
    });
}

/// Insert the chosen note as a link, replacing what has been typed.
fn choose(state: &AppState, index: usize) {
    let Some(path) = state.complete_matches.with_untracked(|found| found.get(index).cloned())
    else {
        return;
    };

    let editor = editor();
    let cursor = editor.cursor();
    let line = editor.line(cursor.line);

    // Re-read rather than remembered: the cursor may have moved since the list
    // was built, and the range being replaced has to be the one on screen now.
    let Some(context) = wikilinks::context_at(&line, cursor.ch as usize) else {
        close(state);
        return;
    };

    let notes = state.notes.get_untracked();
    let current = state.current_file.get_untracked();
    let text = wikilinks::shortest_form(&path, current.as_deref(), &notes);

    editor.replace_range(
        &format!("[[{text}]]"),
        crate::editor::Pos { line: cursor.line, ch: context.start as u32 },
        cursor,
    );

    close(state);
    editor.focus();
}

/// Put the popup by the caret, flipping above it when there is no room below.
fn position(element: &web_sys::HtmlElement) {
    let Some(window) = web_sys::window() else { return };

    let (left, top, bottom) = editor().cursor_coords();
    let height = element.offset_height() as f64;
    let width = element.offset_width() as f64;

    let viewport_height = window.inner_height().ok().and_then(|v| v.as_f64()).unwrap_or(0.0);
    let viewport_width = window.inner_width().ok().and_then(|v| v.as_f64()).unwrap_or(0.0);

    let fits_below = bottom + height + FLIP_MARGIN < viewport_height;
    let y = if fits_below { bottom + CARET_GAP } else { top - height - CARET_GAP };
    let x = left.min(viewport_width - width - EDGE_MARGIN);

    let _ = element.style().set_property("top", &format!("{y}px"));
    let _ = element.style().set_property("left", &format!("{x}px"));
}
