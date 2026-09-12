//! The API contract between the PossessApp server and its UI.
//!
//! Both sides depend on this crate, so a field renamed here stops compiling on
//! whichever side has not caught up. The hand-written fetch wrappers in
//! `js/api.js` could only fail at runtime, in the browser, on the one code path
//! that happened to touch the changed field.
//!
//! Serialization is exact on purpose: these types reproduce the JSON the
//! original FastAPI backend emitted, field for field. Where a field was absent
//! rather than null it is `skip_serializing_if`, and where it was null it is a
//! plain `Option` — the difference is observable to the client and is covered by
//! the differential test.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

// ── Vault selection ──

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VaultInfo {
    pub path: String,
    pub name: String,
    /// Reported by GET /api/vault and omitted by POST, which answers about a
    /// folder it has just confirmed is there.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub exists: Option<bool>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SetVaultRequest {
    pub path: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DirEntry {
    pub name: String,
    pub path: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BrowseResponse {
    pub path: String,
    /// Null at a filesystem root, where there is nowhere further up to go.
    pub parent: Option<String>,
    pub dirs: Vec<DirEntry>,
    /// Drive letters on Windows, so the picker isn't stranded on one volume.
    pub drives: Vec<String>,
}

// ── Notes ──

/// One note in a folder listing: its filename and its absolute path.
///
/// A two-element array in JSON, which is what the sidebar destructures.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NoteEntry(pub String, pub String);

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FoldersResponse {
    pub base: String,
    /// Keyed by folder path relative to the base ("." for the base itself),
    /// listing only folders that directly hold notes. The client splits those
    /// keys to rebuild the hierarchy, so folders that hold none need no entry.
    pub folders: BTreeMap<String, Vec<NoteEntry>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileResponse {
    pub filename: String,
    pub content: String,
    pub mtime: f64,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SaveFileRequest {
    #[serde(default)]
    pub content: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SaveFileResponse {
    pub saved: bool,
    pub mtime: f64,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct CreateRequest {
    pub kind: Option<String>,
    pub parent: Option<String>,
    pub name: Option<String>,
    #[serde(default)]
    pub content: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateResponse {
    pub path: String,
    pub kind: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct RenameRequest {
    pub path: Option<String>,
    pub name: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RenameResponse {
    pub path: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct DeleteRequest {
    pub path: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeleteResponse {
    pub deleted: String,
    pub kind: String,
    /// Handed back to /api/restore, which is what makes an "Undo" offer
    /// possible after deleting a folder of notes.
    pub token: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct RestoreRequest {
    pub token: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RestoreResponse {
    pub restored: String,
    pub kind: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StatusResponse {
    pub exists: bool,
    pub changed: bool,
    /// Absent, not null, when the file is gone — there is no mtime to report.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mtime: Option<f64>,
}

/// What the trash records about one deletion, so it can be put back.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TrashManifest {
    pub path: String,
    pub name: String,
    pub kind: String,
    pub deleted_at: String,
}

// ── Search ──

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchResult {
    pub path: String,
    pub name: String,
    /// The first matching line, so the sidebar can show context without
    /// shipping whole notes. Empty when only the filename matched.
    pub snippet: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchResponse {
    pub query: String,
    pub results: Vec<SearchResult>,
}

// ── Scripts ──

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScriptInfo {
    pub name: String,
    /// "manual" for scripts/*.py, "hook" for scripts/hooks/*.py.
    pub kind: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScriptsResponse {
    pub scripts: Vec<ScriptInfo>,
    pub dir: String,
    pub exists: bool,
    pub hooks_enabled: bool,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct RunScriptRequest {
    pub name: Option<String>,
    pub note: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RunScriptResponse {
    pub script: String,
    pub code: i32,
    pub stdout: String,
    pub stderr: String,
    pub seconds: f64,
    /// Notes the run created, deleted or rewrote, so the UI can rebuild the
    /// tree and reload the open note instead of waiting for the sync poll.
    pub changed: Vec<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SetHooksRequest {
    #[serde(default)]
    pub enabled: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SetHooksResponse {
    pub hooks_enabled: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScaffoldResponse {
    pub created: Vec<String>,
    pub dir: String,
}

// ── Images ──

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct PasteImageRequest {
    pub note: Option<String>,
    pub name: Option<String>,
    /// Base64, optionally still wrapped in a data: URI — a clipboard image has
    /// no file on disk to upload.
    pub data: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PasteImageResponse {
    pub path: String,
    /// What the note should link to: a sibling folder, so the link survives the
    /// vault being moved or renamed.
    pub markdown: String,
    pub bytes: usize,
}

// ── Errors ──

/// The body every failing endpoint returns.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ErrorBody {
    pub detail: String,
}
