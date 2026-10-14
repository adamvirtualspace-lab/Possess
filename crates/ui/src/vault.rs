//! The vault picker: browse the filesystem and point the app at a folder.
//!
//! The server does the walking — this only shows what it reports and sends back
//! the folder that was settled on. Switching vaults invalidates everything the
//! app holds, so the open note is closed and the tree reloaded.

use crate::modal;
use crate::state::AppState;
use leptos::prelude::*;
use possess_common::DirEntry;
use wasm_bindgen::JsCast;

#[component]
pub fn VaultPicker() -> impl IntoView {
    let state = expect_context::<AppState>();

    let open = state.vault_open;
    let browsing = RwSignal::new(String::new());
    let parent = RwSignal::new(None::<String>);
    let dirs = RwSignal::new(Vec::<DirEntry>::new());
    let drives = RwSignal::new(Vec::<String>::new());
    let error = RwSignal::new(None::<String>);

    let navigate = move |path: Option<String>| {
        leptos::task::spawn_local(async move {
            match crate::api::browse(path.as_deref()).await {
                Ok(listing) => {
                    browsing.set(listing.path);
                    parent.set(listing.parent);
                    dirs.set(listing.dirs);
                    drives.set(listing.drives);
                    error.set(None);
                }
                Err(err) => error.set(Some(err)),
            }
        });
    };

    // Opening starts wherever the current vault is.
    {
        let state = state.clone();
        Effect::new(move |_| {
            if !open.get() {
                return;
            }
            error.set(None);
            let at = state.vault_path.get_untracked();
            navigate(if at.is_empty() { None } else { Some(at) });
        });
    }

    let select = {
        let state = state.clone();
        move |_| {
            let target = browsing.get_untracked();
            if target.is_empty() {
                return;
            }
            let state = state.clone();

            leptos::task::spawn_local(async move {
                match crate::api::set_vault(&target).await {
                    Ok(info) => {
                        state.vault_name.set(info.name);
                        state.vault_path.set(info.path);
                        open.set(false);

                        // The open note belonged to the old vault, so it goes
                        // before the tree is rebuilt — otherwise the autosave
                        // and sync timers keep pointing at a stale path.
                        state.close();
                        state.expanded.update(|folders| folders.clear());
                        if let Err(err) = state.load_notes().await {
                            leptos::logging::error!("Could not load the new vault: {err}");
                        }
                    }
                    Err(err) => error.set(Some(err)),
                }
            });
        }
    };

    let on_path_key = move |event: web_sys::KeyboardEvent| {
        if event.key() != "Enter" {
            return;
        }
        if let Some(input) = event
            .target()
            .and_then(|t| t.dyn_into::<web_sys::HtmlInputElement>().ok())
        {
            navigate(Some(input.value()));
        }
    };

    modal::close_on_escape(open);
    let on_backdrop = modal::close_on_backdrop(open);

    view! {
        <div
            class="modal-backdrop"
            id="vault-modal"
            hidden=move || !open.get()
            on:click=on_backdrop
        >
        <div class="modal" role="dialog" aria-modal="true" aria-labelledby="vault-modal-title">
            <div class="modal-header">
                <h2 id="vault-modal-title">"Select vault folder"</h2>
                <button
                    class="modal-close"
                    id="vault-close"
                    title="Close"
                    on:click=move |_| open.set(false)
                >
                    "\u{00D7}"
                </button>
            </div>
            <div class="modal-body">
            <div class="vault-path-row">
                <input
                    type="text"
                    id="vault-path-input"
                    spellcheck="false"
                    placeholder="Type or paste a folder path"
                    prop:value=move || browsing.get()
                    on:keydown=on_path_key
                />
                <button
                    class="btn"
                    id="vault-go"
                    on:click=move |_| {
                        let value = web_sys::window()
                            .and_then(|w| w.document())
                            .and_then(|d| d.get_element_by_id("vault-path-input"))
                            .and_then(|e| e.dyn_into::<web_sys::HtmlInputElement>().ok())
                            .map(|input| input.value())
                            .unwrap_or_default();
                        navigate(Some(value));
                    }
                >
                    "Go"
                </button>
            </div>

            <div class="vault-drives" id="vault-drives" hidden=move || drives.with(Vec::is_empty)>
                <For
                    each=move || drives.get()
                    key=|drive: &String| drive.clone()
                    children=move |drive| {
                        let target = drive.clone();
                        view! {
                            <button
                                class="vault-drive"
                                on:click=move |_| navigate(Some(target.clone()))
                            >
                                {drive.replace('\\', "")}
                            </button>
                        }
                    }
                />
            </div>

            <div class="vault-list" id="vault-list">
                <Show when=move || parent.with(Option::is_some)>
                    {move || {
                        let up = parent.get().unwrap_or_default();
                        let title = up.clone();
                        view! {
                            <button
                                class="vault-row is-parent"
                                title=title
                                on:click=move |_| navigate(Some(up.clone()))
                            >
                                ".."
                            </button>
                        }
                    }}
                </Show>
                <For
                    each=move || dirs.get()
                    key=|dir: &DirEntry| dir.path.clone()
                    children=move |dir| {
                        let target = dir.path.clone();
                        view! {
                            <button
                                class="vault-row"
                                title=dir.path.clone()
                                on:click=move |_| navigate(Some(target.clone()))
                            >
                                {dir.name.clone()}
                            </button>
                        }
                    }
                />
                <Show when=move || {
                    parent.with(Option::is_none) && dirs.with(Vec::is_empty)
                }>
                    <p class="vault-empty">"No sub-folders here."</p>
                </Show>
            </div>

            <p class="vault-error" id="vault-error" hidden=move || error.with(Option::is_none)>
                {move || error.get().unwrap_or_default()}
            </p>

            </div>
            <div class="modal-footer">
                <button class="btn" id="vault-cancel" on:click=move |_| open.set(false)>
                    "Cancel"
                </button>
                <button class="btn active" id="vault-select" on:click=select>
                    "Use this folder"
                </button>
            </div>
        </div>
        </div>
    }
}
