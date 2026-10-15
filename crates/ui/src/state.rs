//! The state the whole UI shares, and the operations that change it.
//!
//! This replaces the old `app.js` — its `App.state` plus the six `document`
//! CustomEvents it used to coordinate modules (`file-selected`, `vault-changed`,
//! `entry-renamed`, `entry-deleted`, `vault-mutated`, `note-loaded`). Signals
//! replace the events: anything that reads `current_file` re-runs when it
//! changes, so there is nothing to dispatch and nothing to subscribe to.

use crate::api;
use crate::editor::{editor, Editor};
use leptos::prelude::*;
use wasm_bindgen::prelude::*;
use wasm_bindgen::JsCast;
use possess_common::tree::{self, TreeNode};
use possess_markdown::{render, RenderContext};
use std::cell::Cell;
use std::collections::HashSet;
use std::rc::Rc;

/// How long the editor sits idle before the note is written back.
pub const AUTOSAVE_MS: i32 = 5_000;

/// Where the open folders are remembered between visits.
const EXPANDED_KEY: &str = "possess:expanded-folders";

/// How long after the last keystroke in the search box the vault is searched.
///
/// Search walks every note, so a request per keystroke would have long queries
/// racing each other on a large vault.
pub const SEARCH_MS: i32 = 200;

/// How long after the last keystroke the preview is re-rendered.
///
/// Rendering reads the whole document across the editor boundary, so it is
/// worth not doing on every character.
pub const RERENDER_MS: i32 = 300;

/// A timer that replaces its own pending callback.
///
/// Used for autosave and for re-rendering: both should happen once the typing
/// stops, not once per keystroke.
#[derive(Default)]
pub struct Debounce {
    handle: Cell<Option<i32>>,
}

impl Debounce {
    pub fn schedule(&self, delay_ms: i32, task: impl FnOnce() + 'static) {
        self.cancel();

        let closure = Closure::once_into_js(task);
        let handle = web_sys::window().and_then(|window| {
            window
                .set_timeout_with_callback_and_timeout_and_arguments_0(
                    closure.unchecked_ref(),
                    delay_ms,
                )
                .ok()
        });
        self.handle.set(handle);
    }

    pub fn cancel(&self) {
        if let Some(handle) = self.handle.take() {
            if let Some(window) = web_sys::window() {
                window.clear_timeout_with_handle(handle);
            }
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ViewMode {
    Edit,
    Preview,
}

#[derive(Clone)]
pub struct AppState {
    /// Vault-relative path of the open note, if any.
    pub current_file: RwSignal<Option<String>>,
    /// The mtime the server reported for it, used to spot outside edits.
    pub current_mtime: RwSignal<f64>,
    pub view_mode: RwSignal<ViewMode>,
    /// Every note in the vault, which is what wiki links resolve against.
    pub notes: RwSignal<Vec<String>>,
    /// The vault's folders and notes, as the sidebar renders them.
    pub tree: RwSignal<TreeNode>,
    /// Paths of the folders currently open in the sidebar.
    ///
    /// Held here rather than in the DOM, which is what lets an open folder stay
    /// open when the tree is rebuilt after a create or a rename — and, stored
    /// by `remember_expanded`, across a reload.
    pub expanded: RwSignal<HashSet<String>>,
    /// What is typed in the search box; empty means show the tree.
    pub search_query: RwSignal<String>,
    pub search_results: RwSignal<Vec<possess_common::SearchResult>>,
    pub vault_name: RwSignal<String>,
    /// Absolute path of the open vault, which the picker starts from.
    pub vault_path: RwSignal<String>,
    pub vault_open: RwSignal<bool>,
    pub scripts_open: RwSignal<bool>,
    pub preview_html: RwSignal<String>,
    /// Set while the editor holds changes that have not been saved.
    pub dirty: RwSignal<bool>,
    /// The undo offer currently on screen, if any.
    pub undo: RwSignal<Option<crate::undo::Offer>>,
    /// What the context menu is open for, if it is open.
    pub context_menu: RwSignal<Option<crate::context_menu::Target>>,
    /// Notes offered by the `[[` autocomplete; empty means the list is closed.
    pub complete_matches: RwSignal<Vec<String>>,
    /// Which of them is selected.
    pub complete_active: RwSignal<usize>,
}

#[derive(Default)]
struct Timers {
    save: Debounce,
    rerender: Debounce,
    search: Debounce,
    undo: Debounce,
}

thread_local! {
    /// Pending autosave and preview re-render.
    ///
    /// Outside `AppState` for the same reason the editor is: `Rc` is not
    /// `Send`, Leptos stores context thread-safely, and in the browser there
    /// is one of each anyway. A later keystroke replaces the earlier timer
    /// rather than stacking on it.
    static TIMERS: Timers = Timers::default();
}

impl AppState {
    pub fn new() -> Self {
        Self {
            current_file: RwSignal::new(None),
            current_mtime: RwSignal::new(0.0),
            view_mode: RwSignal::new(ViewMode::Edit),
            notes: RwSignal::new(Vec::new()),
            tree: RwSignal::new(TreeNode::default()),
            expanded: RwSignal::new(crate::store::get_lines(EXPANDED_KEY).into_iter().collect()),
            search_query: RwSignal::new(String::new()),
            search_results: RwSignal::new(Vec::new()),
            vault_name: RwSignal::new(String::new()),
            vault_path: RwSignal::new(String::new()),
            vault_open: RwSignal::new(false),
            scripts_open: RwSignal::new(false),
            preview_html: RwSignal::new(String::new()),
            dirty: RwSignal::new(false),
            undo: RwSignal::new(None),
            context_menu: RwSignal::new(None),
            complete_matches: RwSignal::new(Vec::new()),
            complete_active: RwSignal::new(0),
        }
    }

    /// Write the open folders back to storage whenever they change.
    ///
    /// Separate from `new` so the constructor stays free of effects: this needs
    /// a reactive owner, which the root component has and a bare `new` call
    /// would not. The set is sorted on the way out so the stored value does not
    /// churn with the hash order.
    pub fn remember_expanded(&self) {
        let expanded = self.expanded;
        Effect::new(move |_| {
            let mut open: Vec<String> = expanded.get().into_iter().collect();
            open.sort();
            crate::store::set_lines(EXPANDED_KEY, open);
        });
    }

    /// The editor changed. Mark the note dirty and set both timers running.
    ///
    /// This is the hot path — it runs on every keystroke — so it does no work
    /// itself beyond resetting two timers. Neither the document nor the render
    /// crosses the boundary until the typing stops.
    pub fn on_edit(&self) {
        self.dirty.set(true);

        let state = self.clone();
        TIMERS.with(|timers| timers.rerender.schedule(RERENDER_MS, move || {
            if state.view_mode.get_untracked() == ViewMode::Preview {
                state.rerender();
            }
        }));

        let state = self.clone();
        TIMERS.with(|timers| timers.save.schedule(AUTOSAVE_MS, move || {
            leptos::task::spawn_local(async move {
                if let Err(err) = state.save_now().await {
                    leptos::logging::error!("Could not save: {err}");
                }
            });
        }));
    }

    /// Withdraw an undo offer after `delay_ms`, replacing any pending timeout.
    pub fn undo_debounce(&self, delay_ms: i32, task: impl FnOnce() + 'static) {
        TIMERS.with(|timers| timers.undo.schedule(delay_ms, task));
    }

    /// Run `task` once the search box has been quiet for `delay_ms`.
    pub fn search_debounce(&self, delay_ms: i32, task: impl FnOnce() + 'static) {
        TIMERS.with(|timers| timers.search.schedule(delay_ms, task));
    }

    /// The page's editor, which lives outside this struct so that `AppState`
    /// stays `Send` and can be shared through Leptos context.
    pub fn editor(&self) -> Rc<Editor> {
        editor()
    }

    /// Re-render the preview from the editor's current content.
    ///
    /// Reads the document across the boundary, which is why it is called when
    /// something needs the result rather than on every keystroke.
    pub fn rerender(&self) {
        let content = self.editor().content();
        let notes = self.notes.get_untracked();
        let current = self.current_file.get_untracked();

        let context = RenderContext {
            note_path: current.as_deref(),
            notes: &notes,
        };
        self.preview_html.set(render(&content, &context));
    }

    /// Load the vault's note list, which the tree and wiki links both need.
    pub async fn load_notes(&self) -> Result<(), String> {
        let vault = api::get_vault().await?;
        let folders = api::list_folders(None).await?;

        let tree = tree::build(&folders.folders);

        self.vault_name.set(vault.name);
        self.vault_path.set(vault.path);
        // Derived from the tree rather than built twice, so the sidebar and
        // wiki-link resolution can never disagree about what exists.
        self.notes.set(tree.note_paths());
        self.tree.set(tree);
        Ok(())
    }

    /// Open a note, and open the folders on the way down to it.
    ///
    /// The data-level equivalent of walking up the DOM removing `collapsed`:
    /// a note reached from search or a wiki link should be visible in the tree,
    /// not hidden inside a folder that happens to be shut.
    pub async fn open_and_reveal(&self, path: &str) -> Result<(), String> {
        self.open(path).await?;
        self.expanded.update(|open| {
            open.extend(tree::ancestors(path));
        });
        Ok(())
    }

    /// Open a note in the editor.
    pub async fn open(&self, path: &str) -> Result<(), String> {
        let file = api::get_file(path).await?;

        TIMERS.with(|timers| timers.save.cancel());
        self.current_file.set(Some(path.to_string()));
        self.current_mtime.set(file.mtime);
        self.editor().set_content(&file.content);
        self.dirty.set(false);
        self.rerender();
        Ok(())
    }

    /// Write the open note back, now.
    ///
    /// Used where an edit has a side effect already on disk — a pasted image is
    /// written the moment it is pasted — so the link to it should not lag five
    /// seconds behind.
    pub async fn save_now(&self) -> Result<(), String> {
        TIMERS.with(|timers| timers.save.cancel());
        let Some(path) = self.current_file.get_untracked() else {
            return Ok(());
        };
        if !self.dirty.get_untracked() {
            return Ok(());
        }

        let saved = api::save_file(&path, &self.editor().content()).await?;
        self.current_mtime.set(saved.mtime);
        self.dirty.set(false);
        Ok(())
    }

    /// Replace the whole document and save, used by edits the preview makes.
    pub fn apply_and_save(&self, content: String) {
        self.editor().set_content(&content);
        self.dirty.set(true);
        self.rerender();

        let state = self.clone();
        leptos::task::spawn_local(async move {
            if let Err(err) = state.save_now().await {
                leptos::logging::error!("Could not save: {err}");
            }
        });
    }

    /// Close the open note, leaving nothing for the timers to point at.
    #[allow(dead_code)]   // used once the sidebar can delete the open note
    pub fn close(&self) {
        TIMERS.with(|timers| timers.save.cancel());
        self.current_file.set(None);
        self.current_mtime.set(0.0);
        self.editor().set_content("");
        self.dirty.set(false);
        self.preview_html.set(String::new());
    }
}
