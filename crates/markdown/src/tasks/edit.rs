//! Changing a task's status in the source, touching nothing else.
//!
//! A wire may tick a task in a file the user also has open, so the edit has
//! to be surgical: one character swapped, every other byte (trailing spaces,
//! tabs, a missing final newline, CRLF) exactly as it was.

use super::parse::parse;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EditError {
    /// The line isn't a task (it may have changed since it was read).
    NoTask { line: usize },
    /// The new character isn't in the file's status format.
    UnknownStatus { ch: char },
    /// The task's status isn't what the caller last saw: the file changed.
    Stale { line: usize, found: char },
    /// No such file in the vault.
    NoFile { path: String },
    /// The line isn't a node of the note's flow.
    NoNode { line: usize },
}

impl std::fmt::Display for EditError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            EditError::NoTask { line } => write!(f, "line {line} isn't a task"),
            EditError::UnknownStatus { ch } => {
                write!(f, "[{ch}] isn't in this file's status format")
            }
            EditError::Stale { line, found } => write!(
                f,
                "the task on line {line} is now [{found}]: the file changed since it was read"
            ),
            EditError::NoFile { path } => write!(f, "{path} isn't in the vault"),
            EditError::NoNode { line } => write!(f, "line {line} isn't a node of this flow"),
        }
    }
}

impl std::error::Error for EditError {}

/// The source with the task on `line` set to status `ch`.
///
/// It re-parses the source it's given rather than trusting an earlier parse,
/// because the file may have changed since.
pub fn set_status(src: &str, line: usize, ch: char) -> Result<String, EditError> {
    let parsed = parse(src);
    let task = parsed
        .task_at_line(line)
        .ok_or(EditError::NoTask { line })?;
    if parsed.format.get(ch).is_none() {
        return Err(EditError::UnknownStatus { ch });
    }

    let start = task.status_byte;
    let end = start + task.ch.len_utf8();
    let mut out = String::with_capacity(src.len() + 4);
    out.push_str(&src[..start]);
    out.push(ch);
    out.push_str(&src[end..]);
    Ok(out)
}

/// The source with the flow node on `line` moved to `x, y`.
///
/// Everything after the node's link is replaced by ` @ x, y`, so a missing or
/// broken position is fixed and nothing else on the line or in the file moves.
pub fn set_flow_position(src: &str, line: usize, x: i32, y: i32) -> Result<String, EditError> {
    let parsed = parse(src);
    let node = parsed
        .flow
        .as_ref()
        .and_then(|flow| flow.nodes.iter().find(|n| n.line == line))
        .ok_or(EditError::NoNode { line })?;

    let mut out = String::with_capacity(src.len() + 16);
    out.push_str(&src[..node.tail.start]);
    out.push_str(&format!(" @ {x}, {y}"));
    out.push_str(&src[node.tail.end..]);
    Ok(out)
}
