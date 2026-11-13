//! Every parsed file at once, for what crosses file boundaries.
//!
//! It answers "is this task's need met?", flags links and statuses that don't
//! add up, and applies a status change together with what it sets off
//! (SYNTAX.md section 6): unblocking the tasks that were waiting on it, or
//! blocking them again on a retake.

use std::collections::BTreeMap;

use super::edit::{self, EditError};
use super::format::Role;
use super::parse::{parse, ParsedFile, Task, TaskRef};
use super::{Warning, WarningCode};

/// Whether a task's `needs::` are satisfied.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NeedsState {
    /// The task has no `needs::`.
    None,
    /// Every needed task is done.
    Met,
    /// At least one needed task isn't done yet.
    Unmet,
    /// At least one link points nowhere, so the need can never be met.
    Broken,
}

/// One status change written to a file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Change {
    /// Vault key: relative path without `.md`.
    pub path: String,
    pub line: usize,
    pub from: char,
    pub to: char,
}

#[derive(Debug, Clone, Default)]
pub struct Vault {
    /// Keyed by vault-relative path without `.md`, with `/` separators:
    /// the form `[[path#^id]]` links use.
    files: BTreeMap<String, ParsedFile>,
    sources: BTreeMap<String, String>,
}

/// `anim\sh040.md` → `anim/sh040`.
pub fn key(path: &str) -> String {
    let path = path.replace('\\', "/");
    path.strip_suffix(".md").unwrap_or(&path).to_string()
}

impl Vault {
    pub fn new() -> Self {
        Self::default()
    }

    /// Parse and add (or replace) one file.
    pub fn insert(&mut self, path: &str, src: &str) {
        let key = key(path);
        self.files.insert(key.clone(), parse(src));
        self.sources.insert(key, src.to_string());
    }

    pub fn remove(&mut self, path: &str) {
        self.files.remove(&key(path));
        self.sources.remove(&key(path));
    }

    pub fn file(&self, path: &str) -> Option<&ParsedFile> {
        self.files.get(&key(path))
    }

    pub fn source(&self, path: &str) -> Option<&str> {
        self.sources.get(&key(path)).map(String::as_str)
    }

    pub fn files(&self) -> impl Iterator<Item = (&str, &ParsedFile)> {
        self.files.iter().map(|(k, v)| (k.as_str(), v))
    }

    /// The task a link points at, and its file's key.
    pub fn resolve(&self, from: &str, link: &TaskRef) -> Option<(&str, &Task)> {
        let path = match &link.path {
            Some(path) => key(path),
            None => key(from),
        };
        let (path, file) = self.files.get_key_value(&path)?;
        Some((path.as_str(), file.task_by_id(&link.id)?))
    }

    pub fn is_done(&self, path: &str, task: &Task) -> bool {
        self.files
            .get(&key(path))
            .is_some_and(|file| file.format.role_of(task.ch) == Some(Role::Done))
    }

    pub fn needs_state(&self, path: &str, task: &Task) -> NeedsState {
        if task.needs.is_empty() {
            return NeedsState::None;
        }
        let mut met = true;
        for link in &task.needs {
            match self.resolve(path, link) {
                None => return NeedsState::Broken,
                Some((target_path, target)) => met &= self.is_done(target_path, target),
            }
        }
        if met {
            NeedsState::Met
        } else {
            NeedsState::Unmet
        }
    }

    /// Every file's own warnings plus the ones that cross files, by file.
    pub fn warnings(&self) -> BTreeMap<&str, Vec<Warning>> {
        let mut out: BTreeMap<&str, Vec<Warning>> = BTreeMap::new();
        for (path, file) in &self.files {
            let mut list = file.warnings.clone();
            for task in &file.tasks {
                for link in task.needs.iter().chain(&task.sync) {
                    if self.resolve(path, link).is_none() {
                        let target = link.path.as_deref().unwrap_or("");
                        list.push(Warning::new(
                            link.line,
                            WarningCode::LinkBroken,
                            format!("[[{target}#^{}]] doesn't point at a task.", link.id),
                        ));
                    }
                }
                if let Some(message) = self.mismatch(path, file, task) {
                    list.push(Warning::new(task.line, WarningCode::NeedsMismatch, message));
                }
            }
            for node in file.flow.iter().flat_map(|flow| &flow.nodes) {
                if !self.files.contains_key(&key(&node.path)) {
                    list.push(Warning::new(
                        node.line,
                        WarningCode::NodeMissing,
                        format!("[[{}]] isn't a note in this vault.", node.path),
                    ));
                }
            }
            list.sort_by_key(|w| w.line);
            if !list.is_empty() {
                out.insert(path.as_str(), list);
            }
        }
        out
    }

    /// Opening a vault never edits it, so a saved status that disagrees with
    /// its `needs::` is only flagged, until something changes.
    fn mismatch(&self, path: &str, file: &ParsedFile, task: &Task) -> Option<String> {
        let role = file.format.role_of(task.ch);
        let blocked = file.format.with_role(Role::Blocked)?;
        let open = file.format.with_role(Role::Open)?;
        match (self.needs_state(path, task), role) {
            (NeedsState::Met, Some(Role::Blocked)) => Some(format!(
                "Its needs are met, so it should be {} {}, not {} {}.",
                open.symbol, open.label, blocked.symbol, blocked.label
            )),
            (NeedsState::Unmet, Some(Role::Open)) => Some(format!(
                "Its needs aren't met yet, so it should be {} {}.",
                blocked.symbol, blocked.label
            )),
            _ => None,
        }
    }

    /// Rewrite one file's status character and re-parse it.
    fn write_status(&mut self, path: &str, line: usize, to: char) -> Result<char, EditError> {
        let key = key(path);
        let src = self
            .sources
            .get(&key)
            .ok_or_else(|| EditError::NoFile { path: key.clone() })?;
        let from = parse(src)
            .task_at_line(line)
            .ok_or(EditError::NoTask { line })?
            .ch;
        let new_src = edit::set_status(src, line, to)?;
        self.insert(&key, &new_src);
        Ok(from)
    }

    /// Set the task on `line` of `path` from `from` to `to`, then apply what
    /// that sets off, and return every change in order. The caller writes
    /// the changed files ([`Vault::source`]).
    ///
    /// `from` is the status the caller saw: if the file says otherwise it
    /// changed in the meantime, and nothing is written.
    ///
    /// - **Unblocking:** when the task becomes done, each blocked task that
    ///   needs it and now has every need met flips to its file's open status.
    /// - **Retakes:** when a done task stops being done, each task that needs
    ///   it and is still open goes back to blocked. One that has moved on is
    ///   left alone.
    ///
    /// Files without a blocked status get neither: the app shows a lock and
    /// writes nothing. A flip never makes a task done, so it can't set off
    /// anything further.
    pub fn set_status(
        &mut self,
        path: &str,
        line: usize,
        from: char,
        to: char,
    ) -> Result<Vec<Change>, EditError> {
        let path = key(path);
        let file = self
            .files
            .get(&path)
            .ok_or_else(|| EditError::NoFile { path: path.clone() })?;
        let task = file.task_at_line(line).ok_or(EditError::NoTask { line })?;
        if task.ch != from {
            return Err(EditError::Stale { line, found: task.ch });
        }
        let id = task.id.clone();
        let was_done = file.format.role_of(from) == Some(Role::Done);
        let now_done = file.format.role_of(to) == Some(Role::Done);

        self.write_status(&path, line, to)?;
        let mut changes = vec![Change { path: path.clone(), line, from, to }];

        let Some(id) = id else { return Ok(changes) };
        if was_done == now_done {
            return Ok(changes);
        }

        // Every task that needs the changed one, decided against the vault
        // as it is now, before any flip is written.
        let mut flips: Vec<(String, usize, char)> = Vec::new();
        for (other_path, other) in &self.files {
            for waiting in &other.tasks {
                let needs_it = waiting.needs.iter().any(|link| {
                    self.resolve(other_path, link)
                        .is_some_and(|(p, t)| p == path && t.id.as_deref() == Some(id.as_str()))
                });
                if !needs_it {
                    continue;
                }
                let format = &other.format;
                let (Some(open), Some(blocked)) =
                    (format.with_role(Role::Open), format.with_role(Role::Blocked))
                else {
                    continue;
                };
                let role = format.role_of(waiting.ch);
                if now_done
                    && role == Some(Role::Blocked)
                    && self.needs_state(other_path, waiting) == NeedsState::Met
                {
                    flips.push((other_path.clone(), waiting.line, open.ch));
                } else if !now_done && role == Some(Role::Open) {
                    flips.push((other_path.clone(), waiting.line, blocked.ch));
                }
            }
        }

        for (flip_path, flip_line, flip_to) in flips {
            let flip_from = self.write_status(&flip_path, flip_line, flip_to)?;
            changes.push(Change { path: flip_path, line: flip_line, from: flip_from, to: flip_to });
        }
        Ok(changes)
    }
}
