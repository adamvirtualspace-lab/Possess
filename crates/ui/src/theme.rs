//! Light/dark switching.
//!
//! The chosen theme is one attribute on `<html>` and every colour in
//! `css/main.css` keys off it; nothing else in the app needs to know. The page
//! resolves it before first paint, so this only wires the button and keeps its
//! label honest.

use gloo_storage::{LocalStorage, Storage};
use leptos::prelude::*;

const KEY: &str = "possess-theme";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Theme {
    Dark,
    Light,
}

impl Theme {
    fn as_str(self) -> &'static str {
        match self {
            Theme::Dark => "dark",
            Theme::Light => "light",
        }
    }

    fn other(self) -> Self {
        match self {
            Theme::Dark => Theme::Light,
            Theme::Light => Theme::Dark,
        }
    }
}

/// Whatever the pre-paint script settled on.
fn current() -> Theme {
    let attribute = web_sys::window()
        .and_then(|window| window.document())
        .and_then(|document| document.document_element())
        .and_then(|root| root.get_attribute("data-theme"));

    match attribute.as_deref() {
        Some("light") => Theme::Light,
        _ => Theme::Dark,
    }
}

fn apply(theme: Theme) {
    if let Some(root) = web_sys::window()
        .and_then(|window| window.document())
        .and_then(|document| document.document_element())
    {
        let _ = root.set_attribute("data-theme", theme.as_str());
    }
}

#[component]
pub fn ThemeButton() -> impl IntoView {
    let theme = RwSignal::new(current());

    let toggle = move |_| {
        let next = theme.get_untracked().other();
        apply(next);
        theme.set(next);
        // A private window throws on write; the theme still holds for this
        // session, so the failure is deliberately ignored.
        let _ = LocalStorage::set(KEY, next.as_str());
    };

    let label = move || format!("Switch to {} theme", theme.get().other().as_str());

    view! {
        <button
            class="theme-btn"
            id="btn-theme"
            title=label
            aria-label=label
            on:click=toggle
        ></button>
    }
}

/// Follow the OS while no explicit choice has been made here.
pub fn follow_system() {
    let Some(window) = web_sys::window() else { return };
    let Ok(Some(query)) = window.match_media("(prefers-color-scheme: light)") else {
        return;
    };

    let handler = wasm_bindgen::closure::Closure::wrap(Box::new(move |_: web_sys::Event| {
        // An explicit choice wins; this only tracks the OS until one is made.
        if LocalStorage::get::<String>(KEY).is_ok() {
            return;
        }
        let matches = web_sys::window()
            .and_then(|w| w.match_media("(prefers-color-scheme: light)").ok().flatten())
            .map(|q| q.matches())
            .unwrap_or(false);
        apply(if matches { Theme::Light } else { Theme::Dark });
    }) as Box<dyn FnMut(_)>);

    let _ = query.add_event_listener_with_callback("change", handler.as_ref().unchecked_ref());
    handler.forget();
}

use wasm_bindgen::JsCast;
