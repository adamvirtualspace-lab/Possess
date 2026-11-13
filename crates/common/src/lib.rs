//! The API contract between the PossessApp server and its UI.
//!
//! Both sides depend on this crate, so a field renamed here stops compiling on
//! whichever side has not caught up. The hand-written fetch wrappers this
//! replaced could only fail at runtime, in the browser, on the one code path
//! that happened to touch the changed field.
//!
//! Serialization is exact on purpose: these types reproduce the JSON the
//! original FastAPI backend emitted, field for field. Where a field was absent
//! rather than null it is `skip_serializing_if`, and where it was null it is a
//! plain `Option` — the difference is observable to the client, and the two
//! backends were diffed response by response while both still existed.

pub mod tree;

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

// ── Graph (SYNTAX.md sections 5 to 7) ──

/// One status a node's file offers, in its format's order.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GraphStatus {
    pub ch: char,
    pub symbol: String,
    pub label: String,
    /// "open", "done" or "blocked".
    pub role: Option<String>,
    /// Listed after the done status: offered only by the right-click menu.
    pub right_click_only: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GraphTask {
    /// 1-based line in the node's file: what a status change names.
    pub line: usize,
    pub ch: char,
    /// What to draw: the status symbol, or ⚠ when `known` is false.
    pub symbol: String,
    pub label: String,
    pub known: bool,
    pub text: String,
    pub id: Option<String>,
    pub people: Vec<String>,
    pub done: bool,
    /// Has `needs::` wires, which then decide whether it is blocked.
    pub has_needs: bool,
    /// Has `needs::` that aren't all done. Drawn with a lock; clicking it
    /// does nothing while it is blocked.
    pub locked: bool,
    /// Where a click sends it, if anywhere.
    pub next: Option<char>,
    /// Set when the saved status disagrees with the task's `needs::`.
    pub warning: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GraphNode {
    /// Vault key: relative path without `.md`. What wires and flows use.
    pub path: String,
    /// The file to open in the editor, with `.md`.
    pub file: String,
    /// The note's first `#` heading, or its file name.
    pub title: String,
    /// A flow links to a note that doesn't exist.
    pub missing: bool,
    /// Canvas position: saved in the flow, or laid out automatically.
    pub x: i32,
    pub y: i32,
    /// The position came from the flow note, not the automatic layout.
    pub placed: bool,
    /// The node's line in the flow note, so a drag can save its position.
    /// `None` outside a flow.
    pub flow_line: Option<usize>,
    pub statuses: Vec<GraphStatus>,
    pub tasks: Vec<GraphTask>,
    /// Wires arriving at and leaving this node's tasks.
    pub wires_in: usize,
    pub wires_out: usize,
    /// Tasks whose status isn't done.
    pub open: usize,
    pub warnings: usize,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GraphEdge {
    /// "needs" or "sync".
    pub kind: String,
    /// Upstream: the task that is needed or synced from.
    pub from_node: String,
    pub from_line: usize,
    /// Downstream: the task the wire is written on.
    pub to_node: String,
    pub to_line: usize,
    /// The upstream task is done.
    pub live: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GraphResponse {
    /// The flow note shown, or `None` for the whole vault.
    pub flow: Option<String>,
    pub nodes: Vec<GraphNode>,
    pub edges: Vec<GraphEdge>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskStatusRequest {
    /// Vault-relative path, with or without `.md`.
    pub path: String,
    pub line: usize,
    /// The status the client last saw. A mismatch means the file changed,
    /// and nothing is written.
    pub from: char,
    pub to: char,
}

/// One line the server rewrote.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct StatusChange {
    pub path: String,
    pub line: usize,
    pub from: char,
    pub to: char,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskStatusResponse {
    /// The requested change first, then every task it unblocked or blocked.
    pub changes: Vec<StatusChange>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FlowPositionRequest {
    /// The flow note.
    pub flow: String,
    /// The node's line in it.
    pub line: usize,
    pub x: i32,
    pub y: i32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FlowPositionResponse {
    pub saved: bool,
    pub mtime: f64,
}
