//! Chain reactions: a status change and everything its wires set off
//! (SYNTAX.md section 6).
//!
//! - **Unblocking:** when every `needs::` of a blocked task is done, it flips
//!   to its file's open status.
//! - **Retakes:** when a needed task stops being done, a waiting task that is
//!   still open goes back to blocked. One that has moved on is left alone.
//! - **Sync:** when a task becomes done, every task with a `sync::` to it is
//!   set to its own file's done status.
//!
//! Each change can set off more, so changes run as a queue until nothing new
//! happens. **Loop guard:** within one chain the wires change each task at
//! most once; a second attempt is skipped and reported. The change that
//! started the chain doesn't count, so a chain can still send its own
//! starting task somewhere once.

use std::collections::{HashSet, VecDeque};

use super::edit::{self, EditError};
use super::format::Role;
use super::parse::parse;
use super::vault::{key, Change, NeedsState, Vault};

/// A change the wires wanted to make but the loop guard skipped.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LoopSkip {
    pub path: String,
    pub line: usize,
    pub text: String,
    /// The status it would have been set to.
    pub wanted: char,
}

/// Every change one chain made, in order, and what the loop guard skipped.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Chain {
    pub changes: Vec<Change>,
    pub loops: Vec<LoopSkip>,
}

impl Vault {
    /// Rewrite one task's status and re-parse its file.
    fn write_status(&mut self, path: &str, line: usize, to: char) -> Result<Change, EditError> {
        let key = key(path);
        let src = self
            .sources
            .get(&key)
            .ok_or_else(|| EditError::NoFile { path: key.clone() })?;
        let task = parse(src)
            .task_at_line(line)
            .cloned()
            .ok_or(EditError::NoTask { line })?;
        let new_src = edit::set_status(src, line, to)?;
        self.insert(&key, &new_src);
        Ok(Change {
            path: key,
            line,
            id: task.id,
            text: task.text,
            from: task.ch,
            to,
        })
    }

    /// Where a task recorded earlier is now. Its old line if that still holds
    /// the same task, else its `^id`, else its text if exactly one task has it.
    pub fn find_task_line(
        &self,
        path: &str,
        line: usize,
        id: Option<&str>,
        text: &str,
    ) -> Option<usize> {
        let file = self.file(path)?;
        if let Some(task) = file.task_at_line(line) {
            let same = match id {
                Some(id) => task.id.as_deref() == Some(id),
                None => task.text == text,
            };
            if same {
                return Some(line);
            }
        }
        if let Some(id) = id {
            return file.task_by_id(id).map(|t| t.line);
        }
        let mut matches = file.tasks.iter().filter(|t| t.text == text);
        match (matches.next(), matches.next()) {
            (Some(task), None) => Some(task.line),
            _ => None,
        }
    }

    /// Set the task on `line` of `path` from `from` to `to`, then run what
    /// that sets off. Nothing is written to disk: the caller saves the
    /// changed files ([`Vault::source`]).
    ///
    /// `from` is the status the caller saw: if the file says otherwise it
    /// changed in the meantime, and nothing changes.
    pub fn set_status(
        &mut self,
        path: &str,
        line: usize,
        from: char,
        to: char,
    ) -> Result<Chain, EditError> {
        let path = key(path);
        let file = self
            .files
            .get(&path)
            .ok_or_else(|| EditError::NoFile { path: path.clone() })?;
        let task = file.task_at_line(line).ok_or(EditError::NoTask { line })?;
        if task.ch != from {
            return Err(EditError::Stale {
                line,
                found: task.ch,
            });
        }
        let first = self.write_status(&path, line, to)?;
        let mut chain = Chain {
            changes: vec![first.clone()],
            loops: Vec::new(),
        };
        self.propagate(&mut chain, vec![first])?;
        Ok(chain)
    }

    /// Run what changes that already happened set off: an edit made in
    /// another app, say, which the vault has already read.
    pub fn follow(&mut self, started_by: &[Change]) -> Result<Chain, EditError> {
        let mut chain = Chain::default();
        self.propagate(&mut chain, started_by.to_vec())?;
        Ok(chain)
    }

    /// What [`Vault::follow`] would do, without changing anything.
    pub fn preview(&self, started_by: &[Change]) -> Chain {
        let mut copy = self.clone();
        copy.follow(started_by).unwrap_or_default()
    }

    /// Put changes back, newest first. All or nothing: if any task no longer
    /// shows what its change set (it was edited since), nothing changes and
    /// the error names it.
    pub fn revert(&mut self, changes: &[Change]) -> Result<Vec<Change>, EditError> {
        let mut copy = self.clone();
        let mut reverted = Vec::new();
        for change in changes.iter().rev() {
            let line = copy
                .find_task_line(
                    &change.path,
                    change.line,
                    change.id.as_deref(),
                    &change.text,
                )
                .ok_or(EditError::NoTask { line: change.line })?;
            let current = copy
                .file(&change.path)
                .and_then(|f| f.task_at_line(line))
                .map(|t| t.ch)
                .ok_or(EditError::NoTask { line })?;
            if current != change.to {
                return Err(EditError::Stale {
                    line,
                    found: current,
                });
            }
            reverted.push(copy.write_status(&change.path, line, change.from)?);
        }
        *self = copy;
        Ok(reverted)
    }

    /// The tasks whose wires react to `event`, and the status each should
    /// get, decided against the vault as it is now.
    fn reactions(&self, event: &Change) -> Vec<(String, usize, char)> {
        let Some(id) = event.id.as_deref() else {
            return Vec::new();
        };
        let Some(file) = self.files.get(&event.path) else {
            return Vec::new();
        };
        let was_done = file.format.role_of(event.from) == Some(Role::Done);
        let now_done = file.format.role_of(event.to) == Some(Role::Done);
        if was_done == now_done {
            return Vec::new();
        }

        let points_here = |from: &str, link: &super::TaskRef| {
            self.resolve(from, link)
                .is_some_and(|(p, t)| p == event.path && t.id.as_deref() == Some(id))
        };

        let mut out = Vec::new();
        for (other_path, other) in &self.files {
            let format = &other.format;
            for waiting in &other.tasks {
                let role = format.role_of(waiting.ch);

                if waiting.needs.iter().any(|l| points_here(other_path, l)) {
                    if let (Some(open), Some(blocked)) = (
                        format.with_role(Role::Open),
                        format.with_role(Role::Blocked),
                    ) {
                        if now_done
                            && role == Some(Role::Blocked)
                            && self.needs_state(other_path, waiting) == NeedsState::Met
                        {
                            out.push((other_path.clone(), waiting.line, open.ch));
                        } else if !now_done && role == Some(Role::Open) {
                            out.push((other_path.clone(), waiting.line, blocked.ch));
                        }
                    }
                }

                if now_done
                    && role != Some(Role::Done)
                    && waiting.sync.iter().any(|l| points_here(other_path, l))
                {
                    if let Some(done) = format.with_role(Role::Done) {
                        out.push((other_path.clone(), waiting.line, done.ch));
                    }
                }
            }
        }
        out
    }

    fn propagate(&mut self, chain: &mut Chain, started_by: Vec<Change>) -> Result<(), EditError> {
        // Tasks the wires have changed in this chain. The starting changes
        // aren't in it: they don't count against the loop guard.
        let mut touched: HashSet<(String, usize)> = HashSet::new();
        let mut queue: VecDeque<Change> = started_by.into();

        while let Some(event) = queue.pop_front() {
            for (path, line, to) in self.reactions(&event) {
                let Some(current) = self
                    .files
                    .get(&path)
                    .and_then(|f| f.task_at_line(line))
                    .cloned()
                else {
                    continue;
                };
                if current.ch == to {
                    continue;
                }
                if !touched.insert((path.clone(), line)) {
                    chain.loops.push(LoopSkip {
                        path,
                        line,
                        text: current.text,
                        wanted: to,
                    });
                    continue;
                }
                let change = self.write_status(&path, line, to)?;
                chain.changes.push(change.clone());
                queue.push_back(change);
            }
        }
        Ok(())
    }
}
