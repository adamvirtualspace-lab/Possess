//! The wire engine's memory: the activity log, undo, and edits made outside
//! the app (SYNTAX.md sections 6 and 8).
//!
//! Everything that changes task statuses goes through one [`Engine`], behind
//! one lock, so a click, an undo, the editor's save and the outside-edit scan
//! never interleave.
//!
//! - **The log** is `.obsessed/activity.json` in the vault, so undo survives
//!   a restart. Undo checks every line still shows what the chain set; if
//!   one was edited since, nothing is undone.
//! - **Outside edits** are found by comparing each note with the last content
//!   the engine saw (its own writes included, so those never look like
//!   outside edits). Their wires don't fire on their own: the entry lists what
//!   they would do, and Apply runs it.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

use possess_common::{
    ActivityChange, ActivityEntry, ActivityLoop, ActivityResponse, ActivityResult, StatusChange,
};
use possess_markdown::tasks::{
    note_key, parse, Chain, Change, EditError, LoopSkip, NeedsState, Role, Vault,
};
use serde::{Deserialize, Serialize};

use crate::error::{AppError, AppResult};
use crate::notes::walk_notes;

/// Oldest entries are dropped past this.
const MAX_ENTRIES: usize = 200;

/// Notes modified this recently are re-read on every scan, whatever their
/// size and time say.
const RECENT: std::time::Duration = std::time::Duration::from_secs(3);

// ── The log file ──

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
struct LogChange {
    path: String,
    line: usize,
    id: Option<String>,
    text: String,
    from: char,
    to: char,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
struct LogLoop {
    path: String,
    line: usize,
    text: String,
    wanted: char,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
struct LogEntry {
    id: u64,
    time: String,
    source: String,
    changes: Vec<LogChange>,
    #[serde(default)]
    loops: Vec<LogLoop>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    undone: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    undo_of: Option<u64>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    would: Vec<LogChange>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    state: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
struct LogFile {
    version: u32,
    next_id: u64,
    entries: Vec<LogEntry>,
}

impl Default for LogFile {
    fn default() -> Self {
        LogFile {
            version: 1,
            next_id: 1,
            entries: Vec::new(),
        }
    }
}

fn to_log(change: &Change) -> LogChange {
    LogChange {
        path: change.path.clone(),
        line: change.line,
        id: change.id.clone(),
        text: change.text.clone(),
        from: change.from,
        to: change.to,
    }
}

fn from_log(change: &LogChange) -> Change {
    Change {
        path: change.path.clone(),
        line: change.line,
        id: change.id.clone(),
        text: change.text.clone(),
        from: change.from,
        to: change.to,
    }
}

fn loop_to_log(skip: &LoopSkip) -> LogLoop {
    LogLoop {
        path: skip.path.clone(),
        line: skip.line,
        text: skip.text.clone(),
        wanted: skip.wanted,
    }
}

fn now() -> String {
    chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true)
}

pub fn log_path(vault: &Path) -> PathBuf {
    vault.join(".obsessed").join("activity.json")
}

/// A note's modification time and size when the engine last read it.
type Stamp = (Option<SystemTime>, u64);

fn stamp(meta: Option<std::fs::Metadata>) -> Stamp {
    (
        meta.as_ref().and_then(|m| m.modified().ok()),
        meta.map_or(0, |m| m.len()),
    )
}

/// The engine for one vault.
pub struct Engine {
    dir: PathBuf,
    log: LogFile,
    /// Every note, parsed, as the engine last saw it: read once when the
    /// engine opens, then kept current by its own writes and by the edits
    /// the watcher reports. Nothing re-reads the whole vault per request,
    /// and there is only ever this one copy in memory.
    notes: Vault,
    /// Time and size per note, for the safety rescan.
    stamps: HashMap<String, Stamp>,
}

impl Engine {
    /// Open a vault's engine: read every note once, and the log if there is
    /// one. A log that can't be read is set aside as `activity.json.bad`,
    /// not overwritten.
    pub fn open(dir: &Path) -> Engine {
        let path = log_path(dir);
        let log = match std::fs::read_to_string(&path) {
            Ok(text) => match serde_json::from_str(&text) {
                Ok(log) => log,
                Err(err) => {
                    eprintln!(
                        "[PossessApp] {} is unreadable ({err}); starting a new log",
                        path.display()
                    );
                    let _ = std::fs::rename(&path, path.with_extension("json.bad"));
                    LogFile::default()
                }
            },
            Err(_) => LogFile::default(),
        };
        let mut engine = Engine {
            dir: dir.to_path_buf(),
            log,
            notes: Vault::new(),
            stamps: HashMap::new(),
        };
        engine.load_all();
        engine
    }

    /// Read every note from disk. Used when the engine opens, and to recover
    /// if a change ever fails halfway.
    fn load_all(&mut self) {
        self.notes = Vault::new();
        self.stamps.clear();
        for entry in walk_notes(&self.dir) {
            let Ok(rel) = entry.path().strip_prefix(&self.dir) else {
                continue;
            };
            if let Ok(src) = std::fs::read_to_string(entry.path()) {
                let key = note_key(&rel.to_string_lossy());
                self.notes.insert(&key, &src);
                self.stamps.insert(key, stamp(entry.metadata().ok()));
            }
        }
    }

    pub fn dir(&self) -> &Path {
        &self.dir
    }

    /// The vault as the engine knows it.
    pub fn notes(&self) -> &Vault {
        &self.notes
    }

    fn save_log(&self) -> std::io::Result<()> {
        let path = log_path(&self.dir);
        std::fs::create_dir_all(path.parent().expect("log has a parent"))?;
        let tmp = path.with_extension("json.tmp");
        let json = serde_json::to_string_pretty(&self.log).expect("log serializes");
        std::fs::write(&tmp, json + "\n")?;
        // Replace in one step, so a crash never leaves half a log.
        std::fs::rename(&tmp, &path)
    }

    fn push(&mut self, mut entry: LogEntry) -> AppResult<u64> {
        entry.id = self.log.next_id;
        self.log.next_id += 1;
        self.log.entries.push(entry);
        if self.log.entries.len() > MAX_ENTRIES {
            let extra = self.log.entries.len() - MAX_ENTRIES;
            self.log.entries.drain(..extra);
        }
        self.save_log()?;
        Ok(self.log.next_id - 1)
    }

    /// Write every note a chain changed in memory. Stamps are taken after
    /// writing, so the watcher's report of these writes is recognised as the
    /// app's own and costs no more than a compare.
    fn write_changed(&mut self, changes: &[Change]) -> AppResult<()> {
        let mut done: Vec<&str> = Vec::new();
        for change in changes {
            if done.contains(&change.path.as_str()) {
                continue;
            }
            done.push(&change.path);
            let full = crate::vault::resolve_in_vault(&self.dir, &format!("{}.md", change.path))?;
            let src = self
                .notes
                .source(&change.path)
                .ok_or_else(|| AppError::internal("a changed note vanished from the vault"))?;
            std::fs::write(&full, src)?;
            self.stamps
                .insert(change.path.clone(), stamp(std::fs::metadata(&full).ok()));
        }
        Ok(())
    }

    /// Changes are made to the one in-memory copy, never a clone. If one
    /// fails partway (which validation should make impossible), the copy is
    /// read back from disk so it never disagrees with the files.
    fn recover<T>(&mut self, result: Result<T, EditError>) -> AppResult<T> {
        result.map_err(|err| {
            self.load_all();
            edit_error(err)
        })
    }

    // ── Outside edits ──

    /// Read one note again if it changed, and return its status changes.
    /// `None` means the path isn't a note of this vault.
    fn reread(&mut self, path: &Path) -> Option<Vec<Change>> {
        let rel = path.strip_prefix(&self.dir).ok()?;
        let hidden = rel
            .components()
            .any(|c| c.as_os_str().to_string_lossy().starts_with('.'));
        if hidden || rel.extension().is_none_or(|e| e != "md") {
            return None;
        }
        let key = note_key(&rel.to_string_lossy());
        let Ok(content) = std::fs::read_to_string(path) else {
            // Deleted (or unreadable): it's no longer part of the vault.
            self.notes.remove(&key);
            self.stamps.remove(&key);
            return Some(Vec::new());
        };
        self.stamps
            .insert(key.clone(), stamp(std::fs::metadata(path).ok()));
        let changes = match self.notes.source(&key) {
            Some(old) if old == content => return Some(Vec::new()),
            Some(old) => status_changes(&key, old, &content),
            // A new note: nothing to compare its statuses with.
            None => Vec::new(),
        };
        self.notes.insert(&key, &content);
        Some(changes)
    }

    /// Re-read the notes the file watcher reported, and log their status
    /// changes as `source` ("outside", or "editor" for the app's own
    /// markdown editor). A reported folder (a rename, say) means a full
    /// rescan. Returns whether anything was logged.
    pub fn scan_paths(&mut self, paths: &[PathBuf], source: &str) -> AppResult<bool> {
        let mut changes = Vec::new();
        for path in paths {
            if path.is_dir() {
                return self.scan(source);
            }
            if let Some(found) = self.reread(path) {
                changes.extend(found);
            }
        }
        self.log_outside(source, changes)
    }

    /// Check every note's time and size, and re-read only the ones that
    /// changed: the safety net under the file watcher, and the fallback on a
    /// platform without one. Returns whether anything was logged.
    pub fn scan(&mut self, source: &str) -> AppResult<bool> {
        let mut changes = Vec::new();
        let mut present = std::collections::HashSet::new();
        let dir = self.dir.clone();
        for entry in walk_notes(&dir) {
            let Ok(rel) = entry.path().strip_prefix(&dir) else {
                continue;
            };
            let key = note_key(&rel.to_string_lossy());
            let now = stamp(entry.metadata().ok());
            // Same time and size means unchanged, unless the time is recent:
            // ticking [ ] to [x] keeps the size, and on coarse file systems
            // two quick edits can share a timestamp.
            let recent = now
                .0
                .and_then(|m| m.elapsed().ok())
                .is_none_or(|age| age < RECENT);
            let unchanged = self.stamps.get(&key) == Some(&now) && !recent;
            present.insert(key);
            if !unchanged {
                if let Some(found) = self.reread(entry.path()) {
                    changes.extend(found);
                }
            }
        }
        let gone: Vec<String> = self
            .stamps
            .keys()
            .filter(|k| !present.contains(*k))
            .cloned()
            .collect();
        for key in gone {
            self.notes.remove(&key);
            self.stamps.remove(&key);
        }
        self.log_outside(source, changes)
    }

    fn log_outside(&mut self, source: &str, changes: Vec<Change>) -> AppResult<bool> {
        if changes.is_empty() {
            return Ok(false);
        }
        let would = self.notes.preview(&changes);
        self.push(LogEntry {
            id: 0,
            time: now(),
            source: source.to_string(),
            changes: changes.iter().map(to_log).collect(),
            loops: Vec::new(),
            undone: None,
            undo_of: None,
            state: (!would.changes.is_empty()).then(|| "pending".to_string()),
            would: would.changes.iter().map(to_log).collect(),
        })?;
        Ok(true)
    }

    // ── Changes the app makes ──

    fn result(&self, entry: Option<u64>, changes: &[Change]) -> ActivityResult {
        ActivityResult {
            entry,
            changes: changes
                .iter()
                .map(|c| StatusChange {
                    path: c.path.clone(),
                    line: c.line,
                    from: c.from,
                    to: c.to,
                })
                .collect(),
        }
    }

    fn log_chain(&mut self, source: &str, chain: &Chain, undo_of: Option<u64>) -> AppResult<u64> {
        self.push(LogEntry {
            id: 0,
            time: now(),
            source: source.to_string(),
            changes: chain.changes.iter().map(to_log).collect(),
            loops: chain.loops.iter().map(loop_to_log).collect(),
            undone: (source != "undo").then_some(false),
            undo_of,
            would: Vec::new(),
            state: None,
        })
    }

    /// A status change from the app (a click or the status menu), and what
    /// its wires set off.
    pub fn set_status(
        &mut self,
        path: &str,
        line: usize,
        from: char,
        to: char,
    ) -> AppResult<ActivityResult> {
        let vault = &self.notes;
        let key = note_key(path);

        // A blocked task waiting on its wires is the wires' to change.
        if let Some(file) = vault.file(&key) {
            if let Some(task) = file.task_at_line(line) {
                let blocked = file.format.role_of(task.ch) == Some(Role::Blocked);
                let waiting = matches!(
                    vault.needs_state(&key, task),
                    NeedsState::Unmet | NeedsState::Broken
                );
                if blocked && waiting && task.ch == from {
                    return Err(AppError::conflict(
                        "This task is waiting on its needs:: wires, so it can't be changed yet.",
                    ));
                }
            }
        }
        let result = self.notes.set_status(&key, line, from, to);
        let chain = match result {
            // A stale or impossible click changed nothing: no recovery needed.
            Err(
                err @ (EditError::Stale { .. }
                | EditError::NoFile { .. }
                | EditError::NoTask { .. }),
            ) => return Err(edit_error(err)),
            other => self.recover(other)?,
        };
        self.write_changed(&chain.changes)?;
        let id = self.log_chain("click", &chain, None)?;
        Ok(self.result(Some(id), &chain.changes))
    }

    /// The entry Undo acts on: the newest one the app made, not yet undone.
    fn undo_target(&self) -> Option<usize> {
        self.log.entries.iter().rposition(|e| {
            matches!(e.source.as_str(), "click" | "apply") && e.undone == Some(false)
        })
    }

    pub fn undo(&mut self) -> AppResult<ActivityResult> {
        let index = self
            .undo_target()
            .ok_or_else(|| AppError::conflict("There is nothing to undo."))?;
        let entry = self.log.entries[index].clone();
        let changes: Vec<Change> = entry.changes.iter().map(from_log).collect();
        let reverted = self.notes.revert(&changes).map_err(|err| match err {
            EditError::Stale { line, found } => {
                let path = changes
                    .iter()
                    .find(|c| c.line == line)
                    .map_or("a note", |c| c.path.as_str());
                AppError::conflict(format!(
                    "Can't undo: line {line} of {path} now shows [{found}]. It was edited since, so nothing was undone."
                ))
            }
            other => edit_error(other),
        })?;
        self.write_changed(&reverted)?;
        self.log.entries[index].undone = Some(true);
        let chain = Chain {
            changes: reverted,
            loops: Vec::new(),
        };
        let id = self.log_chain("undo", &chain, Some(entry.id))?;
        Ok(self.result(Some(id), &chain.changes))
    }

    fn pending(&self, id: u64) -> AppResult<usize> {
        let index = self
            .log
            .entries
            .iter()
            .position(|e| e.id == id)
            .ok_or_else(|| AppError::not_found(format!("No activity entry {id}")))?;
        if self.log.entries[index].state.as_deref() != Some("pending") {
            return Err(AppError::conflict(
                "That entry has nothing waiting to apply.",
            ));
        }
        Ok(index)
    }

    /// Run what an outside edit's wires would do, checked against the files
    /// as they are now. Logged as an `apply` chain, which can be undone.
    pub fn apply(&mut self, id: u64) -> AppResult<ActivityResult> {
        let index = self.pending(id)?;
        let vault = &self.notes;
        // Only the edits that still hold set anything off.
        let still: Vec<Change> = self.log.entries[index]
            .changes
            .iter()
            .map(from_log)
            .filter_map(|c| {
                let line = vault.find_task_line(&c.path, c.line, c.id.as_deref(), &c.text)?;
                let now = vault.file(&c.path)?.task_at_line(line)?.ch;
                (now == c.to).then_some(Change { line, ..c })
            })
            .collect();
        let result = self.notes.follow(&still);
        let chain = self.recover(result)?;
        self.write_changed(&chain.changes)?;
        self.log.entries[index].state = Some("applied".into());
        let id = if chain.changes.is_empty() {
            self.save_log()?;
            None
        } else {
            Some(self.log_chain("apply", &chain, None)?)
        };
        Ok(self.result(id, &chain.changes))
    }

    pub fn dismiss(&mut self, id: u64) -> AppResult<()> {
        let index = self.pending(id)?;
        self.log.entries[index].state = Some("dismissed".into());
        self.save_log()?;
        Ok(())
    }

    // ── Reading the log ──

    pub fn activity(&self) -> AppResult<ActivityResponse> {
        // Symbols from each note's current status format.
        let vault = &self.notes;
        let symbol = |path: &str, ch: char| {
            vault
                .file(path)
                .and_then(|f| f.format.get(ch).map(|s| s.symbol.clone()))
                .unwrap_or_else(|| format!("[{ch}]"))
        };
        let show = |c: &LogChange| ActivityChange {
            path: c.path.clone(),
            line: c.line,
            id: c.id.clone(),
            text: c.text.clone(),
            from: c.from,
            to: c.to,
            from_symbol: symbol(&c.path, c.from),
            to_symbol: symbol(&c.path, c.to),
        };
        let target = self.undo_target().map(|i| self.log.entries[i].id);
        Ok(ActivityResponse {
            entries: self
                .log
                .entries
                .iter()
                .rev()
                .map(|e| ActivityEntry {
                    id: e.id,
                    time: e.time.clone(),
                    source: e.source.clone(),
                    changes: e.changes.iter().map(show).collect(),
                    loops: e
                        .loops
                        .iter()
                        .map(|l| ActivityLoop {
                            path: l.path.clone(),
                            line: l.line,
                            text: l.text.clone(),
                            wanted: l.wanted,
                        })
                        .collect(),
                    undone: e.undone == Some(true),
                    undo_of: e.undo_of,
                    would: e.would.iter().map(show).collect(),
                    state: e.state.clone(),
                })
                .collect(),
            undo_target: target,
        })
    }
}

pub fn edit_error(err: EditError) -> AppError {
    match err {
        EditError::Stale { .. } => AppError::conflict(err.to_string()),
        EditError::NoFile { .. } | EditError::NoTask { .. } | EditError::NoNode { .. } => {
            AppError::not_found(err.to_string())
        }
        EditError::UnknownStatus { .. } => AppError::bad_request(err.to_string()),
    }
}

/// The status changes between two versions of a note. Tasks are matched by
/// `^id`, else by text where it's unique in both, else by line: a line added
/// above a task shouldn't read as every task below it changing.
pub fn status_changes(key: &str, before: &str, after: &str) -> Vec<Change> {
    let old = parse(before);
    let new = parse(after);
    let unique = |tasks: &[possess_markdown::tasks::Task], text: &str| {
        tasks.iter().filter(|t| t.text == text).count() == 1
    };
    let mut out = Vec::new();
    for task in &new.tasks {
        let previous = match &task.id {
            Some(id) => old.task_by_id(id),
            None if unique(&new.tasks, &task.text) && unique(&old.tasks, &task.text) => {
                old.tasks.iter().find(|t| t.text == task.text)
            }
            None => old.task_at_line(task.line).filter(|t| t.text == task.text),
        };
        if let Some(previous) = previous {
            if previous.ch != task.ch {
                out.push(Change {
                    path: key.to_string(),
                    line: task.line,
                    id: task.id.clone(),
                    text: task.text.clone(),
                    from: previous.ch,
                    to: task.ch,
                });
            }
        }
    }
    out
}

// ── HTTP ──

use axum::extract::State;
use axum::response::sse::{Event, KeepAlive, Sse};
use axum::Json;
use possess_common::{ActivityIdRequest, TaskStatusRequest};
use tokio_stream::wrappers::BroadcastStream;
use tokio_stream::{Stream, StreamExt};

use crate::vault::AppState;

/// How long to let a burst of file events settle (a `git pull`, an editor
/// that writes a temp file and renames it) before reading anything.
const DEBOUNCE: std::time::Duration = std::time::Duration::from_millis(150);
/// The safety rescan under the file watcher: file times and sizes only.
const RESCAN: std::time::Duration = std::time::Duration::from_secs(60);
/// The rescan when the platform can't watch files at all.
const FALLBACK_RESCAN: std::time::Duration = std::time::Duration::from_secs(2);

/// Would this path's change matter? Notes only, and nothing hidden, so the
/// app's own `.obsessed/activity.json` writes never wake the watcher. A path
/// with no extension may be a folder (renamed or moved), which does matter.
fn worth_waking_for(dir: &Path, path: &Path) -> bool {
    let Ok(rel) = path.strip_prefix(dir) else {
        return false;
    };
    let hidden = rel
        .components()
        .any(|c| c.as_os_str().to_string_lossy().starts_with('.'));
    !hidden && rel.extension().is_none_or(|e| e == "md")
}

/// Start the OS file watcher on `dir`. Its callback runs on the watcher's
/// own thread and only records paths and wakes the async side.
fn start_watcher(dir: &Path, state: &AppState) -> Option<notify::RecommendedWatcher> {
    use notify::Watcher;
    let pending = state.pending.clone();
    let wake = state.wake.clone();
    let root = dir.to_path_buf();
    let mut watcher = notify::recommended_watcher(move |res: notify::Result<notify::Event>| {
        let Ok(event) = res else { return };
        if event.kind.is_access() {
            return;
        }
        let paths: Vec<PathBuf> = event
            .paths
            .into_iter()
            .filter(|p| worth_waking_for(&root, p))
            .collect();
        if paths.is_empty() {
            return;
        }
        pending.lock().expect("pending lock").extend(paths);
        wake.notify_one();
    })
    .map_err(|err| eprintln!("[PossessApp] No file watcher ({err}); checking every 2s instead"))
    .ok()?;
    match watcher.watch(dir, notify::RecursiveMode::Recursive) {
        Ok(()) => Some(watcher),
        Err(err) => {
            eprintln!(
                "[PossessApp] Can't watch {} ({err}); checking every 2s instead",
                dir.display()
            );
            None
        }
    }
}

/// Catch the engine up on the paths the watcher reported.
pub(crate) fn catch_up(state: &AppState, engine: &mut Engine) -> AppResult<bool> {
    let paths: Vec<PathBuf> = state
        .pending
        .lock()
        .expect("pending lock")
        .drain()
        .collect();
    if paths.is_empty() {
        return Ok(false);
    }
    engine.scan_paths(&paths, "outside")
}

/// Notice edits made outside the app, for as long as the server runs.
///
/// Event-driven: at idle this task sleeps, waking once a minute for a
/// rescan that only compares file times and sizes. Each event is let settle
/// for a moment, then only the notes it named are re-read.
pub async fn watch(state: AppState) {
    loop {
        let dir = state.vault_resolved();
        let watcher = start_watcher(&dir, &state);
        let every = if watcher.is_some() {
            RESCAN
        } else {
            FALLBACK_RESCAN
        };
        let mut rescan = tokio::time::interval_at(tokio::time::Instant::now() + every, every);
        rescan.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);

        loop {
            let logged = tokio::select! {
                // A different vault: watch that one instead.
                _ = state.vault_changed.notified() => break,
                _ = state.wake.notified() => {
                    tokio::time::sleep(DEBOUNCE).await;
                    let mut engine = state.engine().await;
                    catch_up(&state, &mut engine)
                }
                _ = rescan.tick() => {
                    let mut engine = state.engine().await;
                    catch_up(&state, &mut engine).and_then(|a| Ok(engine.scan("outside")? || a))
                }
            };
            match logged {
                Ok(true) => state.notify(),
                Ok(false) => {}
                Err(err) => eprintln!(
                    "[PossessApp] Could not check the vault for edits: {}",
                    err.detail
                ),
            }
        }
        drop(watcher);
    }
}

/// Run `action` on the engine after catching up on reported edits, so they
/// are logged as outside edits rather than folded into the app's change.
pub(crate) async fn act<T>(
    state: &AppState,
    action: impl FnOnce(&mut Engine) -> AppResult<T>,
) -> AppResult<T> {
    let result = {
        let mut engine = state.engine().await;
        catch_up(state, &mut engine)?;
        action(&mut engine)
    };
    state.notify();
    result
}

pub async fn set_task_status(
    State(state): State<AppState>,
    Json(req): Json<TaskStatusRequest>,
) -> AppResult<Json<ActivityResult>> {
    act(&state, |e| {
        e.set_status(&req.path, req.line, req.from, req.to)
    })
    .await
    .map(Json)
}

pub async fn get_activity(State(state): State<AppState>) -> AppResult<Json<ActivityResponse>> {
    let engine = state.engine().await;
    engine.activity().map(Json)
}

pub async fn undo(State(state): State<AppState>) -> AppResult<Json<ActivityResult>> {
    act(&state, Engine::undo).await.map(Json)
}

pub async fn apply(
    State(state): State<AppState>,
    Json(req): Json<ActivityIdRequest>,
) -> AppResult<Json<ActivityResult>> {
    act(&state, |e| e.apply(req.id)).await.map(Json)
}

pub async fn dismiss(
    State(state): State<AppState>,
    Json(req): Json<ActivityIdRequest>,
) -> AppResult<Json<ActivityResult>> {
    act(&state, |e| {
        e.dismiss(req.id)?;
        Ok(ActivityResult {
            entry: None,
            changes: Vec::new(),
        })
    })
    .await
    .map(Json)
}

/// A server-sent event whenever statuses or the log change, so an open
/// graph redraws on its own when someone ticks a task in Obsidian.
pub async fn events(
    State(state): State<AppState>,
) -> Sse<impl Stream<Item = Result<Event, std::convert::Infallible>>> {
    // A listener that fell behind just hears "changed" once more.
    let stream = BroadcastStream::new(state.events.subscribe())
        .map(|_| Ok(Event::default().data("changed")));
    Sse::new(stream).keep_alive(KeepAlive::default())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A throwaway copy of the sample vault.
    fn sample_copy() -> PathBuf {
        let src = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../sample-vault");
        let dst = std::env::temp_dir().join(format!("possess-activity-{}", uuid::Uuid::new_v4()));
        for entry in walkdir::WalkDir::new(&src)
            .into_iter()
            .filter_map(Result::ok)
        {
            let rel = entry.path().strip_prefix(&src).unwrap();
            let target = dst.join(rel);
            if entry.file_type().is_dir() {
                std::fs::create_dir_all(&target).unwrap();
            } else {
                std::fs::copy(entry.path(), &target).unwrap();
            }
        }
        crate::vault::canonical(&dst)
    }

    fn edit(base: &Path, rel: &str, from: &str, to: &str) {
        let path = base.join(rel);
        let text = std::fs::read_to_string(&path).unwrap();
        assert!(text.contains(from), "{rel} has no {from:?}");
        std::fs::write(path, text.replacen(from, to, 1)).unwrap();
    }

    fn read(base: &Path, rel: &str) -> String {
        std::fs::read_to_string(base.join(rel)).unwrap()
    }

    #[test]
    fn clicks_are_logged_and_undone_across_a_restart() {
        let base = sample_copy();
        let mut engine = Engine::open(&base);
        engine.scan("outside").unwrap();
        engine.set_status("anim/sh040", 10, ' ', 'x').unwrap();
        let result = engine
            .set_status("fx/sh040-water.md", 10, ' ', 'x')
            .unwrap();
        assert_eq!(result.changes.len(), 2, "the tick and lighting's unblock");
        assert!(read(&base, "lighting/sh040.md").contains("- [ ] Render lighting pass"));
        // The engine's own writes aren't outside edits.
        assert!(!engine.scan("outside").unwrap());

        // A restart: the log is read back from .obsessed/activity.json.
        drop(engine);
        let mut engine = Engine::open(&base);
        let activity = engine.activity().unwrap();
        assert_eq!(activity.entries.len(), 2);
        assert_eq!(activity.undo_target, Some(2));
        assert_eq!(activity.entries[0].changes[1].to_symbol, "🔳");

        engine.undo().unwrap();
        assert!(read(&base, "lighting/sh040.md").contains("- [-] Render lighting pass"));
        assert!(read(&base, "fx/sh040-water.md").contains("- [ ] Cache water to disk"));
        // Again: the chain before it.
        engine.undo().unwrap();
        assert!(read(&base, "anim/sh040.md").contains("- [ ] Polish pass"));
        assert_eq!(
            engine.undo().unwrap_err().status.as_u16(),
            409,
            "nothing left to undo"
        );
        let log: serde_json::Value =
            serde_json::from_str(&read(&base, ".obsessed/activity.json")).unwrap();
        assert_eq!(log["version"], 1);
        assert_eq!(log["entries"].as_array().unwrap().len(), 4);
        std::fs::remove_dir_all(base).ok();
    }

    #[test]
    fn clicks_that_cant_happen() {
        let base = sample_copy();
        let mut engine = Engine::open(&base);
        // Stale: the client thinks Polish pass is ready to check.
        let err = engine.set_status("anim/sh040", 10, '?', 'x').unwrap_err();
        assert_eq!(err.status.as_u16(), 409);
        // Waiting on wires: comp still needs l2, f2 and f3.
        let err = engine.set_status("comp/sh040", 16, '-', ' ').unwrap_err();
        assert_eq!(err.status.as_u16(), 409);
        // Outside the vault.
        assert!(engine.set_status("../escape", 1, ' ', 'x').is_err());
        assert!(
            engine.activity().unwrap().entries.is_empty(),
            "nothing was logged"
        );
        std::fs::remove_dir_all(base).ok();
    }

    #[test]
    fn undo_refuses_when_a_line_changed_since() {
        let base = sample_copy();
        let mut engine = Engine::open(&base);
        engine.set_status("anim/sh040", 10, ' ', 'x').unwrap();
        engine.set_status("fx/sh040-water", 10, ' ', 'x').unwrap();
        // Lighting is moved on in another app.
        edit(
            &base,
            "lighting/sh040.md",
            "- [ ] Render lighting",
            "- [?] Render lighting",
        );
        // The handler always catches up on reported edits first.
        engine
            .scan_paths(&[base.join("lighting/sh040.md")], "outside")
            .unwrap();
        let err = engine.undo().unwrap_err();
        assert_eq!(err.status.as_u16(), 409);
        assert!(err.detail.contains("lighting/sh040"), "{}", err.detail);
        // Nothing was undone.
        assert!(read(&base, "fx/sh040-water.md").contains("- [x] Cache water"));
        std::fs::remove_dir_all(base).ok();
    }

    #[test]
    fn outside_edits_are_flagged_not_fired_then_applied() {
        let base = sample_copy();
        let mut engine = Engine::open(&base);
        engine.scan("outside").unwrap(); // takes stock
        engine.set_status("anim/sh040", 10, ' ', 'x').unwrap();

        // Cache water to disk is ticked in Obsidian.
        edit(
            &base,
            "fx/sh040-water.md",
            "- [ ] Cache water",
            "- [x] Cache water",
        );
        assert!(engine.scan("outside").unwrap());
        // Nothing written: lighting is still blocked.
        assert!(read(&base, "lighting/sh040.md").contains("- [-] Render lighting pass"));
        let entry = engine.activity().unwrap().entries[0].clone();
        assert_eq!(entry.source, "outside");
        assert_eq!(entry.state.as_deref(), Some("pending"));
        assert_eq!(entry.would.len(), 1);
        assert_eq!(entry.would[0].path, "lighting/sh040");

        let result = engine.apply(entry.id).unwrap();
        assert_eq!(result.changes.len(), 1);
        assert!(read(&base, "lighting/sh040.md").contains("- [ ] Render lighting pass"));
        let activity = engine.activity().unwrap();
        assert_eq!(activity.entries[1].state.as_deref(), Some("applied"));
        assert_eq!(activity.entries[0].source, "apply");
        // An applied chain can be undone; the outside edit itself can't.
        engine.undo().unwrap();
        assert!(read(&base, "lighting/sh040.md").contains("- [-] Render lighting pass"));
        assert!(read(&base, "fx/sh040-water.md").contains("- [x] Cache water"));
        std::fs::remove_dir_all(base).ok();
    }

    #[test]
    fn an_edit_with_nothing_to_set_off_is_just_logged() {
        let base = sample_copy();
        let mut engine = Engine::open(&base);
        engine.scan("outside").unwrap();
        // Text edits and moved lines aren't status changes.
        edit(
            &base,
            "anim/sh040.md",
            "## Notes",
            "## Notes\n\nA new paragraph.",
        );
        assert!(!engine.scan("outside").unwrap());
        edit(&base, "audio/ep04.md", "- [ ] Final mix", "- [?] Final mix");
        assert!(engine.scan("editor").unwrap());
        let entry = engine.activity().unwrap().entries[0].clone();
        assert_eq!(entry.source, "editor");
        assert_eq!(entry.state, None);
        assert_eq!(engine.apply(entry.id).unwrap_err().status.as_u16(), 409);
        std::fs::remove_dir_all(base).ok();
    }

    #[test]
    fn dismissing_and_applying_stale_edits() {
        let base = sample_copy();
        let mut engine = Engine::open(&base);
        engine.scan("outside").unwrap();
        engine.set_status("anim/sh040", 10, ' ', 'x').unwrap();
        edit(
            &base,
            "fx/sh040-water.md",
            "- [ ] Cache water",
            "- [x] Cache water",
        );
        engine.scan("outside").unwrap();
        let id = engine.activity().unwrap().entries[0].id;
        // The edit is reverted in Obsidian before anyone applies it. (The
        // handler always scans before acting; so does this.)
        edit(
            &base,
            "fx/sh040-water.md",
            "- [x] Cache water",
            "- [ ] Cache water",
        );
        engine.scan("outside").unwrap();
        let result = engine.apply(id).unwrap();
        assert!(
            result.changes.is_empty(),
            "only edits that still hold set anything off"
        );
        assert!(read(&base, "lighting/sh040.md").contains("- [-] Render lighting pass"));

        edit(
            &base,
            "fx/sh040-water.md",
            "- [ ] Cache water",
            "- [x] Cache water",
        );
        engine.scan("outside").unwrap();
        let id = engine.activity().unwrap().entries[0].id;
        engine.dismiss(id).unwrap();
        assert_eq!(
            engine.activity().unwrap().entries[0].state.as_deref(),
            Some("dismissed")
        );
        assert!(read(&base, "lighting/sh040.md").contains("- [-] Render lighting pass"));
        std::fs::remove_dir_all(base).ok();
    }

    /// 230 copies of the sample pipeline (2,070 notes), each copy's wires
    /// pointing within itself, like a studio vault with many shots.
    fn large_vault() -> PathBuf {
        let src = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../sample-vault");
        let dst = std::env::temp_dir().join(format!("possess-large-{}", uuid::Uuid::new_v4()));
        let notes: Vec<(String, String)> = walkdir::WalkDir::new(&src)
            .into_iter()
            .filter_map(Result::ok)
            .filter(|e| e.path().extension().is_some_and(|x| x == "md"))
            .filter_map(|e| {
                let rel = e
                    .path()
                    .strip_prefix(&src)
                    .ok()?
                    .to_string_lossy()
                    .replace('\\', "/");
                let skip = rel.starts_with("_parser-tests")
                    || rel.starts_with("flows")
                    || rel == "README.md";
                (!skip).then(|| (rel, std::fs::read_to_string(e.path()).unwrap()))
            })
            .collect();
        for copy in 0..230 {
            for (rel, text) in &notes {
                let mut out = text.clone();
                for folder in [
                    "anim", "audio", "comp", "delivery", "fx", "lighting", "review", "rig",
                ] {
                    out = out.replace(&format!("[[{folder}/"), &format!("[[s{copy}/{folder}/"));
                }
                let path = dst.join(format!("s{copy}")).join(rel);
                std::fs::create_dir_all(path.parent().unwrap()).unwrap();
                std::fs::write(path, out).unwrap();
            }
        }
        crate::vault::canonical(&dst)
    }

    /// Timed against 2,070 notes. The bounds are loose (this runs
    /// unoptimised, on whatever machine CI has): they catch work that grows
    /// with the vault creeping back into a click, not small slowdowns.
    #[test]
    fn a_large_vault_stays_fast() {
        use std::time::{Duration, Instant};
        let base = large_vault();
        let time = |what: &str, limit: Duration, run: &mut dyn FnMut()| {
            let start = Instant::now();
            run();
            let took = start.elapsed();
            eprintln!("  {what}: {took:?}");
            assert!(took < limit, "{what} took {took:?}, over {limit:?}");
        };

        let mut engine = None;
        time(
            "open (read and parse every note)",
            Duration::from_secs(10),
            &mut || {
                engine = Some(Engine::open(&base));
            },
        );
        let mut engine = engine.unwrap();
        assert_eq!(engine.notes().files().count(), 2070);

        time("click", Duration::from_millis(250), &mut || {
            engine.set_status("s7/anim/sh040", 10, ' ', 'x').unwrap();
        });
        time(
            "click that unblocks",
            Duration::from_millis(250),
            &mut || {
                let result = engine
                    .set_status("s7/fx/sh040-water", 10, ' ', 'x')
                    .unwrap();
                assert_eq!(result.changes.len(), 2);
            },
        );
        time("undo", Duration::from_millis(250), &mut || {
            engine.undo().unwrap();
        });

        edit(
            &base,
            "s9/audio/ep04.md",
            "- [ ] Final mix",
            "- [?] Final mix",
        );
        time(
            "re-read one reported note",
            Duration::from_millis(100),
            &mut || {
                assert!(engine
                    .scan_paths(&[base.join("s9/audio/ep04.md")], "outside")
                    .unwrap());
            },
        );
        time(
            "full rescan, nothing changed",
            Duration::from_secs(5),
            &mut || {
                engine.scan("outside").unwrap();
            },
        );
        time("whole-vault graph", Duration::from_secs(10), &mut || {
            let graph = crate::graph::build(engine.notes(), None).unwrap();
            // Notes with tasks: each copy's review reply has none.
            assert_eq!(graph.nodes.len(), 1840);
        });
        std::fs::remove_dir_all(base).ok();
    }

    #[test]
    fn status_changes_match_tasks_not_lines() {
        let before = "- [ ] One ^a\n- [ ] Two\n- [ ] Same\n- [ ] Same\n";
        let after = "New line\n- [x] One ^a\n- [ ] Two\n- [ ] Same\n- [?] Same\n";
        let changes = status_changes("n", before, after);
        let found: Vec<(usize, char, char)> =
            changes.iter().map(|c| (c.line, c.from, c.to)).collect();
        // One by its id; the duplicate "Same" by its line, which moved, so
        // its change can't be told apart from a new task and isn't reported.
        assert_eq!(found, [(2, ' ', 'x')]);
    }
}
