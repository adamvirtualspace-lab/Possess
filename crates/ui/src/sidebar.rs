//! The sidebar: the vault's folders and notes, search, and what's selected.
//!
//! Renders the class names `css/sidebar.css` already expects, so the port
//! changes the language and not the design. Two things are worth knowing:
//!
//! - Which folders are open lives in a signal, not in the DOM. The JavaScript
//!   this replaces rebuilt the tree with `innerHTML = ''`, so every folder you
//!   had open collapsed whenever anything was created or renamed. Here the set
//!   outlives the rebuild, and is stored, so it outlives a reload too.
//! - Indentation is inline rather than nested CSS selectors, because a vault
//!   nests arbitrarily deep and a stylesheet cannot follow it down.

use crate::context_menu;
use crate::notes;
use crate::state::{AppState, SEARCH_MS};
use leptos::prelude::*;
use possess_common::tree::{NoteRef, TreeNode};
use possess_common::SearchResult;
use wasm_bindgen::JsCast;

/// Padding for a folder at depth zero.
const INDENT_BASE: usize = 6;
/// Added per level of nesting.
const INDENT_STEP: usize = 13;
/// Notes sit further in than the folder header above them, clearing the arrow.
const FILE_OFFSET: usize = 18;

#[component]
pub fn Sidebar() -> impl IntoView {
    let state = expect_context::<AppState>();

    let vault = state.vault_name;
    let query = state.search_query;
    let tree = state.tree;

    let open_vault = {
        let state = state.clone();
        move |_| state.vault_open.set(true)
    };

    view! {
        <aside class="sidebar">
            <div class="sidebar-header">
                <div class="logo-icon"><span>"P"</span></div>
                <span class="sidebar-title">"PossessApp"</span>
            </div>

            <button class="vault-btn" id="btn-vault" on:click=open_vault>
                <svg class="vault-btn-icon" viewBox="0 0 24 24" aria-hidden="true">
                    <path d="M3 7a2 2 0 0 1 2-2h4l2 2h8a2 2 0 0 1 2 2v8a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2z" />
                </svg>
                <span class="vault-btn-name" id="vault-name">{move || vault.get()}</span>
                <span class="vault-btn-hint">"Change"</span>
            </button>

            <SearchBox />

            <nav id="file-tree">
                <Show
                    when=move || query.with(|q| !q.is_empty())
                    fallback=move || view! { <TreeLevel node=tree.into() depth=0 /> }
                >
                    <SearchResults />
                </Show>
            </nav>
        </aside>
    }
}

/// The search field, debounced, with Escape to clear.
#[component]
fn SearchBox() -> impl IntoView {
    let state = expect_context::<AppState>();
    let query = state.search_query;

    let for_tools = state.clone();
    let run = move |text: String| {
        query.set(text.clone());
        if text.is_empty() {
            state.search_results.set(Vec::new());
            return;
        }

        let state = state.clone();
        let wanted = text.clone();
        state.clone().search_debounce(SEARCH_MS, move || {
            leptos::task::spawn_local(async move {
                match crate::api::search(&wanted).await {
                    Ok(found) => {
                        // A slow answer to a query the user has already replaced
                        // would overwrite the newer results, so drop it.
                        if state.search_query.get_untracked() == wanted {
                            state.search_results.set(found.results);
                        }
                    }
                    Err(err) => leptos::logging::error!("Search failed: {err}"),
                }
            });
        });
    };

    let on_input = {
        let run = run.clone();
        move |event: web_sys::Event| {
            let Some(input) = event
                .target()
                .and_then(|t| t.dyn_into::<web_sys::HtmlInputElement>().ok())
            else {
                return;
            };
            run(input.value().trim().to_string());
        }
    };

    let on_keydown = move |event: web_sys::KeyboardEvent| {
        if event.key() != "Escape" {
            return;
        }
        if let Some(input) = event
            .target()
            .and_then(|t| t.dyn_into::<web_sys::HtmlInputElement>().ok())
        {
            input.set_value("");
        }
        run(String::new());
    };

    // New entries land beside the open note, which is nearly always what is
    // meant; with nothing open they go to the vault root.
    let new_note = {
        let state = for_tools.clone();
        move |_| {
            let state = state.clone();
            leptos::task::spawn_local(async move {
                notes::create_note(&state, &notes::current_folder(&state)).await;
            });
        }
    };
    let new_folder = {
        let state = for_tools.clone();
        move |_| {
            let state = state.clone();
            leptos::task::spawn_local(async move {
                notes::create_folder(&state, &notes::current_folder(&state)).await;
            });
        }
    };
    let open_scripts = {
        let state = for_tools.clone();
        move |_| state.scripts_open.set(true)
    };

    view! {
        <div class="sidebar-tools">
            <div class="search-row">
                <input
                    type="search"
                    id="file-search"
                    spellcheck="false"
                    placeholder="Search notes..."
                    aria-label="Search notes"
                    on:input=on_input
                    on:keydown=on_keydown
                />
            </div>
            <div class="tool-row">
                <button class="tool-btn" id="btn-new-note" title="New note" on:click=new_note>
                    "+ Note"
                </button>
                <button class="tool-btn" id="btn-new-folder" title="New folder" on:click=new_folder>
                    "+ Folder"
                </button>
                <button class="tool-btn" id="btn-scripts" title="Python scripts" on:click=open_scripts>
                    "Scripts"
                </button>
            </div>
        </div>
    }
}

/// Search hits, which replace the tree while a query is live.
#[component]
fn SearchResults() -> impl IntoView {
    let state = expect_context::<AppState>();
    let results = state.search_results;
    let query = state.search_query;

    view! {
        <Show
            when=move || !results.with(Vec::is_empty)
            fallback=move || {
                view! {
                    <div class="search-empty">
                        {move || format!("No notes matching \"{}\"", query.get())}
                    </div>
                }
            }
        >
            <For
                each=move || results.get()
                key=|result: &SearchResult| result.path.clone()
                children=move |result| view! { <ResultItem result=result /> }
            />
        </Show>
    }
}

#[component]
fn ResultItem(result: SearchResult) -> impl IntoView {
    let state = expect_context::<AppState>();
    let current = state.current_file;

    let path = result.path.clone();
    let mine = result.path.clone();
    let snippet = result.snippet.clone();

    let on_contextmenu = {
        let state = state.clone();
        let path = result.path.clone();
        let name = result.name.clone();
        move |event: web_sys::MouseEvent| {
            context_menu::open_for(&state, &event, "note", &path, &name);
        }
    };

    view! {
        <div
            class="file-item search-result"
            class:active=move || current.get().as_deref() == Some(mine.as_str())
            data-path=result.path.clone()
            title=result.path.clone()
            on:click=move |_| open(&state, &path)
            on:contextmenu=on_contextmenu
        >
            <div class="search-result-name">{result.name.clone()}</div>
            <Show when=move || !snippet.is_empty()>
                <div class="search-result-snippet">{result.snippet.clone()}</div>
            </Show>
        </div>
    }
}

/// One level of the tree: its folders, then its notes.
///
/// Returns an erased view because this and `Folder` call each other, and a pair
/// of `impl IntoView` types that contain one another cannot be resolved. The
/// nesting itself has to stay: `.folder-children.collapsed` hides a folder's
/// contents through the wrapper element, so a flattened list would have nothing
/// to hide.
#[component]
fn TreeLevel(node: Signal<TreeNode>, depth: usize) -> AnyView {
    view! {
        <For
            each=move || node.with(|n| n.folders.clone())
            key=|folder: &TreeNode| folder.path.clone()
            children=move |folder| view! { <Folder node=folder depth=depth /> }
        />
        <For
            each=move || node.with(|n| n.notes.clone())
            key=|note: &NoteRef| note.path.clone()
            children=move |note| view! { <Note note=note depth=depth /> }
        />
    }
    .into_any()
}

/// A folder header and its contents, collapsible.
#[component]
fn Folder(node: TreeNode, depth: usize) -> impl IntoView {
    let state = expect_context::<AppState>();
    let expanded = state.expanded;

    let toggled = node.path.clone();

    // Collapsed is the default: an unopened folder is simply absent from the
    // set. Built twice because the header and the children each need one and a
    // closure capturing the path is not `Copy`.
    let collapsed_for = |path: String| {
        move || expanded.with(|open| !open.contains(path.as_str()))
    };
    let header_collapsed = collapsed_for(node.path.clone());
    let children_collapsed = collapsed_for(node.path.clone());

    let on_click = move |_| {
        expanded.update(|open| {
            if !open.remove(&toggled) {
                open.insert(toggled.clone());
            }
        });
    };

    let on_contextmenu = {
        let state = state.clone();
        let path = node.path.clone();
        let name = node.name.clone();
        move |event: web_sys::MouseEvent| {
            context_menu::open_for(&state, &event, "folder", &path, &name);
        }
    };

    let children = RwSignal::new(node.clone());

    view! {
        <div class="folder-group">
            <div
                class="folder-header"
                class:collapsed=header_collapsed
                style:padding-left=format!("{}px", INDENT_BASE + depth * INDENT_STEP)
                title=node.path.clone()
                data-path=node.path.clone()
                on:click=on_click
                on:contextmenu=on_contextmenu
            >
                <span class="arrow">"\u{25BC}"</span>
                <span class="folder-name">{node.name.clone()}</span>
            </div>
            <div class="folder-children" class:collapsed=children_collapsed>
                <TreeLevel node=children.into() depth=depth + 1 />
            </div>
        </div>
    }
}

#[component]
fn Note(note: NoteRef, depth: usize) -> impl IntoView {
    let state = expect_context::<AppState>();
    let current = state.current_file;

    let path = note.path.clone();
    let mine = note.path.clone();

    let on_contextmenu = {
        let state = state.clone();
        let path = note.path.clone();
        let name = note.name.clone();
        move |event: web_sys::MouseEvent| {
            context_menu::open_for(&state, &event, "note", &path, &name);
        }
    };

    view! {
        <div
            class="file-item"
            class:active=move || current.get().as_deref() == Some(mine.as_str())
            style:padding-left=format!(
                "{}px",
                INDENT_BASE + depth * INDENT_STEP + FILE_OFFSET,
            )
            data-path=note.path.clone()
            title=note.path.clone()
            on:click=move |_| open(&state, &path)
            on:contextmenu=on_contextmenu
        >
            {note.name.clone()}
        </div>
    }
}

/// Open a note, revealing it in the tree, and scroll it into view.
fn open(state: &AppState, path: &str) {
    let state = state.clone();
    let path = path.to_string();

    leptos::task::spawn_local(async move {
        if let Err(err) = state.open_and_reveal(&path).await {
            leptos::logging::error!("Could not open note: {err}");
            return;
        }
        // After the reveal, so the element exists to scroll to.
        scroll_into_view(&path);
    });
}

/// Bring the selected note into view.
///
/// Found by comparing the data attribute rather than interpolating the path into
/// a selector — real note names contain quotes and brackets often enough.
fn scroll_into_view(path: &str) {
    let Some(document) = web_sys::window().and_then(|window| window.document()) else {
        return;
    };
    let Ok(items) = document.query_selector_all(".file-item[data-path]") else {
        return;
    };

    for index in 0..items.length() {
        let Some(element) = items.item(index).and_then(|n| n.dyn_into::<web_sys::Element>().ok())
        else {
            continue;
        };
        if element.get_attribute("data-path").as_deref() == Some(path) {
            let options = web_sys::ScrollIntoViewOptions::new();
            options.set_block(web_sys::ScrollLogicalPosition::Nearest);
            element.scroll_into_view_with_scroll_into_view_options(&options);
            return;
        }
    }
}
