//! User-authored Python scripts, run against the vault.
//!
//! Each run gets the vault path as argv[1] and in the environment:
//!   POSSESS_VAULT  absolute path of the vault
//!   POSSESS_NOTE   the note that triggered it (save-hooks only)
//! stdout/stderr come back to the panel, so print() is the way to report.
//!
//! There is no sandbox: these run as the user, with the user's permissions.

use crate::error::{AppError, AppResult};
use crate::notes::{mtime_secs, trim_rel, walk_notes};
use crate::vault::{self, AppState};
use axum::extract::State;
use axum::Json;
use possess_common::{
    RunScriptRequest, RunScriptResponse, ScaffoldResponse, ScriptInfo, ScriptsResponse,
    SetHooksRequest, SetHooksResponse,
};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::Duration;

pub const SCRIPTS_DIRNAME: &str = "scripts";
pub const HOOKS_DIRNAME: &str = "hooks";
pub const SCRIPT_TIMEOUT: u64 = 30;
pub const HOOK_TIMEOUT: u64 = 10;

/// Worked examples written into a new scripts/ folder on request. They are
/// ordinary files once created — edit or delete them freely.
const EXAMPLE_SCRIPTS: &[(&str, &str)] = &[
    ("README.md", include_str!("../../../scripts_examples/README.md")),
    ("vault_stats.py", include_str!("../../../scripts_examples/vault_stats.py")),
    ("find_todos.py", include_str!("../../../scripts_examples/find_todos.py")),
    (
        "hooks/sync_checkboxes.py",
        include_str!("../../../scripts_examples/hooks/sync_checkboxes.py"),
    ),
];

/// The interpreter to run scripts with.
///
/// Python's sys.executable pointed at the very interpreter running the server;
/// a compiled binary has no such thing, so look for one on PATH. POSSESS_PYTHON
/// overrides for a venv or a non-standard install.
pub fn python_executable() -> String {
    if let Ok(explicit) = std::env::var("POSSESS_PYTHON") {
        if !explicit.trim().is_empty() {
            return explicit;
        }
    }

    let candidates: &[&str] = if cfg!(windows) {
        &["python.exe", "python3.exe"]
    } else {
        &["python3", "python"]
    };

    for candidate in candidates {
        if which(candidate).is_some() {
            return candidate.to_string();
        }
    }
    candidates[0].to_string()
}

/// First match for `name` on PATH.
fn which(name: &str) -> Option<PathBuf> {
    let path = std::env::var_os("PATH")?;
    std::env::split_paths(&path)
        .map(|dir| dir.join(name))
        .find(|p| p.is_file())
}

pub fn scripts_dir(vault: &Path) -> PathBuf {
    vault.join(SCRIPTS_DIRNAME)
}

/// Every runnable script in the vault, as (manual list, hook paths).
pub fn list_scripts_in(vault: &Path) -> (Vec<ScriptInfo>, Vec<PathBuf>) {
    let root = scripts_dir(vault);
    let (mut listed, mut hooks) = (Vec::new(), Vec::new());

    if !root.is_dir() {
        return (listed, hooks);
    }

    for path in sorted_py_files(&root) {
        let name = path.file_name().unwrap_or_default().to_string_lossy().to_string();
        listed.push(ScriptInfo { name, kind: "manual".into() });
    }

    let hooks_root = root.join(HOOKS_DIRNAME);
    if hooks_root.is_dir() {
        for path in sorted_py_files(&hooks_root) {
            let name = path.file_name().unwrap_or_default().to_string_lossy().to_string();
            listed.push(ScriptInfo { name: format!("{HOOKS_DIRNAME}/{name}"), kind: "hook".into() });
            hooks.push(path);
        }
    }

    (listed, hooks)
}

/// Runnable .py files directly in `dir`, sorted by name.
///
/// A leading underscore marks a helper module the user imports rather than a
/// script to run, so those are skipped.
fn sorted_py_files(dir: &Path) -> Vec<PathBuf> {
    let mut files: Vec<PathBuf> = std::fs::read_dir(dir)
        .into_iter()
        .flatten()
        .filter_map(Result::ok)
        .map(|e| e.path())
        .filter(|p| {
            p.is_file()
                && p.extension().is_some_and(|e| e == "py")
                && !p.file_name().unwrap_or_default().to_string_lossy().starts_with('_')
        })
        .collect();
    files.sort();
    files
}

/// Resolve a script name from the client inside <vault>/scripts/.
fn resolve_script(vault: &Path, name: &str) -> AppResult<PathBuf> {
    let root = vault::canonical(&scripts_dir(vault));
    let full = vault::resolve_in_vault(&root, name)
        .map_err(|_| AppError::forbidden("Access denied"))?;

    if full.extension().is_none_or(|e| e != "py") || !full.is_file() {
        return Err(AppError::not_found(format!("No such script: {name}")));
    }
    Ok(full)
}

/// Run one script and capture what it did.
async fn run_one(path: &Path, vault: &Path, note: Option<&str>, timeout: u64) -> RunScriptResponse {
    let mut command = tokio::process::Command::new(python_executable());
    command
        .arg(path)
        .arg(vault)
        .current_dir(vault)
        .env("POSSESS_VAULT", vault)
        .env("PYTHONUNBUFFERED", "1")
        .kill_on_drop(true);

    if let Some(note) = note {
        command.env("POSSESS_NOTE", note);
    }

    let name = path.file_name().unwrap_or_default().to_string_lossy().to_string();
    let started = std::time::Instant::now();

    let (stdout, stderr, code) =
        match tokio::time::timeout(Duration::from_secs(timeout), command.output()).await {
            Ok(Ok(out)) => (
                String::from_utf8_lossy(&out.stdout).to_string(),
                String::from_utf8_lossy(&out.stderr).to_string(),
                out.status.code().unwrap_or(-1),
            ),
            // kill_on_drop reaps the process when `command.output()` is dropped
            // here, so a runaway script does not outlive its timeout.
            Err(_) => (
                String::new(),
                format!("Timed out after {timeout}s — script killed."),
                -1,
            ),
            Ok(Err(err)) => (String::new(), format!("Could not start script: {err}"), -1),
        };

    RunScriptResponse {
        script: name,
        code,
        stdout: tail(&stdout, 20000),
        stderr: tail(&stderr, 20000),
        seconds: (started.elapsed().as_secs_f64() * 100.0).round() / 100.0,
        changed: Vec::new(),
    }
}

/// The last `limit` bytes, on a character boundary.
fn tail(text: &str, limit: usize) -> String {
    if text.len() <= limit {
        return text.to_string();
    }
    let mut start = text.len() - limit;
    while start < text.len() && !text.is_char_boundary(start) {
        start += 1;
    }
    text[start..].to_string()
}

/// Run every hook after a save.
///
/// Never propagates a failure — a broken hook must not take the save down with
/// it; its output goes to the server log.
pub async fn run_save_hooks(state: &AppState, note: &str) {
    let vault_path = state.vault_resolved();
    if !vault::hooks_enabled(&vault_path) {
        return;
    }

    let (_, hooks) = list_scripts_in(&vault_path);
    if hooks.is_empty() {
        return;
    }

    // One hook run at a time, and never queued: autosave fires every 5s, and a
    // hook slower than that would otherwise pile up runs that fight each other
    // over the same files. try_lock drops this run rather than waiting.
    let Ok(_guard) = state.hook_lock.try_lock() else {
        return;
    };

    for hook in hooks {
        let result = run_one(&hook, &vault_path, Some(note), HOOK_TIMEOUT).await;
        let name = &result.script;

        if result.code != 0 {
            let err = result.stderr.trim();
            eprintln!(
                "[PossessApp] hook {name} failed ({}): {}",
                result.code,
                truncate(err, 400)
            );
        } else {
            let out = result.stdout.trim();
            if !out.is_empty() {
                println!("[PossessApp] hook {name}: {}", truncate(out, 400));
            }
        }
    }
}

fn truncate(text: &str, limit: usize) -> &str {
    match text.char_indices().nth(limit) {
        Some((idx, _)) => &text[..idx],
        None => text,
    }
}

pub async fn list_scripts(State(state): State<AppState>) -> AppResult<Json<ScriptsResponse>> {
    let vault_path = state.vault_resolved();
    let (scripts, _) = list_scripts_in(&vault_path);
    let dir = scripts_dir(&vault_path);

    Ok(Json(ScriptsResponse {
        scripts,
        exists: dir.is_dir(),
        dir: dir.display().to_string(),
        hooks_enabled: vault::hooks_enabled(&vault_path),
    }))
}

pub async fn run_script(
    State(state): State<AppState>,
    Json(body): Json<RunScriptRequest>,
) -> AppResult<Json<RunScriptResponse>> {
    let name = trim_rel(body.name.as_deref().unwrap_or(""));
    if name.is_empty() {
        return Err(AppError::bad_request("No script named"));
    }

    let vault_path = state.vault_resolved();
    let script = resolve_script(&vault_path, &name)?;

    // Snapshot before/after so the UI knows whether to reload the open note and
    // rebuild the tree, instead of waiting for the 5s sync poll.
    let before = vault_snapshot(&vault_path);
    let mut result = run_one(&script, &vault_path, body.note.as_deref(), SCRIPT_TIMEOUT).await;
    let after = vault_snapshot(&vault_path);

    result.changed = changed_since(&before, &after);
    Ok(Json(result))
}

pub async fn set_hooks(
    State(state): State<AppState>,
    Json(body): Json<SetHooksRequest>,
) -> AppResult<Json<SetHooksResponse>> {
    let vault_path = state.vault_resolved();
    vault::set_hooks_enabled(&vault_path, body.enabled);

    println!(
        "[PossessApp] Save-hooks {} for {}",
        if body.enabled { "enabled" } else { "disabled" },
        vault_path.display()
    );
    Ok(Json(SetHooksResponse { hooks_enabled: body.enabled }))
}

/// Create scripts/ with worked examples, on explicit request only.
pub async fn scaffold_scripts(State(state): State<AppState>) -> AppResult<Json<ScaffoldResponse>> {
    let vault_path = state.vault_resolved();
    let root = scripts_dir(&vault_path);
    std::fs::create_dir_all(root.join(HOOKS_DIRNAME))?;

    let mut written = Vec::new();
    for (rel, source) in EXAMPLE_SCRIPTS {
        let target = root.join(rel);
        if target.exists() {
            continue;
        }
        std::fs::write(&target, source)?;
        written.push(rel.to_string());
    }

    Ok(Json(ScaffoldResponse { created: written, dir: root.display().to_string() }))
}

fn vault_snapshot(vault: &Path) -> HashMap<String, f64> {
    walk_notes(vault)
        .map(|e| (e.path().display().to_string(), mtime_secs(e.path())))
        .collect()
}

/// Notes created, deleted or rewritten between two snapshots.
fn changed_since(before: &HashMap<String, f64>, after: &HashMap<String, f64>) -> Vec<String> {
    let mut changed: Vec<String> = before
        .keys()
        .chain(after.keys())
        .filter(|path| match (before.get(*path), after.get(*path)) {
            (Some(a), Some(b)) => a != b,
            _ => true,
        })
        .cloned()
        .collect();

    changed.sort();
    changed.dedup();
    changed
}
