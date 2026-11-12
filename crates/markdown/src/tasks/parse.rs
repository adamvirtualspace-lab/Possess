//! Reading one file: its status format, tasks, people, IDs and wire lines.
//!
//! Line-based on purpose. pulldown-cmark would find list items, but the rules
//! in SYNTAX.md are stricter than CommonMark (the brackets must follow the
//! marker and one space; numbered lists don't count) and the editor needs the
//! exact bytes of each status character.

use super::format::StatusFormat;
use super::{Warning, WarningCode};

const MARKER: &str = "::statusformat::";

/// A pointer to a task: `[[path#^id]]`, or `[[#^id]]` for the same file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TaskRef {
    /// Vault-relative, without `.md`. `None` means the file the link is in.
    pub path: Option<String>,
    pub id: String,
    /// The wire line the link was written on.
    pub line: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Task {
    /// 1-based.
    pub line: usize,
    /// Leading whitespace in columns, tabs counting as 4. Nesting only.
    pub indent: usize,
    /// The character between the brackets, exactly as written.
    pub ch: char,
    /// False when `ch` isn't in the file's format. The task is still listed,
    /// with a warning, but its line is left alone.
    pub known: bool,
    /// The line after the brackets, without the trailing `^id`.
    pub text: String,
    pub id: Option<String>,
    pub people: Vec<String>,
    pub needs: Vec<TaskRef>,
    pub sync: Vec<TaskRef>,
    /// Byte offset of `ch` in the source, for rewriting just that character.
    pub(crate) status_byte: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParsedFile {
    pub format: StatusFormat,
    pub tasks: Vec<Task>,
    pub warnings: Vec<Warning>,
}

impl ParsedFile {
    pub fn task_by_id(&self, id: &str) -> Option<&Task> {
        self.tasks.iter().find(|t| t.id.as_deref() == Some(id))
    }

    pub fn task_at_line(&self, line: usize) -> Option<&Task> {
        self.tasks.iter().find(|t| t.line == line)
    }
}

/// One line of the source: its number, where it starts, and its text without
/// the line ending.
struct Line<'a> {
    number: usize,
    start: usize,
    text: &'a str,
}

fn lines(src: &str) -> Vec<Line<'_>> {
    let mut out = Vec::new();
    let mut start = 0;
    for (index, raw) in src.split('\n').enumerate() {
        out.push(Line {
            number: index + 1,
            start,
            text: raw.strip_suffix('\r').unwrap_or(raw),
        });
        start += raw.len() + 1;
    }
    out
}

/// A fence opener or closer: its character and length, if the line is one.
fn fence(text: &str) -> Option<(char, usize)> {
    let trimmed = text.trim_start();
    let ch = trimmed.chars().next().filter(|c| *c == '`' || *c == '~')?;
    let run = trimmed.chars().take_while(|c| *c == ch).count();
    (run >= 3).then_some((ch, run))
}

fn indent_of(text: &str) -> (usize, usize) {
    let mut columns = 0;
    let mut bytes = 0;
    for ch in text.chars() {
        match ch {
            ' ' => columns += 1,
            '\t' => columns += 4,
            _ => break,
        }
        bytes += 1;
    }
    (columns, bytes)
}

/// What a line holds, as far as tasks are concerned.
enum Kind<'a> {
    Blank,
    /// A list item `- [c] …`. `ch_byte` is relative to the line start.
    Task {
        ch: char,
        ch_byte: usize,
        rest: &'a str,
    },
    Wire {
        key: WireKey,
        value: &'a str,
    },
    Other,
}

#[derive(Clone, Copy)]
enum WireKey {
    Needs,
    Sync,
}

fn classify(text: &str, indent_bytes: usize) -> Kind<'_> {
    let body = &text[indent_bytes..];
    if body.trim().is_empty() {
        return Kind::Blank;
    }
    let Some(after_marker) = body
        .strip_prefix("- ")
        .or_else(|| body.strip_prefix("* "))
        .or_else(|| body.strip_prefix("+ "))
    else {
        return Kind::Other;
    };

    for (prefix, key) in [("needs::", WireKey::Needs), ("sync::", WireKey::Sync)] {
        if let Some(value) = after_marker.strip_prefix(prefix) {
            return Kind::Wire { key, value };
        }
    }

    let Some(inside) = after_marker.strip_prefix('[') else {
        return Kind::Other;
    };
    let mut chars = inside.chars();
    let (Some(ch), Some(']')) = (chars.next(), chars.next()) else {
        return Kind::Other;
    };
    let rest = &inside[ch.len_utf8() + 1..];
    // `- [x]text` isn't a task: the brackets end the status.
    if !(rest.is_empty() || rest.starts_with(' ') || rest.starts_with('\t')) {
        return Kind::Other;
    }
    Kind::Task {
        ch,
        // indent + marker + space + '['
        ch_byte: indent_bytes + 3,
        rest,
    }
}

/// Split `text ^id` into the text and the ID, if the line ends with one.
fn split_id(rest: &str) -> (&str, Option<&str>) {
    let trimmed = rest.trim_end();
    if let Some(pos) = trimmed.rfind('^') {
        let id = &trimmed[pos + 1..];
        let before = &trimmed[..pos];
        let after_space = before.is_empty() || before.ends_with([' ', '\t']);
        if after_space
            && !id.is_empty()
            && id.chars().all(|c| c.is_ascii_alphanumeric() || c == '-')
        {
            return (before.trim(), Some(id));
        }
    }
    (trimmed.trim(), None)
}

/// `@name` where the `@` starts a word, so `bob@example.com` isn't a mention.
fn people(text: &str) -> Vec<String> {
    let mut found: Vec<String> = Vec::new();
    let mut previous: Option<char> = None;
    for (pos, ch) in text.char_indices() {
        if ch == '@' && previous.is_none_or(char::is_whitespace) {
            let name: String = text[pos + 1..]
                .chars()
                .take_while(|c| c.is_alphanumeric() || *c == '-' || *c == '_')
                .collect();
            if !name.is_empty() && !found.contains(&name) {
                found.push(name);
            }
        }
        previous = Some(ch);
    }
    found
}

/// Every `[[…]]` in a wire line's value that points at a task.
fn task_refs(value: &str, line: usize, warnings: &mut Vec<Warning>) -> Vec<TaskRef> {
    let mut refs = Vec::new();
    let mut rest = value;
    while let Some(open) = rest.find("[[") {
        let Some(close) = rest[open + 2..].find("]]") else {
            break;
        };
        let inner = &rest[open + 2..open + 2 + close];
        rest = &rest[open + 2 + close + 2..];

        let target = inner.split('|').next().unwrap_or("").trim();
        match target.split_once("#^") {
            Some((path, id)) if !id.trim().is_empty() => {
                let path = path.trim();
                let path = path.strip_suffix(".md").unwrap_or(path);
                refs.push(TaskRef {
                    path: (!path.is_empty()).then(|| path.to_string()),
                    id: id.trim().to_string(),
                    line,
                });
            }
            _ => warnings.push(Warning::new(
                line,
                WarningCode::LinkNotTask,
                format!("[[{inner}]] doesn't point at a task. Wires need [[file#^id]]."),
            )),
        }
    }
    refs
}

pub fn parse(src: &str) -> ParsedFile {
    let lines = lines(src);
    let mut warnings = Vec::new();

    // Pass 1: fences and status format blocks. Lines inside either are
    // never tasks.
    let mut skip = vec![false; lines.len()];
    let mut blocks: Vec<Vec<(usize, &str)>> = Vec::new();
    let mut open_fence: Option<(char, usize)> = None;
    let mut i = 0;
    while i < lines.len() {
        let text = lines[i].text;
        if let Some((ch, run)) = open_fence {
            skip[i] = true;
            if let Some((c, r)) = fence(text) {
                if c == ch && r >= run && text.trim().chars().all(|x| x == ch) {
                    open_fence = None;
                }
            }
            i += 1;
            continue;
        }
        if let Some(found) = fence(text) {
            open_fence = Some(found);
            skip[i] = true;
            i += 1;
            continue;
        }
        if let Some(after) = text.strip_prefix(MARKER) {
            // One line: `::statusformat:: … ::statusformat::`
            if let Some(end) = after.find(MARKER) {
                blocks.push(vec![(lines[i].number, &after[..end])]);
                skip[i] = true;
                i += 1;
                continue;
            }
            // Several lines, up to the line holding the closing marker.
            let mut block = vec![(lines[i].number, after)];
            let close = (i + 1..lines.len()).find(|&j| lines[j].text.contains(MARKER));
            match close {
                Some(j) => {
                    for line in &lines[i + 1..j] {
                        block.push((line.number, line.text));
                    }
                    let last = lines[j].text;
                    block.push((lines[j].number, &last[..last.find(MARKER).unwrap_or(0)]));
                    skip[i..=j].iter_mut().for_each(|s| *s = true);
                    blocks.push(block);
                    i = j + 1;
                }
                None => {
                    warnings.push(Warning::new(
                        lines[i].number,
                        WarningCode::FormatUnclosed,
                        "This status format is never closed with ::statusformat::, so it's ignored."
                            .to_string(),
                    ));
                    i += 1;
                }
            }
            continue;
        }
        i += 1;
    }

    // The last format wins, for the whole file. Earlier ones only produce
    // warnings, so their entries are checked against a throwaway list.
    let mut format = StatusFormat::default();
    if let Some((last, earlier)) = blocks.split_last() {
        let winner = last[0].0;
        for block in earlier {
            warnings.push(Warning::new(
                block[0].0,
                WarningCode::FormatIgnored,
                format!("Ignored: the status format on line {winner} comes later, so it wins."),
            ));
        }
        format = StatusFormat::from_entries(last, &mut warnings);
    }

    // Pass 2: tasks and the wire lines under them.
    let mut tasks: Vec<Task> = Vec::new();
    // Tasks a wire line could belong to: (indent, index in `tasks`).
    let mut parents: Vec<(usize, usize)> = Vec::new();

    for (line, skipped) in lines.iter().zip(&skip) {
        if *skipped {
            parents.clear();
            continue;
        }
        let (indent, indent_bytes) = indent_of(line.text);
        let kind = classify(line.text, indent_bytes);
        if !matches!(kind, Kind::Blank) {
            parents.retain(|&(parent_indent, _)| parent_indent < indent);
        }

        match kind {
            Kind::Blank | Kind::Other => {}
            Kind::Wire { key, value } => {
                // A wire line with no task above it is just a list item.
                if let Some(&(_, parent)) = parents.last() {
                    let refs = task_refs(value, line.number, &mut warnings);
                    let task = &mut tasks[parent];
                    match key {
                        WireKey::Needs => task.needs.extend(refs),
                        WireKey::Sync => task.sync.extend(refs),
                    }
                }
            }
            Kind::Task { ch, ch_byte, rest } => {
                let (text, id) = split_id(rest);
                let known = format.get(ch).is_some();
                if !known {
                    warnings.push(Warning::new(
                        line.number,
                        WarningCode::StatusUnknown,
                        format!("[{ch}] isn't in this file's status format. Line left alone."),
                    ));
                }
                let id = id.and_then(|id| {
                    if tasks.iter().any(|t| t.id.as_deref() == Some(id)) {
                        warnings.push(Warning::new(
                            line.number,
                            WarningCode::IdDuplicate,
                            format!("^{id} is already used in this file. The first task keeps it."),
                        ));
                        None
                    } else {
                        Some(id.to_string())
                    }
                });
                parents.push((indent, tasks.len()));
                tasks.push(Task {
                    line: line.number,
                    indent,
                    ch,
                    known,
                    text: text.to_string(),
                    id,
                    people: people(text),
                    needs: Vec::new(),
                    sync: Vec::new(),
                    status_byte: line.start + ch_byte,
                });
            }
        }
    }

    warnings.sort_by_key(|w| w.line);
    ParsedFile {
        format,
        tasks,
        warnings,
    }
}
