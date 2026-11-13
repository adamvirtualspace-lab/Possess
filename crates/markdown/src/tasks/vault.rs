//! Every parsed file at once, for what crosses file boundaries.
//!
//! This only reads. Acting on wires (unblocking, sync, chain reactions) is
//! the wire engine's job; here we answer "is this task's need met?" and flag
//! links and statuses that don't add up.

use std::collections::BTreeMap;

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

#[derive(Debug, Clone, Default)]
pub struct Vault {
    /// Keyed by vault-relative path without `.md`, with `/` separators:
    /// the form `[[path#^id]]` links use.
    files: BTreeMap<String, ParsedFile>,
}

/// `anim\sh040.md` → `anim/sh040`.
fn key(path: &str) -> String {
    let path = path.replace('\\', "/");
    path.strip_suffix(".md").unwrap_or(&path).to_string()
}

impl Vault {
    pub fn new() -> Self {
        Self::default()
    }

    /// Parse and add (or replace) one file.
    pub fn insert(&mut self, path: &str, src: &str) {
        self.files.insert(key(path), parse(src));
    }

    pub fn remove(&mut self, path: &str) {
        self.files.remove(&key(path));
    }

    pub fn file(&self, path: &str) -> Option<&ParsedFile> {
        self.files.get(&key(path))
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

    fn is_done(&self, path: &str, task: &Task) -> bool {
        self.files
            .get(path)
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
}
