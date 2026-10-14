//! Editing a note by clicking the rendered preview.
//!
//! A click on a rendered block hides it and puts a textarea in its place
//! holding the Markdown behind it. Committing writes those bytes back and saves.
//!
//! Two details carry real weight:
//!
//! - The byte range is re-derived from the editor's *current* content when the
//!   edit commits, never from what was on screen when it opened. The note can
//!   change in between — an outside edit, another pane, a script — and writing
//!   back a remembered range would corrupt it.
//! - ArrowUp and ArrowDown move between blocks only when the caret is already
//!   on the first or last *visual* row. A wrapped line has several visual rows
//!   per source line, so that question is answered by measuring a mirror of the
//!   textarea rather than by counting newlines.

use crate::preview::closest;
use crate::state::AppState;
use leptos::prelude::*;
use possess_markdown::source::{self, SourceRange};
use possess_markdown::utf16;
use std::cell::{Cell, RefCell};
use std::rc::Rc;
use wasm_bindgen::prelude::*;
use wasm_bindgen::JsCast;

/// Style properties the caret mirror has to share with the textarea for its
/// measurement to mean anything.
const MIRRORED: &[&str] = &[
    "box-sizing", "width", "padding-top", "padding-right", "padding-bottom",
    "padding-left", "border-top-width", "border-right-width",
    "border-bottom-width", "border-left-width", "font-family", "font-size",
    "font-weight", "font-style", "letter-spacing", "line-height",
    "text-transform", "text-indent", "word-spacing", "tab-size",
];

/// What kind of thing is being edited, which decides how big it opens and how
/// the arrow keys behave.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Kind {
    /// One source line, which may wrap to several visual rows.
    Line,
    /// A whole fenced block, where rows are just newlines.
    Block,
}

/// A DOM listener this session owns, paired with the event it is attached to.
type Listener = (&'static str, Closure<dyn FnMut(web_sys::Event)>);

/// One open editor. Dropping it removes its listeners, so nothing leaks per edit.
struct Session {
    textarea: web_sys::HtmlTextAreaElement,
    source: web_sys::Element,
    kind: Kind,
    /// Which block this edits; re-resolved against current content on commit.
    index: usize,
    /// The line the block starts on, for stepping to a neighbour afterwards.
    start_line: usize,
    finished: Cell<bool>,
    listeners: RefCell<Vec<Listener>>,
}

impl Drop for Session {
    fn drop(&mut self) {
        let target: &web_sys::EventTarget = self.textarea.as_ref();
        for (name, closure) in self.listeners.borrow().iter() {
            let _ = target
                .remove_event_listener_with_callback(name, closure.as_ref().unchecked_ref());
        }
    }
}

fn document() -> web_sys::Document {
    web_sys::window().expect("window").document().expect("document")
}

/// Open an editor on the source line behind `element`.
pub fn edit_line(state: &AppState, element: &web_sys::Element) {
    let Some(line) = attr_usize(element, "data-md-line-index") else { return };
    let content = state.editor().content();
    let Some(range) = source::line_range(&content, line) else { return };
    open(state, element, Kind::Line, line, line, range);
}

/// Open an editor on the whole fenced block behind `element`.
pub fn edit_block(state: &AppState, element: &web_sys::Element) {
    let Some(index) = attr_usize(element, "data-md-block-index") else { return };
    let start_line = attr_usize(element, "data-md-line-index").unwrap_or(0);
    let content = state.editor().content();
    let Some(range) = source::block_range(&content, index) else { return };
    open(state, element, Kind::Block, index, start_line, range);
}

fn open(
    state: &AppState,
    element: &web_sys::Element,
    kind: Kind,
    index: usize,
    start_line: usize,
    range: SourceRange,
) {
    if state.current_file.get_untracked().is_none() || element.has_attribute("hidden") {
        return;
    }

    let document = document();
    let Ok(node) = document.create_element("textarea") else { return };
    let Ok(textarea) = node.dyn_into::<web_sys::HtmlTextAreaElement>() else { return };

    // A fenced block's trailing newline belongs to the block, not the text
    // being edited; it is put back on commit.
    let value = match kind {
        Kind::Line => range.raw.clone(),
        Kind::Block => range.raw.trim_end_matches('\n').to_string(),
    };

    textarea.set_class_name(match kind {
        Kind::Line => "preview-inline-editor",
        Kind::Block => "preview-inline-editor preview-code-editor",
    });
    textarea.set_value(&value);
    let _ = textarea.set_attribute(
        "aria-label",
        match kind {
            Kind::Line => "Edit Markdown line",
            Kind::Block => "Edit Markdown code block",
        },
    );
    let _ = textarea.set_attribute(
        "title",
        match kind {
            Kind::Line => "ArrowUp/ArrowDown at an edge changes line · Ctrl+Enter to save · Escape to cancel",
            Kind::Block => "Edit the complete fenced code block · Ctrl+Enter to save · Escape to cancel",
        },
    );
    textarea.set_rows(match kind {
        Kind::Line => 1,
        Kind::Block => value.lines().count().max(3) as u32,
    });

    let original_height = match_layout(&textarea, element);

    let _ = element.set_attribute("hidden", "");
    if let Some(parent) = element.parent_node() {
        if parent.insert_before(&textarea, Some(element)).is_err() {
            let _ = element.remove_attribute("hidden");
            return;
        }
    } else {
        let _ = element.remove_attribute("hidden");
        return;
    }

    resize(&textarea, original_height);
    let _ = textarea.focus();
    // UTF-16, which is what a textarea's selection offsets count — a char
    // count would land short on any line holding an emoji.
    let end = utf16::len(&value) as u32;
    let _ = textarea.set_selection_range(end, end);

    let session = Rc::new(Session {
        textarea: textarea.clone(),
        source: element.clone(),
        kind,
        index,
        start_line,
        finished: Cell::new(false),
        listeners: RefCell::new(Vec::new()),
    });

    wire(state.clone(), session, original_height);
}

/// Attach the keyboard, input and blur handlers for one editing session.
fn wire(state: AppState, session: Rc<Session>, original_height: f64) {
    // Escape cancels, Ctrl/Cmd+Enter commits, and the arrows commit and step to
    // the neighbouring block when the caret is already at the edge.
    {
        let state = state.clone();
        let mine = session.clone();
        let handler = Closure::wrap(Box::new(move |event: web_sys::Event| {
            let Ok(event) = event.dyn_into::<web_sys::KeyboardEvent>() else { return };
            match event.key().as_str() {
                "Escape" => {
                    event.prevent_default();
                    cancel(&mine);
                }
                "Enter" if event.ctrl_key() || event.meta_key() => {
                    event.prevent_default();
                    commit(&state, &mine, 0);
                }
                "ArrowDown" if at_last_row(&mine) => {
                    event.prevent_default();
                    commit(&state, &mine, 1);
                }
                "ArrowUp" if at_first_row(&mine) => {
                    event.prevent_default();
                    commit(&state, &mine, -1);
                }
                _ => {}
            }
        }) as Box<dyn FnMut(_)>);
        listen(&session, "keydown", handler);
    }

    // Grow with the content so a long edit is not stuck in one row.
    {
        let mine = session.clone();
        let handler = Closure::wrap(Box::new(move |_: web_sys::Event| {
            resize(&mine.textarea, original_height);
        }) as Box<dyn FnMut(_)>);
        listen(&session, "input", handler);
    }

    // Clicking away saves rather than discards, which is what the editor itself
    // does on autosave.
    {
        let state = state.clone();
        let mine = session.clone();
        let handler = Closure::wrap(Box::new(move |_: web_sys::Event| {
            commit(&state, &mine, 0);
        }) as Box<dyn FnMut(_)>);
        listen(&session, "blur", handler);
    }
}

fn listen(session: &Rc<Session>, name: &'static str, handler: Closure<dyn FnMut(web_sys::Event)>) {
    let target: &web_sys::EventTarget = session.textarea.as_ref();
    let _ = target.add_event_listener_with_callback(name, handler.as_ref().unchecked_ref());
    session.listeners.borrow_mut().push((name, handler));
}

/// Put the original element back without writing anything.
fn cancel(session: &Rc<Session>) {
    if session.finished.replace(true) {
        return;
    }
    session.textarea.remove();
    let _ = session.source.remove_attribute("hidden");
}

/// Write the edited text back into the note, then optionally step to the
/// neighbouring block.
fn commit(state: &AppState, session: &Rc<Session>, direction: i32) {
    if session.finished.replace(true) {
        return;
    }

    let root = closest(&session.source, "#preview-panel, .editor-preview-side");
    let edited = session.textarea.value();

    // Re-resolved here, not remembered: the note may have changed since this
    // editor opened, and writing back a stale range would corrupt it.
    let latest = state.editor().content();
    let current = match session.kind {
        Kind::Line => source::line_range(&latest, session.index),
        Kind::Block => source::block_range(&latest, session.index),
    };

    let Some(current) = current else {
        session.textarea.remove();
        let _ = session.source.remove_attribute("hidden");
        return;
    };

    // A fenced block keeps whatever trailing newline it had.
    let replacement = match session.kind {
        Kind::Line => edited.clone(),
        Kind::Block if current.raw.ends_with('\n') => format!("{edited}\n"),
        Kind::Block => edited.clone(),
    };

    let moved_to = step_target(session, &edited, direction);

    if replacement == current.raw {
        // Nothing changed, so leave the document alone and just restore.
        session.textarea.remove();
        let _ = session.source.remove_attribute("hidden");
    } else {
        let updated = format!(
            "{}{replacement}{}",
            &latest[..current.start],
            &latest[current.end..]
        );
        state.apply_and_save(updated);
    }

    if direction != 0 {
        if let Some(root) = root {
            edit_adjacent(state, &root, moved_to, direction);
        }
    }
}

/// Which line the step away from this editor counts as leaving from.
fn step_target(session: &Rc<Session>, edited: &str, direction: i32) -> usize {
    match session.kind {
        Kind::Line => session.start_line,
        // Leaving a block downwards starts from its last line, not its first.
        Kind::Block if direction > 0 => {
            session.start_line + edited.lines().count().saturating_sub(1)
        }
        Kind::Block => session.start_line,
    }
}

/// Open the next editable block above or below `from_line`.
fn edit_adjacent(state: &AppState, root: &web_sys::Element, from_line: usize, direction: i32) {
    let Ok(nodes) = root.query_selector_all("[data-md-line-index]") else { return };

    let mut candidates: Vec<(usize, web_sys::Element)> = Vec::new();
    for index in 0..nodes.length() {
        let Some(element) = nodes.item(index).and_then(|n| n.dyn_into::<web_sys::Element>().ok())
        else {
            continue;
        };
        if element.has_attribute("hidden") {
            continue;
        }
        let Some(line) = attr_usize(&element, "data-md-line-index") else { continue };

        let wanted = if direction > 0 { line > from_line } else { line < from_line };
        if wanted {
            candidates.push((line, element));
        }
    }

    // Nearest in the direction of travel.
    candidates.sort_by_key(|(line, _)| if direction > 0 { *line } else { usize::MAX - *line });

    if let Some((_, element)) = candidates.first() {
        if element.has_attribute("data-md-block-index") {
            edit_block(state, element);
        } else {
            edit_line(state, element);
        }
    }
}

/// Give the textarea the source element's box, so the text does not jump when
/// the editor replaces it. Returns the height it should not shrink below.
fn match_layout(textarea: &web_sys::HtmlTextAreaElement, source: &web_sys::Element) -> f64 {
    let rect = source.get_bounding_client_rect();

    let Some(window) = web_sys::window() else { return rect.height() };
    let Ok(Some(style)) = window.get_computed_style(source) else { return rect.height() };
    let Ok(target) = textarea.clone().dyn_into::<web_sys::HtmlElement>() else {
        return rect.height();
    };

    let _ = target.style().set_property("width", &format!("{}px", rect.width()));
    let _ = target.style().set_property("max-width", "100%");
    for property in ["margin-top", "margin-right", "margin-bottom", "margin-left"] {
        if let Ok(value) = style.get_property_value(property) {
            let _ = target.style().set_property(property, &value);
        }
    }
    rect.height()
}

/// Fit the textarea to its content, never below the height it replaced.
fn resize(textarea: &web_sys::HtmlTextAreaElement, minimum: f64) {
    let Ok(element) = textarea.clone().dyn_into::<web_sys::HtmlElement>() else { return };
    let _ = element.style().set_property("height", "auto");
    let wanted = (textarea.scroll_height() as f64).max(minimum);
    let _ = element.style().set_property("height", &format!("{wanted}px"));
}

/// Whether the caret sits on the first visual row of the textarea.
fn at_first_row(session: &Rc<Session>) -> bool {
    let caret = session.textarea.selection_start().ok().flatten().unwrap_or(0);
    if caret == 0 {
        return true;
    }
    match session.kind {
        // In a code block a row is a line, so no measuring is needed — but the
        // newline's byte offset has to be put in the caret's units first.
        Kind::Block => {
            let value = session.textarea.value();
            value
                .find('\n')
                .is_none_or(|first| caret as usize <= utf16::offset_of(&value, first))
        }
        Kind::Line => match measure_caret(&session.textarea) {
            Some(m) => m.marker_top <= m.padding_top + 3.0,
            None => true,
        },
    }
}

/// Whether the caret sits on the last visual row of the textarea.
fn at_last_row(session: &Rc<Session>) -> bool {
    let caret = session.textarea.selection_start().ok().flatten().unwrap_or(0) as usize;
    match session.kind {
        Kind::Block => {
            let value = session.textarea.value();
            value
                .rfind('\n')
                .is_none_or(|last| caret > utf16::offset_of(&value, last))
        }
        Kind::Line => match measure_caret(&session.textarea) {
            Some(m) => {
                m.marker_top + m.line_height >= m.total_height - m.padding_bottom - 2.0
            }
            None => true,
        },
    }
}

struct CaretRow {
    marker_top: f64,
    line_height: f64,
    padding_top: f64,
    padding_bottom: f64,
    total_height: f64,
}

/// Where the caret's row sits inside the textarea.
///
/// A textarea will not say which visual row the caret is on, so the text up to
/// the caret is laid out again in a hidden div that copies the textarea's
/// metrics, and the position of a marker at the end of it is read off.
fn measure_caret(textarea: &web_sys::HtmlTextAreaElement) -> Option<CaretRow> {
    let window = web_sys::window()?;
    let document = window.document()?;
    let style = window.get_computed_style(textarea).ok()??;

    let mirror = document.create_element("div").ok()?.dyn_into::<web_sys::HtmlElement>().ok()?;
    let marker = document.create_element("span").ok()?.dyn_into::<web_sys::HtmlElement>().ok()?;

    for property in MIRRORED {
        if let Ok(value) = style.get_property_value(property) {
            let _ = mirror.style().set_property(property, &value);
        }
    }
    for (property, value) in [
        ("position", "absolute"),
        ("visibility", "hidden"),
        ("white-space", "pre-wrap"),
        ("overflow-wrap", "break-word"),
        ("top", "-99999px"),
    ] {
        let _ = mirror.style().set_property(property, value);
    }

    let caret = textarea.selection_start().ok().flatten().unwrap_or(0) as usize;
    let value = textarea.value();
    // The caret is a UTF-16 offset; taking that many chars would mirror the
    // wrong amount of text and measure the wrong row.
    mirror.set_text_content(Some(utf16::truncate_at(&value, caret)));

    // A zero-width space, so the marker has a box to measure without changing
    // how the text before it wraps.
    marker.set_text_content(Some("\u{200b}"));
    mirror.append_child(&marker).ok()?;
    document.body()?.append_child(&mirror).ok()?;

    let measured = CaretRow {
        marker_top: marker.offset_top() as f64,
        line_height: parse_px(&style, "line-height")
            .or_else(|| parse_px(&style, "font-size").map(|size| size * 1.55))
            .unwrap_or(16.0),
        padding_top: parse_px(&style, "padding-top").unwrap_or(0.0),
        padding_bottom: parse_px(&style, "padding-bottom").unwrap_or(0.0),
        total_height: mirror.scroll_height() as f64,
    };

    mirror.remove();
    Some(measured)
}

/// A computed length in pixels, if it is one — "normal" line-height is not.
fn parse_px(style: &web_sys::CssStyleDeclaration, property: &str) -> Option<f64> {
    let value = style.get_property_value(property).ok()?;
    value.trim_end_matches("px").parse::<f64>().ok()
}

fn attr_usize(element: &web_sys::Element, name: &str) -> Option<usize> {
    element.get_attribute(name)?.parse().ok()
}
