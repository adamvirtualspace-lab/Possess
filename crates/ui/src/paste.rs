//! Pasting an image straight into a note.
//!
//! A clipboard image has no file on disk, so it goes to the server as base64
//! and comes back as a path. The server files it beside the note; the link that
//! points there replaces a placeholder, and the inline-image widget picks it up.

use crate::editor::{editor, Pos};
use crate::state::AppState;
use leptos::prelude::*;
use possess_markdown::utf16;
use std::cell::Cell;

thread_local! {
    /// Numbers the placeholders, so two quick pastes never match each other's.
    static SEQ: Cell<u32> = const { Cell::new(0) };
}

/// Insert `file` at the cursor.
pub fn insert(state: &AppState, file: web_sys::File) {
    let Some(note) = state.current_file.get_untracked() else {
        state.warn("Open a note before pasting an image.");
        return;
    };

    // A placeholder marks the spot, so a slow write doesn't look like the paste
    // was swallowed — and gives something exact to replace once it lands.
    let seq = SEQ.with(|seq| {
        seq.set(seq.get() + 1);
        seq.get()
    });
    let token = format!("![uploading image {seq}\u{2026}]()");
    editor().replace_selection(&token);

    let state = state.clone();
    leptos::task::spawn_local(async move {
        let uploaded = match read_base64(&file).await {
            Ok(data) => crate::api::paste_image(&note, Some(&file.name()), &data).await,
            Err(err) => Err(err),
        };

        // The user may have opened another note while this was in flight.
        // Switching saved the old one, placeholder and all, so the swap is made
        // in the file on disk instead of in the editor.
        if state.current_file.get_untracked().as_deref() != Some(note.as_str()) {
            let replacement = match &uploaded {
                Ok(saved) => format!("![]({})", saved.markdown),
                Err(_) => String::new(),
            };
            if let Err(err) = replace_on_disk(&note, &token, &replacement).await {
                leptos::logging::error!("Could not update {note}: {err}");
            }
            if let Err(err) = uploaded {
                state.warn(err);
            }
            return;
        }

        match uploaded {
            Ok(saved) => {
                replace_token(&token, &format!("![]({})", saved.markdown));
                editor().refresh_images(0);
                // The image is already on disk, so the note that links to it
                // is saved now rather than at the next autosave.
                if let Err(err) = state.save_now().await {
                    leptos::logging::error!("Could not save: {err}");
                }
            }
            Err(err) => {
                replace_token(&token, "");
                state.warn(err);
            }
        }
    });
}

/// The file's bytes, base64-encoded.
///
/// Encoded by the browser's own `btoa`, which wants a "binary string" — one
/// character per byte — rather than bytes, hence the widening to `char`.
async fn read_base64(file: &web_sys::File) -> Result<String, String> {
    let buffer = wasm_bindgen_futures::JsFuture::from(file.array_buffer())
        .await
        .map_err(|_| "Could not read the pasted image".to_string())?;
    let bytes = js_sys::Uint8Array::new(&buffer).to_vec();
    let binary: String = bytes.into_iter().map(char::from).collect();
    web_sys::window()
        .ok_or("No window to encode the image in")?
        .btoa(&binary)
        .map_err(|_| "Could not encode the pasted image".to_string())
}

/// Swap the placeholder in a note that is no longer open.
async fn replace_on_disk(note: &str, token: &str, replacement: &str) -> Result<(), String> {
    let file = crate::api::get_file(note).await?;
    if file.content.contains(token) {
        crate::api::save_file(note, &file.content.replacen(token, replacement, 1)).await?;
    }
    Ok(())
}

/// Swap the placeholder for `replacement` wherever it has ended up.
///
/// Searched for rather than remembered: the user may have typed above it while
/// the upload was in flight, so a stored position would be wrong by now.
fn replace_token(token: &str, replacement: &str) {
    let editor = editor();

    for line in 0..editor.line_count() {
        let text = editor.line(line);
        let Some(byte) = text.find(token) else { continue };

        // The editor counts UTF-16 units, so the byte offset is converted —
        // anything non-ASCII earlier on the line would otherwise shift it.
        let from = utf16::offset_of(&text, byte) as u32;
        let to = from + utf16::len(token) as u32;
        editor.replace_range(replacement, Pos { line, ch: from }, Pos { line, ch: to });
        editor.set_cursor(Pos { line, ch: from + utf16::len(replacement) as u32 });
        editor.focus();
        return;
    }
}
