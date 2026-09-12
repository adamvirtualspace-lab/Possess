//! PossessApp - Markdown Note Taking Backend

mod assets;
mod error;
mod notes;
mod scripts;
mod search;
mod vault;
mod webroot;

use axum::extract::{Query, State};
use axum::response::Html;
use axum::routing::{get, post};
use axum::{Json, Router};
use error::{AppError, AppResult};
use possess_common::{BrowseResponse, DirEntry, SetVaultRequest, VaultInfo};
use serde::Deserialize;
use tower_http::cors::{Any, CorsLayer};
use vault::AppState;

/// The Rust UI, served alongside the JS app rather than replacing it.
///
/// Both pages talk to the same backend, so the two can be compared directly
/// while the port is in progress. They swap once /next reaches parity.
async fn next_index() -> AppResult<Html<String>> {
    let bytes = webroot::read("next.html")
        .ok_or_else(|| AppError::internal("Could not read next.html"))?;
    Ok(Html(String::from_utf8_lossy(&bytes).into_owned()))
}

/// Serve index.html with cache-busting query strings on our own CSS/JS.
///
/// Plain <link>/<script> tags get cached by the browser with no way to know a
/// file changed underneath them. Tagging each with its file's mtime forces a
/// fresh fetch whenever that specific file is edited, without needing a hard
/// refresh — vendor/ is left alone since those never change.
async fn index() -> AppResult<Html<String>> {
    let bytes = webroot::read("index.html")
        .ok_or_else(|| AppError::internal("Could not read index.html"))?;
    let html = String::from_utf8_lossy(&bytes).into_owned();

    let pattern = regex::Regex::new(r#"((?:href|src)=")(/(?:css|js)/[^"]+)(")"#)
        .expect("asset tag pattern is valid");

    let busted = pattern.replace_all(&html, |caps: &regex::Captures| {
        let (prefix, asset_path, suffix) = (&caps[1], &caps[2], &caps[3]);
        let version = webroot::version(asset_path.trim_start_matches('/'));
        format!("{prefix}{asset_path}?v={version}{suffix}")
    });

    Ok(Html(busted.into_owned()))
}

// ── Vault selection ──

async fn get_vault(State(state): State<AppState>) -> Json<VaultInfo> {
    let path = state.vault();
    Json(VaultInfo {
        name: vault_display_name(&path),
        exists: Some(path.is_dir()),
        path: path.display().to_string(),
    })
}

/// The folder's own name, falling back to the whole path for a filesystem root.
fn vault_display_name(path: &std::path::Path) -> String {
    match path.file_name() {
        Some(name) if !name.is_empty() => name.to_string_lossy().to_string(),
        _ => path.display().to_string(),
    }
}

async fn set_vault(
    State(state): State<AppState>,
    Json(body): Json<SetVaultRequest>,
) -> AppResult<Json<VaultInfo>> {
    let raw = body.path.unwrap_or_default().trim().to_string();
    if raw.is_empty() {
        return Err(AppError::bad_request("No folder provided"));
    }

    let target = notes::expand_user(&raw);
    if !target.is_dir() {
        return Err(AppError::not_found(format!("Not a folder: {raw}")));
    }

    let resolved = vault::canonical(&target);
    state.set_vault(resolved.clone());
    vault::save_vault(&resolved);

    println!("[PossessApp] Vault changed to: {}", resolved.display());
    Ok(Json(VaultInfo {
        name: vault_display_name(&resolved),
        // Absent rather than null: POST answers about a folder it has just
        // confirmed is there, which is what the Python version reported too.
        exists: None,
        path: resolved.display().to_string(),
    }))
}

#[derive(Deserialize)]
struct BrowseQuery {
    path: Option<String>,
}

/// List the sub-folders of `path` so the picker can walk the filesystem.
async fn browse(
    State(state): State<AppState>,
    Query(params): Query<BrowseQuery>,
) -> AppResult<Json<BrowseResponse>> {
    let target = match params.path.as_deref().filter(|p| !p.is_empty()) {
        Some(path) => vault::canonical(&notes::expand_user(path)),
        None => state.vault_resolved(),
    };

    if !target.is_dir() {
        return Err(AppError::not_found(format!("Not a folder: {}", target.display())));
    }

    let entries = std::fs::read_dir(&target).map_err(|err| {
        if err.kind() == std::io::ErrorKind::PermissionDenied {
            AppError::forbidden(format!("Permission denied: {}", target.display()))
        } else {
            AppError::not_found(format!("Not a folder: {}", target.display()))
        }
    })?;

    let mut dirs: Vec<(String, DirEntry)> = Vec::new();
    for entry in entries.filter_map(Result::ok) {
        let name = entry.file_name().to_string_lossy().to_string();
        if name.starts_with('.') {
            continue;
        }
        // Individual entries can still fail (locked, broken symlink) even when
        // the parent listed fine — skip those rather than failing the browse.
        match entry.file_type() {
            Ok(kind) if kind.is_dir() => {}
            // A symlink to a directory reports as a symlink here, so fall back
            // to a stat that follows it before deciding it isn't a folder.
            Ok(kind) if kind.is_symlink() && entry.path().is_dir() => {}
            _ => continue,
        }
        let path = entry.path().display().to_string();
        dirs.push((name.to_lowercase(), DirEntry { name, path }));
    }
    dirs.sort_by(|a, b| a.0.cmp(&b.0));

    Ok(Json(BrowseResponse {
        // Null at a filesystem root: there is nowhere further up to go.
        parent: target
            .parent()
            .filter(|p| *p != target.as_path())
            .map(|p| p.display().to_string()),
        path: target.display().to_string(),
        dirs: dirs.into_iter().map(|(_, entry)| entry).collect(),
        drives: vault::windows_drives(),
    }))
}

// ── Static files ──

/// Serve /css, /js and /vendor out of the web root.
///
/// The prefix is re-attached because it is part of the file's path within the
/// web root, and webroot::serve refuses anything that climbs out of it.
async fn static_file(
    axum::extract::Path(rest): axum::extract::Path<String>,
    prefix: &'static str,
) -> AppResult<axum::response::Response> {
    webroot::serve(&format!("{prefix}/{rest}")).await
}

#[tokio::main]
async fn main() {
    let default_vault = vault::default_vault();

    // Restore the saved vault (creating the default one if needed).
    if let Err(err) = std::fs::create_dir_all(&default_vault) {
        eprintln!("[PossessApp] Could not create {}: {err}", default_vault.display());
    }

    let state = AppState::new(vault::load_saved_vault());
    println!("[PossessApp] Vault: {}", state.vault().display());

    let app = Router::new()
        .route("/", get(index))
        .route("/next", get(next_index))
        .route("/api/vault", get(get_vault).post(set_vault))
        .route("/api/browse", get(browse))
        .route("/api/folders", get(notes::list_folders))
        .route("/api/file/*filepath", get(notes::get_file).post(notes::save_file))
        .route("/api/create", post(notes::create_entry))
        .route("/api/rename", post(notes::rename_entry))
        .route("/api/delete", post(notes::delete_entry))
        .route("/api/restore", post(notes::restore_entry))
        .route("/api/status", get(notes::get_status))
        .route("/api/search", get(search::search))
        .route("/api/scripts", get(scripts::list_scripts))
        .route("/api/scripts/run", post(scripts::run_script))
        .route("/api/scripts/hooks", post(scripts::set_hooks))
        .route("/api/scripts/scaffold", post(scripts::scaffold_scripts))
        .route("/api/asset/*filepath", get(assets::get_asset))
        .route("/api/paste-image", post(assets::paste_image))
        // Mounted after the routes: these are catchalls within their prefix.
        .route("/css/*rest", get(|p| static_file(p, "css")))
        .route("/js/*rest", get(|p| static_file(p, "js")))
        .route("/vendor/*rest", get(|p| static_file(p, "vendor")))
        .route("/pkg/*rest", get(|p| static_file(p, "pkg")))
        // CORS for local development
        .layer(
            CorsLayer::new()
                .allow_origin(Any)
                .allow_methods(Any)
                .allow_headers(Any),
        )
        .with_state(state);

    // Loopback by default: the vault picker can browse the filesystem and
    // read/write .md files anywhere, and this server has no authentication.
    // Set POSSESS_HOST=0.0.0.0 to deliberately expose it to your network.
    let host = std::env::var("POSSESS_HOST").unwrap_or_else(|_| "127.0.0.1".to_string());
    let port = std::env::var("POSSESS_PORT")
        .ok()
        .and_then(|p| p.parse::<u16>().ok())
        .unwrap_or(8000);

    println!("[PossessApp] Running on http://localhost:{port}");
    println!("[Hint] Use the vault button in the sidebar to switch folders.");

    let listener = tokio::net::TcpListener::bind((host.as_str(), port))
        .await
        .unwrap_or_else(|err| panic!("[PossessApp] Could not bind {host}:{port}: {err}"));

    axum::serve(listener, app).await.expect("server failed");
}
