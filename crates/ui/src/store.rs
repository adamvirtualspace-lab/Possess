//! Browser storage, in the format the rest of the app already uses.
//!
//! Deliberately raw strings rather than JSON. The theme is read by an inline
//! script before the WebAssembly loads — a JSON-encoded `"dark"` arrives at
//! that script with its quotes still attached and matches nothing. These are
//! also the keys the JavaScript frontend wrote, so settings made before the
//! port survived it.
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

/// A stored set of paths.
///
/// One path per line, because that is the one character a path cannot contain
/// on any platform the app runs on — and keeping it raw means a value can be
/// read straight out of the browser's storage inspector.
pub fn get_lines(key: &str) -> Vec<String> {
    get(key)
        .map(|stored| {
            stored
                .lines()
                .filter(|line| !line.is_empty())
                .map(str::to_string)
                .collect()
        })
        .unwrap_or_default()
}

pub fn set_lines(key: &str, values: impl IntoIterator<Item = String>) {
    let joined: Vec<String> = values.into_iter().collect();
    set(key, &joined.join("\n"));
}
