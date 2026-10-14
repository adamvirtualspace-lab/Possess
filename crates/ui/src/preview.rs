//! The preview panel: rendered Markdown you can interact with.
//!
//! Three things in the rendered HTML do something when clicked, and all three
//! are delegated from `document` rather than bound to the panel. That is
//! deliberate: SimpleMDE's own side-by-side pane is created and destroyed as
//! the mode toggles, and delegation covers it without re-binding.
//!
//! - a task checkbox writes its state back to the Markdown
//! - a wiki link opens the note it points at
//! - any block carrying a source line opens an editor on exactly those bytes

use crate::inline_edit;
use crate::state::AppState;
use leptos::prelude::*;
use possess_markdown::{source, wikilinks};
use wasm_bindgen::prelude::*;
use wasm_bindgen::JsCast;

/// Wire the delegated handlers. Called once, at start-up.
pub fn wire(state: AppState) {
    let document = web_sys::window().expect("window").document().expect("document");

    // ── Task checkboxes ──
    {
        let state = state.clone();
        let handler = Closure::wrap(Box::new(move |event: web_sys::Event| {
            let Some(input) = closest_input(&event, "input.interactive-task-checkbox") else {
                return;
            };
            toggle_task(&state, &input);
        }) as Box<dyn FnMut(_)>);

        document
            .add_event_listener_with_callback("change", handler.as_ref().unchecked_ref())
            .expect("listen for change");
        handler.forget();
    }

    // ── Clicks: wiki links, then blocks ──
    {
        let state = state.clone();
        let handler = Closure::wrap(Box::new(move |event: web_sys::Event| {
            let Some(target) = event.target().and_then(|t| t.dyn_into::<web_sys::Element>().ok())
            else {
                return;
            };

            // A wiki link opens a note rather than navigating, so it wins over
            // the block handler underneath it.
            if let Some(link) = closest(&target, "a[data-wikilink]") {
                event.prevent_default();
                let path = link.get_attribute("data-wikilink").unwrap_or_default();
                open_wikilink(&state, &path);
                return;
            }

            // Anything the reader might be trying to use is not a click into
            // the source.
            if closest(&target, "a, button, input, img, textarea").is_some() {
                return;
            }

            if let Some(block) = closest(&target, "[data-md-block-index]") {
                inline_edit::edit_block(&state, &block);
                return;
            }
            if let Some(line) = closest(&target, "[data-md-line-index]") {
                inline_edit::edit_line(&state, &line);
            }
        }) as Box<dyn FnMut(_)>);

        document
            .add_event_listener_with_callback("click", handler.as_ref().unchecked_ref())
            .expect("listen for click");
        handler.forget();
    }
}

/// Write a checkbox's new state back into the Markdown.
fn toggle_task(state: &AppState, input: &web_sys::HtmlInputElement) {
    if state.current_file.get_untracked().is_none() {
        return;
    }

    let index = input
        .get_attribute("data-task-index")
        .and_then(|value| value.parse::<usize>().ok());
    let Some(index) = index else { return };

    let content = state.editor().content();
    match source::set_task_checked(&content, index, input.checked()) {
        Some(updated) => state.apply_and_save(updated),
        // The preview and the editor have fallen out of step; restore what is
        // on screen rather than writing somewhere arbitrary.
        None => input.set_checked(!input.checked()),
    }
}

/// Follow a wiki link, creating the note when it does not exist yet.
fn open_wikilink(state: &AppState, path: &str) {
    let notes = state.notes.get_untracked();
    let current = state.current_file.get_untracked();

    if let Some(found) = wikilinks::resolve(path, current.as_deref(), &notes) {
        let state = state.clone();
        leptos::task::spawn_local(async move {
            if let Err(err) = state.open(&found).await {
                leptos::logging::error!("Could not open note: {err}");
            }
        });
        return;
    }

    // Nothing matches, so offer to create it where the link says — beside the
    // note that links to it when the link carries a folder, else at the root.
    let target = if path.ends_with(".md") {
        path.to_string()
    } else {
        format!("{path}.md")
    };
    let confirmed = web_sys::window()
        .and_then(|w| w.confirm_with_message(&format!("Create {target}?")).ok())
        .unwrap_or(false);
    if !confirmed {
        return;
    }

    let state = state.clone();
    leptos::task::spawn_local(async move {
        let (parent, name) = match target.rsplit_once('/') {
            Some((parent, name)) => (parent.to_string(), name.to_string()),
            None => (String::new(), target.clone()),
        };

        match crate::api::create("note", &parent, &name).await {
            Ok(created) => {
                if let Err(err) = state.load_notes().await {
                    leptos::logging::error!("Could not reload notes: {err}");
                }
                if let Err(err) = state.open(&created.path).await {
                    leptos::logging::error!("Could not open new note: {err}");
                }
            }
            Err(err) => leptos::logging::error!("Could not create note: {err}"),
        }
    });
}

/// The nearest ancestor of the event target matching `selector`, as an input.
fn closest_input(event: &web_sys::Event, selector: &str) -> Option<web_sys::HtmlInputElement> {
    let target = event.target()?.dyn_into::<web_sys::Element>().ok()?;
    closest(&target, selector)?.dyn_into().ok()
}

/// `Element.closest`, which is what the JavaScript used for the same job.
pub fn closest(element: &web_sys::Element, selector: &str) -> Option<web_sys::Element> {
    element.closest(selector).ok().flatten()
}
