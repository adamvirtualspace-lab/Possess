//! PossessApp browser UI.
//!
//! Served at /next while the original JS frontend keeps serving /, so the two
//! can be compared side by side against the same backend until this one
//! reaches parity.

mod api;

use leptos::prelude::*;
use possess_markdown::{render, RenderContext};
use wasm_bindgen::prelude::*;

#[wasm_bindgen(start)]
pub fn start() {
    // Without this a panic in WASM is an unhelpful "unreachable executed".
    console_error_panic_hook::set_once();
    leptos::mount::mount_to_body(App);
}

/// What the placeholder page is showing: the first note in the vault, rendered.
#[derive(Default, Clone)]
struct Loaded {
    vault: String,
    note: String,
    html: String,
    notes: Vec<String>,
}

/// Placeholder shell.
///
/// It opens the first note in the vault and renders it, which exercises the
/// whole chain — WASM start-up, the typed API client, the shared types and the
/// Markdown renderer — before any real component lands.
#[component]
fn App() -> impl IntoView {
    let state = RwSignal::new(None::<Result<Loaded, String>>);

    leptos::task::spawn_local(async move {
        state.set(Some(load_first_note().await));
    });

    view! {
        <main class="markdown-preview" style="padding: 2rem; max-width: 52rem;">
            {move || match state.get() {
                None => view! { <p>"Loading…"</p> }.into_any(),
                Some(Err(err)) => view! { <p>"Could not load: " {err}</p> }.into_any(),
                Some(Ok(loaded)) => view! {
                    <p style="opacity: .6">
                        {format!("{} · {} notes · showing {}",
                            loaded.vault, loaded.notes.len(), loaded.note)}
                    </p>
                    <hr />
                    // The renderer produces HTML, which is the point of it.
                    <div inner_html=loaded.html></div>
                }
                .into_any(),
            }}
        </main>
    }
}

/// Fetch the vault, its notes, and render the first one.
async fn load_first_note() -> Result<Loaded, String> {
    let vault = api::get_vault().await?;
    let folders = api::list_folders(None).await?;

    // The same flat map the sidebar builds its tree from: folder path to the
    // notes directly inside it.
    let mut notes: Vec<String> = Vec::new();
    for (folder, entries) in &folders.folders {
        for entry in entries {
            notes.push(if folder == "." {
                entry.0.clone()
            } else {
                format!("{folder}/{}", entry.0)
            });
        }
    }
    notes.sort();

    let Some(first) = notes.first().cloned() else {
        return Ok(Loaded {
            vault: vault.name,
            note: "no notes yet".to_string(),
            ..Default::default()
        });
    };

    let file = api::get_file(&first).await?;
    let context = RenderContext { note_path: Some(&first), notes: &notes };

    Ok(Loaded {
        html: render(&file.content, &context),
        vault: vault.name,
        note: first,
        notes,
    })
}
