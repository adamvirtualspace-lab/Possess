//! Following a `[[link]]` from inside the editor.
//!
//! Ctrl+click (Cmd+click on a Mac) opens the link under the pointer. A plain
//! click has to stay a cursor placement — this is an editor first.

use crate::editor::editor;
use crate::state::AppState;
use possess_markdown::wikilinks;
use wasm_bindgen::prelude::*;
use wasm_bindgen::JsCast;

/// Listen for Ctrl/Cmd+click on the editor.
///
/// In the capture phase, so this runs before CodeMirror's own handler: CodeMirror
/// ignores a mousedown that is already default-prevented, which stops a Ctrl+click
/// on a link from also adding a second cursor.
pub fn wire(state: AppState) {
    let handler = Closure::wrap(Box::new(move |event: web_sys::MouseEvent| {
        if !(event.ctrl_key() || event.meta_key()) || event.button() != 0 {
            return;
        }
        if let Some(path) = link_under(&event) {
            event.prevent_default();
            crate::preview::open_wikilink(&state, &path);
        }
    }) as Box<dyn FnMut(_)>);

    let _ = editor().wrapper.add_event_listener_with_callback_and_bool(
        "mousedown",
        handler.as_ref().unchecked_ref(),
        true,
    );
    // Lives as long as the editor, which lives as long as the page.
    handler.forget();
}

/// The lookup path of the link under the pointer, if it is on one.
///
/// Only a link `editor_links` would mark counts, so what looks like a link is
/// exactly what can be followed — and a `[[` in a code sample is neither.
fn link_under(event: &web_sys::MouseEvent) -> Option<String> {
    let editor = editor();
    let pos = editor.coords_char(event.page_x() as f64, event.page_y() as f64);
    let (line, ch) = (pos.line as usize, pos.ch as usize);

    let marked = wikilinks::editor_links(&editor.content())
        .into_iter()
        .any(|link| link.line == line && (link.start..=link.end).contains(&ch));
    if !marked {
        return None;
    }

    let body = wikilinks::link_at_column(&editor.line(pos.line), ch)?;
    Some(wikilinks::parse(&body).path)
}
