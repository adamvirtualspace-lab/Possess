//! Tasks, status formats and wires, as SYNTAX.md defines them.
//!
//! SYNTAX.md is shared with Obsessed, the Godot version, so both apps read a
//! vault the same way. `sample-vault/_parser-tests/` holds its tricky cases,
//! and the tests here check each one.
//!
//! - [`parse`] reads one file.
//! - [`set_status`] rewrites one task's status character and nothing else;
//!   [`set_flow_position`] rewrites one flow node's `@ x, y`.
//! - [`Vault`] holds every parsed file and checks what crosses files:
//!   links that point nowhere, and tasks whose status disagrees with their
//!   `needs::`. [`Vault::set_status`] applies a change and the unblocking
//!   and re-blocking it sets off.
//!
//! Positions are 1-based line numbers, never columns, so they mean the same
//! thing in any language.

mod edit;
mod flow;
mod format;
mod parse;
mod vault;

#[cfg(test)]
mod tests;

pub use edit::{set_flow_position, set_status, EditError};
pub use flow::{Flow, FlowNode};
pub use format::{Role, Status, StatusFormat, BUILTIN};
pub use parse::{parse, ParsedFile, Task, TaskRef};
pub use vault::{key as note_key, Change, NeedsState, Vault};

/// A problem found while reading files. The line is left as it is.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Warning {
    /// 1-based.
    pub line: usize,
    pub code: WarningCode,
    /// For people. Tests match on `code`, so wording can change freely.
    pub message: String,
}

impl Warning {
    pub(crate) fn new(line: usize, code: WarningCode, message: String) -> Self {
        Warning {
            line,
            code,
            message,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum WarningCode {
    /// An earlier `::statusformat::` lost to a later one.
    FormatIgnored,
    FormatUnclosed,
    EntryNoChar,
    EntryBadChar,
    EntryIncomplete,
    EntryDuplicate,
    RoleUnknown,
    RoleDuplicate,
    /// No done status, so `[x]` was added.
    DoneAdded,
    /// No open status, so `[ ]` was added.
    OpenAdded,
    /// No status has this role and its default character is taken.
    RoleMissing,
    /// A task uses a character its file's format doesn't have.
    StatusUnknown,
    IdDuplicate,
    /// A wire link without `#^id`.
    LinkNotTask,
    /// A wire link to a file or task that doesn't exist.
    LinkBroken,
    /// A task's saved status disagrees with its `needs::`.
    NeedsMismatch,
    /// An earlier `::flow::` lost to a later one.
    FlowIgnored,
    FlowUnclosed,
    /// A flow node's `@ x, y` isn't two whole numbers.
    PositionBad,
    /// The same note twice in one flow.
    NodeDuplicate,
    /// A flow node links to a note that doesn't exist.
    NodeMissing,
}

impl WarningCode {
    pub fn as_str(self) -> &'static str {
        match self {
            WarningCode::FormatIgnored => "format-ignored",
            WarningCode::FormatUnclosed => "format-unclosed",
            WarningCode::EntryNoChar => "entry-no-char",
            WarningCode::EntryBadChar => "entry-bad-char",
            WarningCode::EntryIncomplete => "entry-incomplete",
            WarningCode::EntryDuplicate => "entry-duplicate",
            WarningCode::RoleUnknown => "role-unknown",
            WarningCode::RoleDuplicate => "role-duplicate",
            WarningCode::DoneAdded => "done-added",
            WarningCode::OpenAdded => "open-added",
            WarningCode::RoleMissing => "role-missing",
            WarningCode::StatusUnknown => "status-unknown",
            WarningCode::IdDuplicate => "id-duplicate",
            WarningCode::LinkNotTask => "link-not-task",
            WarningCode::LinkBroken => "link-broken",
            WarningCode::NeedsMismatch => "needs-mismatch",
            WarningCode::FlowIgnored => "flow-ignored",
            WarningCode::FlowUnclosed => "flow-unclosed",
            WarningCode::PositionBad => "position-bad",
            WarningCode::NodeDuplicate => "node-duplicate",
            WarningCode::NodeMissing => "node-missing",
        }
    }
}
