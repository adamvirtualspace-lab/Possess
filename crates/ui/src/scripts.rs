//! The Scripts panel.
//!
//! Lists the `.py` files in `<vault>/scripts/`, runs them, and shows what they
//! printed. A run can rewrite notes on disk, so when one reports changes the
//! tree is rebuilt and the open note reloaded immediately rather than leaving
//! it to the sync poll.

use crate::modal;
use crate::state::AppState;
use leptos::prelude::*;
use possess_common::{ScriptInfo, ScriptsResponse};

#[component]
pub fn ScriptsPanel() -> impl IntoView {
    let state = expect_context::<AppState>();

    let open = state.scripts_open;
    let listing = RwSignal::new(None::<ScriptsResponse>);
    let output = RwSignal::new(String::new());
    let failed = RwSignal::new(false);
    let running = RwSignal::new(None::<String>);

    let print = move |text: String, error: bool| {
        output.set(text);
        failed.set(error);
    };

    let refresh = move || {
        leptos::task::spawn_local(async move {
            match crate::api::list_scripts().await {
                Ok(found) => listing.set(Some(found)),
                Err(err) => print(err, true),
            }
        });
    };

    Effect::new(move |_| {
        if !open.get() {
            return;
        }
        print(String::new(), false);
        refresh();
    });

    modal::close_on_escape(open);
    let on_backdrop = modal::close_on_backdrop(open);
    let for_scaffold = state.clone();

    let run = StoredValue::new({
        let state = state.clone();
        move |name: String| {
            let state = state.clone();
            running.set(Some(name.clone()));
            print(format!("$ {name}\n"), false);

            leptos::task::spawn_local(async move {
                let note = state.current_file.get_untracked();
                match crate::api::run_script(&name, note.as_deref()).await {
                    Ok(result) => {
                        print(describe(&result), result.code != 0);

                        // Notes changed underneath us: rebuild the tree and pull
                        // the open note back in.
                        if !result.changed.is_empty() {
                            if let Err(err) = state.load_notes().await {
                                leptos::logging::error!("Could not reload notes: {err}");
                            }
                            if let Some(path) = state.current_file.get_untracked() {
                                // Gone entirely if the script deleted it.
                                if state.open(&path).await.is_err() {
                                    state.close();
                                }
                            }
                        }
                    }
                    Err(err) => print(err, true),
                }
                running.set(None);
            });
        }
    });


    let toggle_hooks = move |event: web_sys::Event| {
        let Some(input) = event
            .target()
            .and_then(|t| t.dyn_into::<web_sys::HtmlInputElement>().ok())
        else {
            return;
        };
        let enabled = input.checked();

        leptos::task::spawn_local(async move {
            match crate::api::set_hooks(enabled).await {
                Ok(_) => print(
                    if enabled {
                        "Save-hooks are on for this vault. Every script in scripts/hooks/ \
                         now runs after each save."
                            .to_string()
                    } else {
                        "Save-hooks are off for this vault.".to_string()
                    },
                    false,
                ),
                Err(err) => {
                    // Put the box back where it was: the server said no.
                    refresh();
                    print(err, true);
                }
            }
        });
    };

    let scripts = move || {
        listing.get().map(|found| found.scripts).unwrap_or_default()
    };
    let has_scripts = move || !scripts().is_empty();
    let folder_exists = move || listing.get().map(|found| found.exists).unwrap_or(false);

    view! {
        <div
            class="modal-backdrop"
            id="scripts-modal"
            hidden=move || !open.get()
            on:click=on_backdrop
        >
        <div class="modal" role="dialog" aria-modal="true" aria-labelledby="scripts-title">
            <div class="modal-header">
                <h2 id="scripts-title">"Python scripts"</h2>
                <button
                    class="modal-close"
                    id="scripts-close"
                    title="Close"
                    on:click=move |_| open.set(false)
                >
                    "\u{00D7}"
                </button>
            </div>

            <div class="modal-body">
                <p class="scripts-dir" id="scripts-dir">
                    {move || listing.get().map(|f| f.dir).unwrap_or_default()}
                </p>

                <div class="scripts-list" id="scripts-list">
                    <Show
                        when=has_scripts
                        fallback=move || {
                            let state = for_scaffold.clone();
                            view! {
                                <div class="scripts-empty">
                                    // One branch per state rather than nested
                                    // Shows: the button's handler cannot be
                                    // moved out of a closure that may run more
                                    // than once, so it is rebuilt per render.
                                    {move || {
                                        if folder_exists() {
                                            return view! {
                                                <p>
                                                    "No .py files in this folder yet. Add one \
                                                     and it shows up here."
                                                </p>
                                            }
                                            .into_any();
                                        }

                                        let state = state.clone();
                                        let scaffold = move |_| {
                                            let state = state.clone();
                                            leptos::task::spawn_local(async move {
                                                match crate::api::scaffold_scripts().await {
                                                    Ok(made) => {
                                                        print(
                                                            format!(
                                                                "Created:\n  {}",
                                                                made.created.join("\n  "),
                                                            ),
                                                            false,
                                                        );
                                                        refresh();
                                                        if let Err(err) =
                                                            state.load_notes().await
                                                        {
                                                            leptos::logging::error!(
                                                                "Could not reload notes: {err}"
                                                            );
                                                        }
                                                    }
                                                    Err(err) => print(err, true),
                                                }
                                            });
                                        };

                                        view! {
                                            <p>"This vault has no scripts folder yet."</p>
                                            <p class="scripts-empty-hint">
                                                "Creating it adds a README and three worked \
                                                 examples: vault stats, an open-task list, and \
                                                 a checkbox syncer."
                                            </p>
                                            <button class="btn active" on:click=scaffold>
                                                "Create scripts folder"
                                            </button>
                                        }
                                        .into_any()
                                    }}
                                </div>
                            }
                        }
                    >
                        <For
                            each=scripts
                            key=|script: &ScriptInfo| script.name.clone()
                            children=move |script| {
                                let name = script.name.clone();
                                let busy = name.clone();
                                let is_hook = script.kind == "hook";
                                view! {
                                    <div class="script-row">
                                        <span class="script-name">{script.name.clone()}</span>
                                        <Show when=move || is_hook>
                                            <span
                                                class="script-badge"
                                                title="Also runs after every save while hooks are on"
                                            >
                                                "hook"
                                            </span>
                                        </Show>
                                        <button
                                            class="btn script-run"
                                            disabled=move || {
                                                running.get().as_deref() == Some(busy.as_str())
                                            }
                                            on:click=move |_| {
                                                run.with_value(|run| run(name.clone()))
                                            }
                                        >
                                            {move || {
                                                if running.get().is_some() { "Running\u{2026}" }
                                                else { "Run" }
                                            }}
                                        </button>
                                    </div>
                                }
                            }
                        />
                    </Show>
                </div>

                <div class="scripts-hooks-row" id="scripts-hooks-row" hidden=move || !has_scripts()>
                    <input
                        type="checkbox"
                        id="scripts-hooks"
                        prop:checked=move || {
                            listing.get().map(|f| f.hooks_enabled).unwrap_or(false)
                        }
                        on:change=toggle_hooks
                    />
                    <label class="scripts-hooks-text" for="scripts-hooks">
                        "Run " <code>"scripts/hooks/"</code> " after every save"
                        <small>
                            "Off by default for each vault. Scripts run as you, with your \
                             permissions \u{2014} turn this on only for code you trust."
                        </small>
                    </label>
                </div>

                <pre
                    class="scripts-output"
                    id="scripts-output"
                    class:has-error=move || failed.get()
                >
                    {move || output.get()}
                </pre>
            </div>

            <div class="modal-footer">
                <button class="btn" id="scripts-done" on:click=move |_| open.set(false)>
                    "Close"
                </button>
            </div>
        </div>
        </div>
    }
}

/// What a finished run printed, plus how it ended.
fn describe(result: &possess_common::RunScriptResponse) -> String {
    let mut parts = vec![format!("$ {}", result.script), String::new()];

    if !result.stdout.trim().is_empty() {
        parts.push(result.stdout.trim_end().to_string());
    }
    if !result.stderr.trim().is_empty() {
        parts.push(result.stderr.trim_end().to_string());
    }

    let status = if result.code == 0 {
        format!("\n\u{2713} finished in {}s", result.seconds)
    } else {
        format!(
            "\n\u{2717} exited with code {} after {}s",
            result.code, result.seconds
        )
    };

    let changed = if result.changed.is_empty() {
        String::new()
    } else {
        format!("\n{} note(s) changed on disk", result.changed.len())
    };

    format!("{}{status}{changed}", parts.join("\n"))
}

use wasm_bindgen::JsCast;
