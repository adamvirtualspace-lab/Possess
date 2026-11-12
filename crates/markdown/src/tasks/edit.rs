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
}

impl std::fmt::Display for EditError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            EditError::NoTask { line } => write!(f, "line {line} isn't a task"),
            EditError::UnknownStatus { ch } => {
                write!(f, "[{ch}] isn't in this file's status format")
            }
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
