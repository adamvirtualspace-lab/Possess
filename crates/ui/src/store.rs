//! Browser storage, in the format the rest of the app already uses.
//!
//! Deliberately raw strings rather than JSON. The theme is read by an inline
//! script before the WebAssembly loads, and both frontends share these keys
//! while the port is in progress — a JSON-encoded `"dark"` arrives at that
//! script with its quotes still attached and matches nothing.
//!
//! Every accessor tolerates storage being unavailable: a private window throws
//! on write, and blocked site data throws on read.

pub fn get(key: &str) -> Option<String> {
    web_sys::window()?.local_storage().ok()??.get_item(key).ok()?
}

pub fn set(key: &str, value: &str) {
    if let Some(Ok(Some(storage))) = web_sys::window().map(|w| w.local_storage()) {
        let _ = storage.set_item(key, value);
    }
}

/// A stored number, if there is one and it parses.
pub fn get_f64(key: &str) -> Option<f64> {
    get(key)?.parse().ok()
}

pub fn set_f64(key: &str, value: f64) {
    set(key, &value.to_string());
}
