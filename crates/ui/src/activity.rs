//! The Activity column: everything that changed task statuses, newest first,
//! with Undo, and Apply / Dismiss for edits made outside the app
//! (SYNTAX.md section 6).
//!
//! The log lives on the server (`.obsessed/activity.json`); this only shows
//! it, and redraws when `/api/events` says something changed.

use leptos::prelude::*;
use possess_common::{ActivityChange, ActivityEntry, ActivityResponse, ActivityResult};
use wasm_bindgen::prelude::*;
use wasm_bindgen::JsCast;

use crate::api;
use crate::state::{AppState, ViewMode};

/// Listen to `/api/events` for as long as the page is open. The browser
/// reconnects on its own if the server restarts.
pub fn listen(state: AppState) {
    let Ok(source) = web_sys::EventSource::new("/api/events") else {
        leptos::logging::error!("Could not listen for changes: no EventSource");
        return;
    };
    // A burst of events (a `git pull` touching many notes) becomes one
    // redraw: the first event starts a short timer, later ones ride on it.
    let waiting = std::rc::Rc::new(std::cell::Cell::new(false));
    let bump = {
        let waiting = waiting.clone();
        Closure::<dyn FnMut()>::new(move || {
            waiting.set(false);
            state.revision.update(|r| *r += 1);
        })
    };
    let on_message = Closure::<dyn FnMut(web_sys::MessageEvent)>::new(move |_| {
        if waiting.replace(true) {
            return;
        }
        if let Some(window) = web_sys::window() {
            let _ = window.set_timeout_with_callback_and_timeout_and_arguments_0(
                bump.as_ref().unchecked_ref(),
                REDRAW_DELAY_MS,
            );
        }
    });
    source.set_onmessage(Some(on_message.as_ref().unchecked_ref()));
    // Both live for the whole page.
    on_message.forget();
    std::mem::forget(source);
}

/// How long the page waits after a change event before redrawing, so a
/// burst of them costs one fetch.
const REDRAW_DELAY_MS: i32 = 120;

fn source_label(source: &str) -> &'static str {
    match source {
        "click" => "Changed",
        "apply" => "Applied",
        "undo" => "Undone",
        "outside" => "Outside edit",
        "editor" => "Edited in the editor",
        _ => "Change",
    }
}

/// "14:28" in the viewer's own time zone.
fn clock(rfc3339: &str) -> String {
    let date = js_sys::Date::new(&JsValue::from_str(rfc3339));
    if date.get_time().is_nan() {
        return String::new();
    }
    format!("{:02}:{:02}", date.get_hours(), date.get_minutes())
}

fn change_row(change: &ActivityChange) -> impl IntoView {
    view! {
        <div class="activity-change" title=format!("{}.md, line {}", change.path, change.line)>
            <span class="activity-symbols">
                {change.from_symbol.clone()}" → "{change.to_symbol.clone()}
            </span>
            <span class="activity-text">{change.text.clone()}</span>
            <span class="activity-path">{format!("{}:{}", change.path, change.line)}</span>
        </div>
    }
}

/// Run an action, then reload the open note if it touched it and redraw.
fn run(
    state: AppState,
    error: RwSignal<Option<String>>,
    on_change: impl Fn() + Copy + Send + Sync + 'static,
    action: impl std::future::Future<Output = Result<ActivityResult, String>> + 'static,
) {
    leptos::task::spawn_local(async move {
        match action.await {
            Ok(result) => {
                error.set(None);
                state.reload_if_touched(&result.changes).await;
            }
            Err(err) => error.set(Some(err)),
        }
        on_change();
    });
}

#[component]
pub fn ActivityPanel(
    open: RwSignal<bool>,
    /// Called after an undo, apply or dismiss, to redraw the graph.
    on_change: impl Fn() + Copy + Send + Sync + 'static,
) -> impl IntoView {
    let state = expect_context::<AppState>();
    let data = RwSignal::new(None::<ActivityResponse>);
    let error = RwSignal::new(None::<String>);

    // Load when shown, and whenever the server reports a change.
    {
        let state = state.clone();
        Effect::new(move |_| {
            state.revision.track();
            if state.view_mode.get() != ViewMode::Graph || !open.get() {
                return;
            }
            leptos::task::spawn_local(async move {
                match api::get_activity().await {
                    Ok(response) => data.set(Some(response)),
                    Err(err) => error.set(Some(err)),
                }
            });
        });
    }

    let entry_view = {
        let state = state.clone();
        move |entry: ActivityEntry, undo_target: Option<u64>| {
            let id = entry.id;
            let is_target = undo_target == Some(id);
            let pending = entry.state.as_deref() == Some("pending");
            let label = source_label(&entry.source);
            let outside = matches!(entry.source.as_str(), "outside" | "editor");
            let state_undo = state.clone();
            let state_apply = state.clone();
            let state_dismiss = state.clone();
            let would = entry.would.clone();
            let settled = match entry.state.as_deref() {
                Some("applied") => Some("applied"),
                Some("dismissed") => Some("dismissed"),
                _ => None,
            };
            view! {
                <div
                    class="activity-entry"
                    class:is-outside=outside
                    class:is-undone=entry.undone
                    class:is-pending=pending
                    data-id=id
                    data-source=entry.source.clone()
                >
                    <div class="activity-head">
                        <span class="activity-source">{label}</span>
                        {entry.undone.then(|| view! { <span class="activity-tag">"undone"</span> })}
                        {settled.map(|s| view! { <span class="activity-tag">{s}</span> })}
                        <span class="activity-time">{clock(&entry.time)}</span>
                    </div>
                    {entry.changes.iter().map(change_row).collect_view()}
                    {entry
                        .loops
                        .iter()
                        .map(|l| view! {
                            <div class="activity-loop" title=format!("{}.md, line {}", l.path, l.line)>
                                "Loop guard skipped [" {l.wanted.to_string()} "] for " {l.text.clone()}
                            </div>
                        })
                        .collect_view()}
                    {(!would.is_empty()).then(|| view! {
                        <div class="activity-would">
                            <div class="activity-would-label">
                                {if pending { "Its wires would set off:" } else { "Its wires would have set off:" }}
                            </div>
                            {would.iter().map(change_row).collect_view()}
                        </div>
                    })}
                    {pending.then(|| view! {
                        <div class="activity-actions">
                            <button
                                class="btn activity-apply"
                                on:click=move |_| run(state_apply.clone(), error, on_change, api::apply(id))
                            >"Apply"</button>
                            <button
                                class="btn activity-dismiss"
                                on:click=move |_| run(state_dismiss.clone(), error, on_change, api::dismiss(id))
                            >"Dismiss"</button>
                        </div>
                    })}
                    {is_target.then(|| view! {
                        <div class="activity-actions">
                            <button
                                class="btn activity-undo"
                                title="Undo this whole chain"
                                on:click=move |_| run(state_undo.clone(), error, on_change, api::undo())
                            >"Undo"</button>
                        </div>
                    })}
                </div>
            }
        }
    };

    view! {
        <aside class="activity-panel" style:display=move || if open.get() { "flex" } else { "none" }>
            <div class="activity-header">
                <span>"Activity"</span>
                <span class="activity-note">"newest first"</span>
            </div>
            <div class="activity-error">{move || error.get()}</div>
            <div class="activity-list">
                {move || {
                    let entry_view = entry_view.clone();
                    data.with(|d| match d {
                        None => view! { <div class="activity-empty">"Loading…"</div> }.into_any(),
                        Some(d) if d.entries.is_empty() => view! {
                            <div class="activity-empty">
                                "Nothing yet. Status changes made here, and edits made in other apps, show up here."
                            </div>
                        }
                        .into_any(),
                        Some(d) => {
                            let target = d.undo_target;
                            d.entries
                                .iter()
                                .cloned()
                                .map(|e| entry_view(e, target))
                                .collect_view()
                                .into_any()
                        }
                    })
                }}
            </div>
        </aside>
    }
}
