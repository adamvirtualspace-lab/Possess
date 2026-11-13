//! `::statusformat::`: the statuses a file uses, and what each one means.
//!
//! SYNTAX.md section 2. Each entry is `[c] = symbol = label`, plus an optional
//! `= role`. The order is the pipeline order: clicking walks it, and statuses
//! after the done one are reachable only from the right-click menu.

use super::{Warning, WarningCode};

/// What a status means to the wires. Labels and symbols are for people.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Role {
    Open,
    Done,
    Blocked,
}

impl Role {
    fn parse(raw: &str) -> Option<Role> {
        match raw {
            "open" => Some(Role::Open),
            "done" => Some(Role::Done),
            "blocked" => Some(Role::Blocked),
            _ => None,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Role::Open => "open",
            Role::Done => "done",
            Role::Blocked => "blocked",
        }
    }
}

/// One entry of a status format.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Status {
    /// What goes between the task's brackets.
    pub ch: char,
    /// What the app draws instead of the brackets.
    pub symbol: String,
    pub label: String,
    pub role: Option<Role>,
}

/// The statuses that apply to one file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StatusFormat {
    /// In pipeline order.
    pub statuses: Vec<Status>,
    /// 1-based line of the block this came from; `None` for the built-in default.
    pub line: Option<usize>,
}

/// The format of a file that doesn't declare one (SYNTAX.md section 2).
pub const BUILTIN: &str = "[-] = ⚪ = cannot start = blocked, [ ] = 🔳 = ready to start, \
                           [?] = ❓ = ready to check, [x] = ✅ = Finished";

impl Default for StatusFormat {
    fn default() -> Self {
        let mut ignored = Vec::new();
        let mut format = StatusFormat::from_entries(&[(1, BUILTIN)], &mut ignored);
        format.line = None;
        format
    }
}

impl StatusFormat {
    /// Build a format from the text between the markers. `lines` pairs each
    /// line of that text with its 1-based line number in the file, so a
    /// warning points at the entry, not just the block.
    pub(crate) fn from_entries(lines: &[(usize, &str)], warnings: &mut Vec<Warning>) -> Self {
        let block_line = lines.first().map(|(line, _)| *line).unwrap_or(1);
        let mut statuses: Vec<Status> = Vec::new();

        for &(line, text) in lines {
            for piece in text.split(',') {
                let piece = piece.trim();
                if piece.is_empty() {
                    continue;
                }
                match parse_entry(piece) {
                    Ok((ch, symbol, label, role)) => {
                        if statuses.iter().any(|s| s.ch == ch) {
                            warnings.push(Warning::new(
                                line,
                                WarningCode::EntryDuplicate,
                                format!(
                                    "[{ch}] is in the status format twice. The first one is used."
                                ),
                            ));
                            continue;
                        }
                        let role = role.and_then(|raw| {
                            let parsed = Role::parse(raw);
                            match parsed {
                                None => warnings.push(Warning::new(
                                    line,
                                    WarningCode::RoleUnknown,
                                    format!("{raw} isn't a role, so [{ch}] works without one."),
                                )),
                                Some(role) if statuses.iter().any(|s| s.role == Some(role)) => {
                                    warnings.push(Warning::new(
                                        line,
                                        WarningCode::RoleDuplicate,
                                        format!(
                                            "Another status is already {}, so [{ch}] works without a role.",
                                            role.as_str()
                                        ),
                                    ));
                                    return None;
                                }
                                Some(_) => {}
                            }
                            parsed
                        });
                        statuses.push(Status {
                            ch,
                            symbol,
                            label,
                            role,
                        });
                    }
                    Err(problem) => warnings.push(Warning::new(
                        line,
                        problem.code(),
                        format!("{} {}. Skipped.", describe(piece), problem.reason()),
                    )),
                }
            }
        }

        let mut format = StatusFormat {
            statuses,
            line: Some(block_line),
        };
        format.fill_role(Role::Done, 'x', "✅", "Finished", block_line, warnings);
        format.fill_role(
            Role::Open,
            ' ',
            "🔳",
            "ready to start",
            block_line,
            warnings,
        );
        format
    }

    /// Plain markdown's defaults: `[ ]` is open and `[x]` is done unless an
    /// entry claims the role. With neither, the status is added, with a warning.
    fn fill_role(
        &mut self,
        role: Role,
        ch: char,
        symbol: &str,
        label: &str,
        line: usize,
        warnings: &mut Vec<Warning>,
    ) {
        if self.with_role(role).is_some() {
            return;
        }
        match self.statuses.iter_mut().find(|s| s.ch == ch) {
            Some(status) if status.role.is_none() => status.role = Some(role),
            // [x] is taken by something else that has a role of its own.
            Some(_) => warnings.push(Warning::new(
                line,
                WarningCode::RoleMissing,
                format!("No status is {}.", role.as_str()),
            )),
            None => {
                let code = match role {
                    Role::Done => WarningCode::DoneAdded,
                    _ => WarningCode::OpenAdded,
                };
                warnings.push(Warning::new(
                    line,
                    code,
                    format!(
                        "No status is {}, so [{ch}] = {symbol} {label} is added as {}.",
                        role.as_str(),
                        role.as_str()
                    ),
                ));
                self.statuses.push(Status {
                    ch,
                    symbol: symbol.to_string(),
                    label: label.to_string(),
                    role: Some(role),
                });
            }
        }
    }

    /// The status a task's character stands for. A capital X counts as x when
    /// the format has no X of its own, as in plain markdown.
    pub fn get(&self, ch: char) -> Option<&Status> {
        self.statuses.iter().find(|s| s.ch == ch).or_else(|| {
            (ch == 'X')
                .then(|| self.statuses.iter().find(|s| s.ch == 'x'))
                .flatten()
        })
    }

    pub fn role_of(&self, ch: char) -> Option<Role> {
        self.get(ch).and_then(|s| s.role)
    }

    pub fn with_role(&self, role: Role) -> Option<&Status> {
        self.statuses.iter().find(|s| s.role == Some(role))
    }

    fn index_of(&self, ch: char) -> Option<usize> {
        let canonical = self.get(ch)?.ch;
        self.statuses.iter().position(|s| s.ch == canonical)
    }

    /// Statuses after the done one (a retake, say) are only offered by the
    /// right-click menu, so a click can never land on them.
    pub fn is_right_click_only(&self, ch: char) -> bool {
        let done = self
            .statuses
            .iter()
            .position(|s| s.role == Some(Role::Done));
        matches!((self.index_of(ch), done), (Some(i), Some(d)) if i > d)
    }

    /// Where a click sends a task: the next status in order, skipping blocked.
    /// `None` when a click does nothing: on a done task, so an approved shot
    /// can't be un-approved by accident, or past the end of the order.
    pub fn next_on_click(&self, ch: char) -> Option<char> {
        if self.role_of(ch) == Some(Role::Done) {
            return None;
        }
        let from = self.index_of(ch)?;
        self.statuses[from + 1..]
            .iter()
            .find(|s| s.role != Some(Role::Blocked))
            .map(|s| s.ch)
    }
}

enum EntryProblem {
    NoChar,
    BadChar,
    Incomplete,
}

impl EntryProblem {
    fn code(&self) -> WarningCode {
        match self {
            EntryProblem::NoChar => WarningCode::EntryNoChar,
            EntryProblem::BadChar => WarningCode::EntryBadChar,
            EntryProblem::Incomplete => WarningCode::EntryIncomplete,
        }
    }

    fn reason(&self) -> &'static str {
        match self {
            EntryProblem::NoChar => "has no status character",
            EntryProblem::BadChar => "needs exactly one character between the brackets",
            EntryProblem::Incomplete => "needs [character] = symbol = label",
        }
    }
}

/// How a broken entry is named in its warning: by its brackets if it has them.
fn describe(piece: &str) -> String {
    match piece.find(']') {
        Some(end) if piece.starts_with('[') => format!("Entry {}", &piece[..=end]),
        _ => format!("Entry \"{piece}\""),
    }
}

type Entry<'a> = (char, String, String, Option<&'a str>);

/// `[c] = symbol = label [= role]`, already trimmed.
fn parse_entry(piece: &str) -> Result<Entry<'_>, EntryProblem> {
    let mut parts = piece.split('=').map(str::trim);
    let bracket = parts.next().unwrap_or("");

    let inner = bracket
        .strip_prefix('[')
        .and_then(|rest| rest.strip_suffix(']'))
        .ok_or(EntryProblem::Incomplete)?;
    let mut chars = inner.chars();
    let ch = match (chars.next(), chars.next()) {
        (None, _) => return Err(EntryProblem::NoChar),
        (Some(ch), None) => ch,
        (Some(_), Some(_)) => return Err(EntryProblem::BadChar),
    };

    let symbol = parts
        .next()
        .filter(|s| !s.is_empty())
        .ok_or(EntryProblem::Incomplete)?;
    let label = parts
        .next()
        .filter(|s| !s.is_empty())
        .ok_or(EntryProblem::Incomplete)?;
    let role = parts.next().filter(|s| !s.is_empty());
    if parts.next().is_some() {
        return Err(EntryProblem::Incomplete);
    }

    Ok((ch, symbol.to_string(), label.to_string(), role))
}
