//! PossessApp browser UI.
//!
//! Served at /next while the original JS frontend keeps serving /, so the two
//! can be compared side by side against the same backend until this one
//! reaches parity.

mod api;

use leptos::prelude::*;
use wasm_bindgen::prelude::*;

#[wasm_bindgen(start)]
pub fn start() {
    // Without this a panic in WASM is an unhelpful "unreachable executed".
    console_error_panic_hook::set_once();
    leptos::mount::mount_to_body(App);
}

/// Placeholder shell: proves the WASM build, the typed API client and the
/// shared types all work end to end before any real component lands.
#[component]
fn App() -> impl IntoView {
    let vault = RwSignal::new(None::<Result<String, String>>);

    leptos::task::spawn_local(async move {
        vault.set(Some(match api::get_vault().await {
            Ok(info) => Ok(format!("{} ({})", info.name, info.path)),
            Err(err) => Err(err),
        }));
    });

    view! {
        <main style="font: 14px system-ui; padding: 2rem;">
            <h1>"PossessApp — Rust UI"</h1>
            <p>
                {move || match vault.get() {
                    None => "Loading vault…".to_string(),
                    Some(Ok(text)) => format!("Vault: {text}"),
                    Some(Err(err)) => format!("Failed to reach the API: {err}"),
                }}
            </p>
        </main>
    }
}
