//! PossessApp browser UI.
//!
//! Served at /next while the original JS frontend keeps serving /, so the two
//! can be compared side by side against the same backend until this one
//! reaches parity.

mod api;
mod context_menu;
mod editor;
mod inline_edit;
mod linkcomplete;
mod modal;
mod notes;
mod preview;
mod scripts;
mod sidebar;
mod state;
mod theme;
mod undo;
mod vault;

use editor::{Editor, Handlers};
use context_menu::ContextMenu;
use linkcomplete::LinkComplete;
use leptos::prelude::*;
use possess_markdown::{assets, render, RenderContext};
use scripts::ScriptsPanel;
use sidebar::Sidebar;
use theme::ThemeButton;
use undo::UndoBar;
use vault::VaultPicker;
use state::{AppState, ViewMode};
use std::cell::RefCell;
use std::rc::Rc;
use wasm_bindgen::prelude::*;
use wasm_bindgen::JsCast;

#[wasm_bindgen(start)]
pub fn start() {
    // Without this a panic in WASM is an unhelpful "unreachable executed".
    console_error_panic_hook::set_once();

    let root = web_sys::window()
        .and_then(|window| window.document())
        .and_then(|document| document.get_element_by_id("app"))
        .and_then(|element| element.dyn_into::<web_sys::HtmlElement>().ok())
        .expect("#app is in the page");

    leptos::mount::mount_to(root, App).forget();
}

/// What the editor's own callbacks need to know about the open note.
///
/// The editor is built before the state that tracks the open note exists, and
/// its callbacks outlive any one note. This cell breaks that circle: the
/// callbacks read it, and an effect keeps it current.
type NoteContext = Rc<RefCell<(Option<String>, Vec<String>)>>;

thread_local! {
    static RENDER_CONTEXT: RefCell<Option<NoteContext>> = const { RefCell::new(None) };
}

#[component]
fn App() -> impl IntoView {
    let state = AppState::new();
    provide_context(state.clone());
    track_note_context(state.clone());

    // Deferred to an effect because the editor is built into an element the
    // shell renders, which does not exist until after this view is mounted.
    // SimpleMDE replaces that element with its own DOM, so this must happen
    // exactly once.
    Effect::new(move |previous: Option<()>| {
        if previous.is_some() {
            return;
        }
        build_editor(state.clone());

        // The change notification carries no payload; the state pulls what it
        // needs once the typing stops.
        let for_change = state.clone();
        editor::editor().on_change(move || for_change.on_edit());

        preview::wire(state.clone());
        theme::follow_system();

        // Load the vault's notes, then open the first one so there is
        // something on screen. The real sidebar replaces this list.
        let state = state.clone();
        leptos::task::spawn_local(async move {
            if let Err(err) = state.load_notes().await {
                leptos::logging::error!("Could not load notes: {err}");
                return;
            }
            let first = state.notes.get_untracked().first().cloned();
            if let Some(first) = first {
                if let Err(err) = state.open_and_reveal(&first).await {
                    leptos::logging::error!("Could not open note: {err}");
                }
            }
        });
    });

    view! { <Shell /> }
}

/// The chrome around the editor and preview: note list, title, mode buttons.
#[component]
fn Shell() -> impl IntoView {
    let state = expect_context::<AppState>();

    let set_mode = {
        let state = state.clone();
        move |mode: ViewMode| {
            state.view_mode.set(mode);
            match mode {
                ViewMode::Preview => {
                    state.rerender();
                    state.editor().hide();
                }
                ViewMode::Edit => state.editor().show(),
            }
        }
    };
    let set_edit = set_mode.clone();

    let current = state.current_file;
    let mode = state.view_mode;
    let html = state.preview_html;

    view! {
        <Sidebar />
        <LinkComplete />
        <ContextMenu />
        <UndoBar />
        <VaultPicker />
        <ScriptsPanel />

        <section class="main-content">
            <div class="top-bar">
                <span id="current-file-name">
                    {move || current.get().unwrap_or_else(|| "No file selected".into())}
                </span>
                <div class="btn-group">
                    <ThemeButton />
                    <button
                        class="btn"
                        class:active=move || mode.get() == ViewMode::Edit
                        on:click=move |_| set_edit(ViewMode::Edit)
                    >
                        "Edit"
                    </button>
                    <button
                        class="btn"
                        class:active=move || mode.get() == ViewMode::Preview
                        on:click=move |_| set_mode(ViewMode::Preview)
                    >
                        "Preview"
                    </button>
                </div>
            </div>

            <div class="viewport">
                // The editor is built into this host after mount. Rendered
                // once, with no reactive children, so Leptos never touches the
                // DOM SimpleMDE puts inside it.
                <div id="editor-host"></div>
                <div
                    id="preview-panel"
                    class="markdown-preview"
                    style:display=move || {
                        if mode.get() == ViewMode::Preview { "block" } else { "none" }
                    }
                    inner_html=move || html.get()
                ></div>
            </div>
        </section>
    }
}

/// Build the CodeMirror editor and hand it the callbacks it needs.
///
/// `render` and `asset_url` are the two places JavaScript asks Rust a question:
/// SimpleMDE's own side-by-side pane renders through the same renderer as the
/// preview, and inline image thumbnails resolve paths through the same rules —
/// so neither has a second implementation that could drift.
fn build_editor(state: AppState) {
    let context: NoteContext = Rc::new(RefCell::new((None, Vec::new())));
    let for_render = context.clone();
    let for_assets = context.clone();

    Editor::init(
        "editor-host",
        Handlers {
            render: Some(Box::new(move |text| {
                let borrowed = for_render.borrow();
                render(
                    &text,
                    &RenderContext { note_path: borrowed.0.as_deref(), notes: &borrowed.1 },
                )
            })),
            asset_url: Some(Box::new(move |src| {
                let borrowed = for_assets.borrow();
                assets::url(&src, borrowed.0.as_deref())
            })),
            // Moving the cursor re-evaluates whether a `[[` is being typed;
            // losing focus closes the list.
            on_cursor: Some(Box::new({
                let state = state.clone();
                move || linkcomplete::update(&state)
            })),
            on_blur: Some(Box::new({
                let state = state.clone();
                move || linkcomplete::close(&state)
            })),
            ..Default::default()
        },
    );

    RENDER_CONTEXT.with(|slot| *slot.borrow_mut() = Some(context));
}

/// Keep the editor's callbacks in step with the open note and the note list.
fn track_note_context(state: AppState) {
    Effect::new(move |_| {
        let current = state.current_file.get();
        let notes = state.notes.get();
        RENDER_CONTEXT.with(|slot| {
            if let Some(context) = slot.borrow().as_ref() {
                *context.borrow_mut() = (current, notes);
            }
        });
    });
}
