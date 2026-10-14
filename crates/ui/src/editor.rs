//! The Rust side of the CodeMirror boundary.
//!
//! Every call here crosses into `js/editor-shim.js`, so the interface is
//! deliberately coarse: whole-document operations rather than per-item ones.
//! The two places that matters are marks, which go across in one batch, and the
//! change notification, which carries no payload so that typing never copies
//! the document across the boundary.

// The boundary is written out in full rather than a call at a time, so the
// pieces the autocomplete and paste handlers will use are here already.
#![allow(dead_code)]

use std::cell::RefCell;
use std::rc::Rc;
use wasm_bindgen::prelude::*;
use wasm_bindgen::JsCast;

#[wasm_bindgen]
extern "C" {
    #[wasm_bindgen(js_namespace = PossessEditor, js_name = init)]
    fn shim_init(host_id: &str, handlers: &JsValue) -> web_sys::Element;

    #[wasm_bindgen(js_namespace = PossessEditor, js_name = getContent)]
    fn shim_get_content() -> String;

    #[wasm_bindgen(js_namespace = PossessEditor, js_name = setContent)]
    fn shim_set_content(text: &str);

    #[wasm_bindgen(js_namespace = PossessEditor, js_name = getCursor)]
    fn shim_get_cursor() -> Vec<u32>;

    #[wasm_bindgen(js_namespace = PossessEditor, js_name = setCursor)]
    fn shim_set_cursor(line: u32, ch: u32);

    #[wasm_bindgen(js_namespace = PossessEditor, js_name = getLine)]
    fn shim_get_line(line: u32) -> String;

    #[wasm_bindgen(js_namespace = PossessEditor, js_name = lineCount)]
    fn shim_line_count() -> u32;

    #[wasm_bindgen(js_namespace = PossessEditor, js_name = replaceRange)]
    fn shim_replace_range(text: &str, from_line: u32, from_ch: u32, to_line: u32, to_ch: u32);

    #[wasm_bindgen(js_namespace = PossessEditor, js_name = replaceSelection)]
    fn shim_replace_selection(text: &str);

    #[wasm_bindgen(js_namespace = PossessEditor, js_name = setMarks)]
    fn shim_set_marks(class_name: &str, ranges: &[u32]);

    #[wasm_bindgen(js_namespace = PossessEditor, js_name = addKeyMap)]
    fn shim_add_key_map(names: Vec<JsValue>, handler: &JsValue);

    #[wasm_bindgen(js_namespace = PossessEditor, js_name = removeKeyMap)]
    fn shim_remove_key_map();

    #[wasm_bindgen(js_namespace = PossessEditor, js_name = cursorCoords)]
    fn shim_cursor_coords() -> Vec<f64>;

    #[wasm_bindgen(js_namespace = PossessEditor, js_name = coordsChar)]
    fn shim_coords_char(x: f64, y: f64) -> Vec<u32>;

    #[wasm_bindgen(js_namespace = PossessEditor, js_name = tokenTypeAt)]
    fn shim_token_type_at(line: u32, ch: u32) -> String;

    #[wasm_bindgen(js_namespace = PossessEditor, js_name = focus)]
    fn shim_focus();

    #[wasm_bindgen(js_namespace = PossessEditor, js_name = refresh)]
    fn shim_refresh();

    #[wasm_bindgen(js_namespace = PossessEditor, js_name = show)]
    fn shim_show();

    #[wasm_bindgen(js_namespace = PossessEditor, js_name = hide)]
    fn shim_hide();

    #[wasm_bindgen(js_namespace = PossessEditor, js_name = refreshImages)]
    fn shim_refresh_images(delay: u32);
}

/// A position in the document.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Pos {
    pub line: u32,
    pub ch: u32,
}

/// One range to mark, in the flat form the shim expects.
#[derive(Debug, Clone, Copy)]
pub struct MarkRange {
    pub line: u32,
    pub start: u32,
    pub end: u32,
}

/// What the editor tells Rust about. Each is a bare notification: the handler
/// pulls whatever state it needs, so a keystroke costs one call rather than one
/// copy of the document.
#[derive(Default)]
pub struct Handlers {
    pub on_cursor: Option<Box<dyn Fn()>>,
    pub on_blur: Option<Box<dyn Fn()>>,
    pub on_paste_image: Option<Box<dyn Fn(web_sys::File)>>,
    /// Renders Markdown for SimpleMDE's own side-by-side pane.
    pub render: Option<Box<dyn Fn(String) -> String>>,
    /// Resolves a vault-relative image path to a URL for inline thumbnails.
    pub asset_url: Option<Box<dyn Fn(String) -> String>>,
}

thread_local! {
    /// The one editor on the page.
    ///
    /// Held here rather than in `AppState` because `Rc` is neither `Send` nor
    /// `Sync`, and Leptos signals are stored thread-safely by default. A
    /// thread local is the honest shape for it: there is exactly one editor,
    /// and WebAssembly in the browser is single-threaded.
    static INSTANCE: RefCell<Option<Rc<Editor>>> = const { RefCell::new(None) };
}

/// The page's editor. Panics before `Editor::init` has run.
pub fn editor() -> Rc<Editor> {
    INSTANCE.with(|slot| slot.borrow().clone().expect("editor is initialised"))
}

/// A callback handed to JavaScript, kept alive on the Rust side.
type JsCallback = Closure<dyn FnMut(JsValue) -> JsValue>;

/// The keymap handler, which is told the name of whichever key fired.
type KeyCallback = Closure<dyn FnMut(String)>;

/// Owns the closures handed to JavaScript.
///
/// Kept alive deliberately rather than leaked with `.forget()`: dropping this
/// invalidates the callbacks, which is what tearing the editor down would need.
pub struct Editor {
    _closures: RefCell<Vec<JsCallback>>,
    _key_handler: RefCell<Option<KeyCallback>>,
    change_handler: RefCell<Option<Box<dyn Fn()>>>,
    /// The CodeMirror wrapper element, for listeners that must sit on it.
    pub wrapper: web_sys::Element,
}

impl Editor {
    /// Build the editor inside `host_id` and wire the callbacks.
    pub fn init(host_id: &str, handlers: Handlers) -> Rc<Self> {
        let object = js_sys::Object::new();
        let mut closures = Vec::new();

        // Each callback is adapted to one JsValue-in, JsValue-out shape so they
        // can share a vector; the unused argument costs nothing.
        let mut install = |name: &str, f: Box<dyn Fn(JsValue) -> JsValue>| {
            let closure: JsCallback = Closure::wrap(Box::new(move |arg: JsValue| f(arg)));
            js_sys::Reflect::set(&object, &name.into(), closure.as_ref().unchecked_ref())
                .expect("set handler");
            closures.push(closure);
        };

        // Always installed. It dispatches through the thread local rather
        // than capturing the editor, which does not exist yet — and by the
        // time an edit can happen, it does.
        install(
            "onChange",
            Box::new(move |_| {
                INSTANCE.with(|slot| {
                    if let Some(editor) = slot.borrow().as_ref() {
                        editor.notify_change();
                    }
                });
                JsValue::UNDEFINED
            }),
        );
        if let Some(f) = handlers.on_cursor {
            install("onCursor", Box::new(move |_| { f(); JsValue::UNDEFINED }));
        }
        if let Some(f) = handlers.on_blur {
            install("onBlur", Box::new(move |_| { f(); JsValue::UNDEFINED }));
        }
        if let Some(f) = handlers.on_paste_image {
            install(
                "onPasteImage",
                Box::new(move |arg| {
                    if let Ok(file) = arg.dyn_into::<web_sys::File>() {
                        f(file);
                    }
                    JsValue::UNDEFINED
                }),
            );
        }
        if let Some(f) = handlers.render {
            install(
                "render",
                Box::new(move |arg| JsValue::from_str(&f(arg.as_string().unwrap_or_default()))),
            );
        }
        if let Some(f) = handlers.asset_url {
            install(
                "assetUrl",
                Box::new(move |arg| JsValue::from_str(&f(arg.as_string().unwrap_or_default()))),
            );
        }

        let wrapper = shim_init(host_id, &object);

        let editor = Rc::new(Self {
            _closures: RefCell::new(closures),
            _key_handler: RefCell::new(None),
            change_handler: RefCell::new(None),
            wrapper,
        });
        INSTANCE.with(|slot| *slot.borrow_mut() = Some(editor.clone()));
        editor
    }

    /// Attach a change handler after construction.
    ///
    /// The editor is built before the state that reacts to it exists, so this
    /// is set in a second step rather than passed to `init`.
    pub fn on_change(&self, handler: impl Fn() + 'static) {
        *self.change_handler.borrow_mut() = Some(Box::new(handler));
    }

    /// Called from the shim's change callback.
    fn notify_change(&self) {
        if let Some(handler) = self.change_handler.borrow().as_ref() {
            handler();
        }
    }

    pub fn content(&self) -> String {
        shim_get_content()
    }

    pub fn set_content(&self, text: &str) {
        shim_set_content(text);
    }

    pub fn cursor(&self) -> Pos {
        let at = shim_get_cursor();
        Pos { line: at.first().copied().unwrap_or(0), ch: at.get(1).copied().unwrap_or(0) }
    }

    pub fn set_cursor(&self, pos: Pos) {
        shim_set_cursor(pos.line, pos.ch);
    }

    pub fn line(&self, line: u32) -> String {
        shim_get_line(line)
    }

    pub fn line_count(&self) -> u32 {
        shim_line_count()
    }

    pub fn replace_range(&self, text: &str, from: Pos, to: Pos) {
        shim_replace_range(text, from.line, from.ch, to.line, to.ch);
    }

    pub fn replace_selection(&self, text: &str) {
        shim_replace_selection(text);
    }

    /// Replace every mark of `class_name` in one call.
    ///
    /// Batched on purpose: marking the wiki links in a document one at a time
    /// would cross the boundary once per link, on every edit.
    pub fn set_marks(&self, class_name: &str, ranges: &[MarkRange]) {
        let mut flat = Vec::with_capacity(ranges.len() * 4);
        for range in ranges {
            flat.extend_from_slice(&[range.line, range.start, range.line, range.end]);
        }
        shim_set_marks(class_name, &flat);
    }

    /// Claim a set of keys while a popup is open, handing back the name of
    /// whichever one fires. Registered only while needed, so these keys behave
    /// normally the rest of the time.
    pub fn set_key_map(&self, names: &[&str], handler: impl Fn(String) + 'static) {
        let closure: KeyCallback = Closure::wrap(Box::new(handler));
        let list: Vec<JsValue> = names.iter().map(|name| JsValue::from_str(name)).collect();
        shim_add_key_map(list, closure.as_ref().unchecked_ref());
        *self._key_handler.borrow_mut() = Some(closure);
    }

    pub fn clear_key_map(&self) {
        shim_remove_key_map();
        *self._key_handler.borrow_mut() = None;
    }

    /// Where the caret is on screen: (left, top, bottom).
    pub fn cursor_coords(&self) -> (f64, f64, f64) {
        let at = shim_cursor_coords();
        (
            at.first().copied().unwrap_or(0.0),
            at.get(1).copied().unwrap_or(0.0),
            at.get(2).copied().unwrap_or(0.0),
        )
    }

    pub fn coords_char(&self, x: f64, y: f64) -> Pos {
        let at = shim_coords_char(x, y);
        Pos { line: at.first().copied().unwrap_or(0), ch: at.get(1).copied().unwrap_or(0) }
    }

    /// Whether this position is inside code.
    ///
    /// The markdown mode types code spans and fenced blocks as "comment";
    /// following a link out of a code sample would create a stray note.
    pub fn is_code_at(&self, pos: Pos) -> bool {
        shim_token_type_at(pos.line, pos.ch).contains("comment")
    }

    pub fn focus(&self) {
        shim_focus();
    }

    pub fn refresh(&self) {
        shim_refresh();
    }

    pub fn show(&self) {
        shim_show();
    }

    pub fn hide(&self) {
        shim_hide();
    }

    pub fn refresh_images(&self, delay: u32) {
        shim_refresh_images(delay);
    }
}
