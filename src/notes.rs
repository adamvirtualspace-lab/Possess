//! Note and folder CRUD: the endpoints the sidebar and editor live on.

use crate::error::{AppError, AppResult};
use crate::scripts;
use crate::vault::{self, AppState};
use axum::extract::{Path as UrlPath, Query, State};
use axum::Json;
use chrono::Local;
use serde::Deserialize;
use serde_json::{json, Map, Value};
use std::path::{Path, PathBuf};

/// Deletes move here instead of being unlinked, so "delete folder" is a
/// recoverable action rather than a permanent one. Dot-prefixed, so the sidebar
/// walk already skips it.
pub const TRASH_DIRNAME: &str = ".trash";
pub const TRASH_MANIFEST: &str = "manifest.json";

pub fn trash_dir(vault: &Path) -> PathBuf {
    vault.join(TRASH_DIRNAME)
}

/// A file's mtime as seconds since the epoch.
///
/// The client stores this as a JavaScript number and /api/status compares it
/// back with a 0.01s tolerance, so it has to be the same f64 the Python version
/// produced — not a truncated whole-second value, or the 5s sync poll reports a
/// change on every tick.
pub fn mtime_secs(path: &Path) -> f64 {
    path.metadata()
        .and_then(|m| m.modified())
        .ok()
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|d| d.as_secs_f64())
        .unwrap_or(0.0)
}

/// Move a file or directory, falling back to copy-then-delete.
///
/// fs::rename fails with EXDEV across filesystems. The trash lives inside the
/// vault so the common case is a plain rename, but a vault spanning a mount
/// point would break delete and restore outright without this fallback.
fn move_path(src: &Path, dest: &Path) -> AppResult<()> {
    if std::fs::rename(src, dest).is_ok() {
        return Ok(());
    }

    if src.is_dir() {
        let options = fs_extra::dir::CopyOptions::new().copy_inside(true);
        fs_extra::dir::move_dir(src, dest, &options)
            .map_err(|e| AppError::internal(format!("Could not move {}: {e}", src.display())))?;
    } else {
        std::fs::copy(src, dest)?;
        std::fs::remove_file(src)?;
    }
    Ok(())
}

fn is_hidden(name: &std::ffi::OsStr) -> bool {
    name.to_string_lossy().starts_with('.')
}

fn is_markdown(name: &std::ffi::OsStr) -> bool {
    name.to_string_lossy().ends_with(".md")
}

/// Walk the vault, skipping dot-directories.
///
/// An Obsidian vault keeps its config and plugins in .obsidian/ (and deleted
/// notes in .trash/), none of which are the user's notes.
pub fn walk_notes(base: &Path) -> impl Iterator<Item = walkdir::DirEntry> {
    walkdir::WalkDir::new(base)
        .into_iter()
        .filter_entry(|e| e.depth() == 0 || !is_hidden(e.file_name()))
        .filter_map(Result::ok)
        .filter(|e| e.file_type().is_file() && is_markdown(e.file_name()))
}

#[derive(Deserialize)]
pub struct RootQuery {
    root: Option<String>,
}

pub async fn list_folders(
    State(state): State<AppState>,
    Query(params): Query<RootQuery>,
) -> AppResult<Json<Value>> {
    let base = match params.root.as_deref().filter(|r| !r.is_empty()) {
        Some(root) => vault::canonical(&expand_user(root)),
        None => state.vault_resolved(),
    };

    if !base.is_dir() {
        return Err(AppError::not_found(format!(
            "Directory not found: {}",
            base.display()
        )));
    }

    // Keyed by the folder's path relative to the base, listing only folders
    // that directly hold .md files. The client splits those keys to rebuild the
    // hierarchy, so intermediate folders need no entry of their own.
    let mut folders: Map<String, Value> = Map::new();

    for entry in walk_notes(&base) {
        let full = entry.path();
        let parent = full.parent().unwrap_or(&base);

        let rel = match parent.strip_prefix(&base) {
            Ok(p) if p.as_os_str().is_empty() => ".".to_string(),
            // Normalised to forward slashes so the client builds one consistent
            // path shape regardless of platform.
            Ok(p) => p.components()
                .map(|c| c.as_os_str().to_string_lossy())
                .collect::<Vec<_>>()
                .join("/"),
            Err(_) => continue,
        };

        let name = entry.file_name().to_string_lossy().to_string();
        folders
            .entry(rel)
            .or_insert_with(|| Value::Array(Vec::new()))
            .as_array_mut()
            .expect("folder entry is an array")
            .push(json!([name, full.display().to_string()]));
    }

    // os.walk yielded filenames sorted per directory; walkdir does not promise
    // an order, so sort each folder's notes to keep the tree stable.
    for files in folders.values_mut() {
        if let Some(list) = files.as_array_mut() {
            list.sort_by(|a, b| a[0].as_str().unwrap_or("").cmp(b[0].as_str().unwrap_or("")));
        }
    }

    Ok(Json(json!({ "base": base.display().to_string(), "folders": folders })))
}

pub async fn get_file(
    State(state): State<AppState>,
    UrlPath(filepath): UrlPath<String>,
) -> AppResult<Json<Value>> {
    let full = state.resolve(&filepath)?;

    if !full.is_file() {
        return Err(AppError::not_found(format!("File not found: {filepath}")));
    }

    let content = std::fs::read_to_string(&full)?;
    Ok(Json(json!({
        "filename": filepath,
        "content": content,
        "mtime": mtime_secs(&full),
    })))
}

#[derive(Deserialize)]
pub struct SaveBody {
    #[serde(default)]
    content: String,
}

pub async fn save_file(
    State(state): State<AppState>,
    UrlPath(filepath): UrlPath<String>,
    Json(body): Json<SaveBody>,
) -> AppResult<Json<Value>> {
    let full = state.resolve(&filepath)?;

    if let Some(parent) = full.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(&full, &body.content)?;
    let mtime = mtime_secs(&full);

    scripts::run_save_hooks(&state, &filepath).await;

    Ok(Json(json!({ "saved": true, "mtime": mtime })))
}

#[derive(Deserialize)]
pub struct CreateBody {
    kind: Option<String>,
    parent: Option<String>,
    name: Option<String>,
    #[serde(default)]
    content: String,
}

/// Create a new note or folder inside the vault.
///
/// The client sends a parent folder plus a name; both are resolved through
/// resolve_in_vault so a crafted name like "../escape" can't leave the vault.
pub async fn create_entry(
    State(state): State<AppState>,
    Json(body): Json<CreateBody>,
) -> AppResult<Json<Value>> {
    let kind = body.kind.unwrap_or_default().trim().to_string();
    let kind = if kind.is_empty() { "note".to_string() } else { kind };
    let parent = trim_rel(body.parent.as_deref().unwrap_or(""));
    let mut name = body.name.unwrap_or_default().trim().to_string();

    if name.is_empty() {
        return Err(AppError::bad_request("No name provided"));
    }
    if name.contains('/') || name.contains('\\') {
        return Err(AppError::bad_request("Name cannot contain path separators"));
    }
    if kind == "note" && !name.ends_with(".md") {
        name.push_str(".md");
    }

    let rel = if parent.is_empty() { name.clone() } else { format!("{parent}/{name}") };
    let full = state.resolve(&rel)?;

    if full.exists() {
        return Err(AppError::conflict(format!("Already exists: {rel}")));
    }

    if kind == "folder" {
        std::fs::create_dir_all(&full)?;
    } else {
        if let Some(dir) = full.parent() {
            std::fs::create_dir_all(dir)?;
        }
        std::fs::write(&full, &body.content)?;
    }

    Ok(Json(json!({ "path": rel, "kind": kind })))
}

#[derive(Deserialize)]
pub struct RenameBody {
    path: Option<String>,
    name: Option<String>,
}

/// Move/rename a note or folder within the vault.
pub async fn rename_entry(
    State(state): State<AppState>,
    Json(body): Json<RenameBody>,
) -> AppResult<Json<Value>> {
    let src_rel = trim_rel(body.path.as_deref().unwrap_or(""));
    let mut name = body.name.unwrap_or_default().trim().to_string();

    if src_rel.is_empty() || name.is_empty() {
        return Err(AppError::bad_request("Both path and name are required"));
    }
    if name.contains('/') || name.contains('\\') {
        return Err(AppError::bad_request("Name cannot contain path separators"));
    }

    let src = state.resolve(&src_rel)?;
    if !src.exists() {
        return Err(AppError::not_found(format!("Not found: {src_rel}")));
    }

    if src.is_file() && !name.ends_with(".md") {
        name.push_str(".md");
    }

    let parent_rel = match src_rel.rsplit_once('/') {
        Some((parent, _)) => parent.to_string(),
        None => String::new(),
    };
    let dest_rel = if parent_rel.is_empty() { name.clone() } else { format!("{parent_rel}/{name}") };
    let dest = state.resolve(&dest_rel)?;

    // Case-only renames land on the same inode on case-insensitive filesystems,
    // so only treat a *different* existing path as a collision. Compared after
    // canonicalizing both, since that is what makes "Note.md" and "note.md"
    // compare equal on macOS and Windows.
    if dest.exists() && vault::canonical(&dest) != vault::canonical(&src) {
        return Err(AppError::conflict(format!("Already exists: {dest_rel}")));
    }

    std::fs::rename(&src, &dest)?;
    Ok(Json(json!({ "path": dest_rel })))
}

#[derive(Deserialize)]
pub struct DeleteBody {
    path: Option<String>,
}

/// Move a note, or a folder and its contents, into the vault's trash.
///
/// Returns a token the client can hand to /api/restore, which is what makes an
/// "Undo" offer possible — removing a folder of notes is not something to do on
/// a single confirm click.
pub async fn delete_entry(
    State(state): State<AppState>,
    Json(body): Json<DeleteBody>,
) -> AppResult<Json<Value>> {
    let rel = trim_rel(body.path.as_deref().unwrap_or(""));
    if rel.is_empty() {
        return Err(AppError::bad_request("No path provided"));
    }

    let vault_path = state.vault_resolved();
    let full = state.resolve(&rel)?;

    if !full.exists() {
        return Err(AppError::not_found(format!("Not found: {rel}")));
    }
    if full == vault_path {
        return Err(AppError::forbidden("Cannot delete the vault itself"));
    }

    let trash = trash_dir(&vault_path);
    if full == trash || full.starts_with(&trash) {
        return Err(AppError::forbidden(
            "Cannot delete the trash through this endpoint",
        ));
    }

    let token = format!(
        "{}-{}",
        Local::now().format("%Y%m%d-%H%M%S"),
        &uuid::Uuid::new_v4().simple().to_string()[..8]
    );
    let holding = trash.join(&token);
    std::fs::create_dir_all(&holding)?;

    let kind = if full.is_dir() { "folder" } else { "note" };
    let name = full
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .ok_or_else(|| AppError::bad_request(format!("Not a deletable path: {rel}")))?;

    move_path(&full, &holding.join(&name))?;
    std::fs::write(
        holding.join(TRASH_MANIFEST),
        serde_json::to_string_pretty(&json!({
            "path": rel,
            "name": name,
            "kind": kind,
            "deleted_at": Local::now().format("%Y-%m-%dT%H:%M:%S").to_string(),
        }))
        .unwrap_or_default(),
    )?;

    Ok(Json(json!({ "deleted": rel, "kind": kind, "token": token })))
}

#[derive(Deserialize)]
pub struct RestoreBody {
    token: Option<String>,
}

/// Put a trashed note or folder back where it came from.
pub async fn restore_entry(
    State(state): State<AppState>,
    Json(body): Json<RestoreBody>,
) -> AppResult<Json<Value>> {
    let token = body.token.unwrap_or_default().trim().to_string();
    if token.is_empty() {
        return Err(AppError::bad_request("No token provided"));
    }

    let vault_path = state.vault_resolved();
    let trash = trash_dir(&vault_path);

    // The token comes from the client, so it gets the same treatment as any
    // other path: proven to sit inside the trash before anything is moved.
    let holding = vault::resolve_in_vault(&trash, &token)
        .map_err(|_| AppError::not_found(format!("Nothing to restore for {token}")))?;

    if !holding.starts_with(&trash) || !holding.is_dir() {
        return Err(AppError::not_found(format!("Nothing to restore for {token}")));
    }

    let manifest: Value = std::fs::read_to_string(holding.join(TRASH_MANIFEST))
        .ok()
        .and_then(|t| serde_json::from_str(&t).ok())
        .ok_or_else(|| AppError::internal("That deletion's manifest is unreadable"))?;

    let rel = manifest["path"]
        .as_str()
        .ok_or_else(|| AppError::internal("That deletion's manifest is unreadable"))?;
    let name = manifest["name"]
        .as_str()
        .ok_or_else(|| AppError::internal("That deletion's manifest is unreadable"))?;

    let target = state.resolve(rel)?;
    if target.exists() {
        return Err(AppError::conflict(format!("Something is already at {rel}")));
    }

    let source = holding.join(name);
    if !source.exists() {
        return Err(AppError::not_found(format!("Nothing to restore for {token}")));
    }

    if let Some(parent) = target.parent() {
        std::fs::create_dir_all(parent)?;
    }
    move_path(&source, &target)?;
    let _ = std::fs::remove_dir_all(&holding);

    Ok(Json(json!({
        "restored": rel,
        "kind": manifest["kind"].as_str().unwrap_or("note"),
    })))
}

#[derive(Deserialize)]
pub struct StatusQuery {
    filepath: String,
    #[serde(default)]
    mtime: f64,
}

pub async fn get_status(
    State(state): State<AppState>,
    Query(params): Query<StatusQuery>,
) -> AppResult<Json<Value>> {
    let full = state.resolve(&params.filepath)?;

    if !full.is_file() {
        return Ok(Json(json!({ "exists": false, "changed": false })));
    }

    let server_mtime = mtime_secs(&full);
    Ok(Json(json!({
        "exists": true,
        "changed": (server_mtime - params.mtime).abs() > 0.01,
        "mtime": server_mtime,
    })))
}

/// Strip the surrounding whitespace and slashes the client may send.
pub fn trim_rel(raw: &str) -> String {
    raw.trim().trim_matches('/').to_string()
}

/// Expand a leading ~ the way Path.expanduser() does.
pub fn expand_user(raw: &str) -> PathBuf {
    if let Some(rest) = raw.strip_prefix('~') {
        if rest.is_empty() || rest.starts_with('/') || rest.starts_with('\\') {
            if let Some(home) = std::env::var_os("HOME").or_else(|| std::env::var_os("USERPROFILE"))
            {
                return PathBuf::from(home).join(rest.trim_start_matches(['/', '\\']));
            }
        }
    }
    PathBuf::from(raw)
}
