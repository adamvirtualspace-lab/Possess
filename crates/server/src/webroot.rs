//! Where index.html, css/, js/ and vendor/ are read from.
//!
//! Two backends behind one interface: with the `embed` feature the files are
//! baked into the executable at compile time, so a release build ships as a
//! single file. Without it they are read from disk, so editing app.js during
//! development needs no rebuild.

use crate::error::{AppError, AppResult};
use axum::body::Body;
use axum::http::{header, HeaderValue, StatusCode};
use axum::response::{IntoResponse, Response};

/// The only directories reachable from the browser.
///
/// The project directory also holds app.py, the config file and — by default —
/// the vault itself, none of which are the browser's business. Serving a whole
/// directory that happens to contain them is what makes "/js/../app.py" a
/// question worth answering, so each lookup is confined to one of these.
const SERVED_DIRS: &[&str] = &["css", "js", "vendor", "pkg"];

/// The only files at the web root itself that are reachable: the JS app's page
/// and the Rust UI's page.
const SERVED_FILES: &[&str] = &["index.html", "next.html"];

/// Split a web-root path into its served directory and the path within it,
/// rejecting anything that names neither a served file nor a served directory.
fn split_served(path: &str) -> Option<(&str, &str)> {
    match path.split_once('/') {
        None => SERVED_FILES.contains(&path).then_some(("", path)),
        Some((prefix, rest)) => SERVED_DIRS.contains(&prefix).then_some((prefix, rest)),
    }
}

/// A file's contents, or None when it isn't part of the web root.
pub fn read(path: &str) -> Option<Vec<u8>> {
    inner::read(path)
}

/// The cache-busting token for one of our own CSS/JS files.
///
/// On disk this is the file's mtime, so editing a file busts only that file's
/// cache. Embedded files cannot change without a rebuild, so the build's own
/// version stands in.
pub fn version(path: &str) -> i64 {
    inner::version(path)
}

/// Serve a file out of the web root with revalidation forced.
///
/// Browsers fall back to heuristic caching when no Cache-Control is sent, and
/// can keep serving a stale app.js/editor.js from disk cache after we've edited
/// it — even across a hard reload.
pub async fn serve(path: &str) -> AppResult<Response> {
    let bytes = read(path).ok_or_else(|| AppError::not_found(format!("Not found: {path}")))?;

    let media_type = mime_guess::from_path(path)
        .first_raw()
        .unwrap_or("application/octet-stream");

    Ok((
        StatusCode::OK,
        [
            (header::CONTENT_TYPE, HeaderValue::from_static(media_type)),
            (header::CACHE_CONTROL, HeaderValue::from_static("no-cache")),
        ],
        Body::from(bytes),
    )
        .into_response())
}

#[cfg(not(feature = "embed"))]
mod inner {
    use crate::vault;
    use std::path::PathBuf;

    /// Resolve inside the file's own served directory, refusing anything that
    /// climbs out of it — "/js/../app.py" resolves above js/ and is rejected.
    fn safe_join(path: &str) -> Option<PathBuf> {
        let (prefix, rest) = super::split_served(path)?;
        let base = vault::canonical(&vault::root_dir().join(prefix));
        let full = vault::resolve_in_vault(&base, rest).ok()?;
        full.is_file().then_some(full)
    }

    pub fn read(path: &str) -> Option<Vec<u8>> {
        std::fs::read(safe_join(path)?).ok()
    }

    pub fn version(path: &str) -> i64 {
        safe_join(path)
            .map(|p| crate::notes::mtime_secs(&p) as i64)
            .unwrap_or(0)
    }
}

#[cfg(feature = "embed")]
mod inner {
    use rust_embed::Embed;

    // One embed per served directory rather than one rooted at the project, so
    // the build never walks target/ and nothing outside these can be reached.
    #[derive(Embed)]
    #[folder = "../../css"]
    struct Css;

    #[derive(Embed)]
    #[folder = "../../js"]
    struct Js;

    // The vendored editor's full source tree is not served; only the built
    // bundles beside it are, which index.html is what actually loads.
    #[derive(Embed)]
    #[folder = "../../vendor"]
    #[exclude = "simplemde-markdown-editor-fullrepo/*"]
    struct Vendor;

    // The Rust UI's build output. Run ./build-ui.sh before an embed build:
    // rust-embed reads the folder at compile time, so a stale pkg/ ships a
    // stale UI, and a missing one fails the build rather than shipping nothing.
    #[derive(Embed)]
    #[folder = "../../pkg"]
    struct Pkg;

    const INDEX: &[u8] = include_bytes!("../../../index.html");
    const NEXT: &[u8] = include_bytes!("../../../next.html");

    pub fn read(path: &str) -> Option<Vec<u8>> {
        let (prefix, rest) = super::split_served(path)?;
        match (prefix, rest) {
            ("", "index.html") => Some(INDEX.to_vec()),
            ("", "next.html") => Some(NEXT.to_vec()),
            ("css", _) => Css::get(rest).map(|f| f.data.into_owned()),
            ("js", _) => Js::get(rest).map(|f| f.data.into_owned()),
            ("vendor", _) => Vendor::get(rest).map(|f| f.data.into_owned()),
            ("pkg", _) => Pkg::get(rest).map(|f| f.data.into_owned()),
            _ => None,
        }
    }

    pub fn version(_path: &str) -> i64 {
        // Embedded files are fixed for the life of the build, so one token for
        // the whole build is enough to retire a previous version's cache.
        env!("CARGO_PKG_VERSION").bytes().map(|b| b as i64).sum()
    }
}

#[cfg(test)]
mod tests {
    use super::split_served;

    #[test]
    fn accepts_the_served_directories_and_index() {
        assert_eq!(split_served("index.html"), Some(("", "index.html")));
        assert_eq!(split_served("js/app.js"), Some(("js", "app.js")));
        assert_eq!(split_served("css/main.css"), Some(("css", "main.css")));
        assert_eq!(
            split_served("vendor/simplemde.min.js"),
            Some(("vendor", "simplemde.min.js"))
        );
        assert_eq!(split_served("next.html"), Some(("", "next.html")));
        assert_eq!(
            split_served("pkg/possess_ui_bg.wasm"),
            Some(("pkg", "possess_ui_bg.wasm"))
        );
    }

    #[test]
    fn rejects_anything_outside_them() {
        // The project directory holds app.py, the config file and the default
        // vault; none of them are reachable by naming a served directory first.
        assert_eq!(split_served("app.py"), None);
        assert_eq!(split_served("possess-config.json"), None);
        assert_eq!(split_served("notes/private.md"), None);
        assert_eq!(split_served(""), None);
    }

    #[test]
    fn a_traversal_is_left_for_the_path_resolver() {
        // split_served only picks the base; climbing out of it is caught by
        // resolve_in_vault, so these are accepted here and rejected there.
        assert_eq!(split_served("js/../app.py"), Some(("js", "../app.py")));
    }
}
