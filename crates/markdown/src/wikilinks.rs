//! `[[Wiki links]]` — the shorthand that turns a folder of markdown into a vault.
//!
//! ```text
//! [[Ideas]]              by name, found anywhere in the vault
//! [[projects/Ideas]]     by path, when two notes share a name
//! [[Ideas|my ideas]]     with different display text
//! [[Ideas#Later]]        the heading is kept for display, ignored for lookup
//! ```
//!
//! Resolution runs against the note list the sidebar already holds, so typing a
//! link doesn't cost a round trip.

/// The parts of one `[[…]]`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WikiLink {
    /// Everything before the `|`, heading included.
    pub target: String,
    /// The part used for lookup: no alias, no `#heading`.
    pub path: String,
    pub alias: String,
    /// What to display: the alias when given, else the whole target.
    pub label: String,
}

/// Split "target|alias" into its parts, dropping any #heading from the path
/// used for lookup but keeping it in the default display text.
pub fn parse(raw: &str) -> WikiLink {
    let (target_part, alias) = match raw.split_once('|') {
        Some((target, alias)) => (target, alias.trim().to_string()),
        None => (raw, String::new()),
    };

    let target = target_part.trim().to_string();
    let path = target.split('#').next().unwrap_or("").trim().to_string();
    let label = if alias.is_empty() { target.clone() } else { alias.clone() };

    WikiLink { target, path, alias, label }
}

/// Vault-relative path of the note a link points at, or None if nothing matches.
///
/// Order matters: an explicit path must win over a same-named note elsewhere,
/// or `[[projects/Ideas]]` could open the wrong Ideas.
pub fn resolve(path: &str, from_note: Option<&str>, notes: &[String]) -> Option<String> {
    if path.is_empty() {
        return None;
    }

    let with_ext = if path.ends_with(".md") {
        path.to_string()
    } else {
        format!("{path}.md")
    };

    let dir = from_note
        .filter(|note| note.contains('/'))
        .map(|note| &note[..note.rfind('/').unwrap()]);

    let exact = |candidate: &str| -> Option<String> {
        notes
            .iter()
            .find(|note| note.eq_ignore_ascii_case(candidate))
            .cloned()
    };

    // 1. Beside the note that links to it, 2. from the vault root.
    if let Some(dir) = dir {
        if let Some(found) = exact(&format!("{dir}/{with_ext}")) {
            return Some(found);
        }
    }
    if let Some(found) = exact(&with_ext) {
        return Some(found);
    }

    // 3. By filename anywhere. Several notes can share a name, so prefer the
    // nearest: same folder first, then the shallowest path, so the answer is
    // stable rather than dependent on directory-walk order.
    let wanted = with_ext.rsplit('/').next().unwrap_or(&with_ext).to_lowercase();
    let mut matches: Vec<&String> = notes
        .iter()
        .filter(|note| {
            note.rsplit('/').next().unwrap_or(note).to_lowercase() == wanted
        })
        .collect();

    if matches.is_empty() {
        return None;
    }

    let near = |path: &str| -> u8 {
        match dir {
            Some(dir) if path.starts_with(&format!("{dir}/")) => 0,
            _ => 1,
        }
    };

    matches.sort_by(|a, b| {
        near(a)
            .cmp(&near(b))
            .then_with(|| a.split('/').count().cmp(&b.split('/').count()))
            .then_with(|| a.cmp(b))
    });

    matches.first().map(|found| (*found).clone())
}

/// Byte ranges of every `[[…]]` in `text`, code spans included.
///
/// For the renderer, which knows from the parse what is code and never offers
/// those runs here. A link may not span a newline, so this is safe to run over
/// a whole document.
pub fn all_ranges(text: &str) -> Vec<(usize, usize)> {
    let mut found = Vec::new();
    let mut offset = 0;

    while let Some((start, end)) = next_link(&text[offset..]) {
        found.push((offset + start, offset + end));
        offset += end;
    }
    found
}

/// Byte ranges of the `[[…]]` links on one line, skipping any inside a
/// `code span`.
///
/// For the editor overlay, which styles links in the raw source and has no
/// parse to consult, so it has to spot code spans itself.
pub fn ranges(text: &str) -> Vec<(usize, usize)> {
    let code = code_spans(text);
    let mut found = Vec::new();
    let mut offset = 0;

    while let Some((start, end)) = next_link(&text[offset..]) {
        let (start, end) = (offset + start, offset + end);
        if !code.iter().any(|(from, to)| start >= *from && start < *to) {
            found.push((start, end));
        }
        offset = end;
    }
    found
}

/// The byte range of the first `[[target|alias]]` in `text`.
///
/// Neither part may span a newline or contain a `]`, and the target may not
/// contain a `|` — that is what separates it from the alias.
fn next_link(text: &str) -> Option<(usize, usize)> {
    let mut search = 0;
    loop {
        let open = text[search..].find("[[")? + search;
        let after = open + 2;

        match link_body_end(&text[after..]) {
            Some(len) => return Some((open, after + len + 2)),
            // Not a link after all — keep looking past these brackets.
            None => search = after,
        }
    }
}

/// Length of the body of a link starting just after `[[`, or None if what
/// follows never closes as one.
fn link_body_end(rest: &str) -> Option<usize> {
    let mut seen_pipe = false;
    let mut target_len = 0;

    for (index, ch) in rest.char_indices() {
        match ch {
            '\n' => return None,
            ']' => {
                // Must be "]]", and the target must not have been empty.
                if !rest[index..].starts_with("]]") || (!seen_pipe && target_len == 0) {
                    return None;
                }
                return Some(index);
            }
            '|' if !seen_pipe => {
                if target_len == 0 {
                    return None;
                }
                seen_pipe = true;
            }
            '|' => return None,
            _ if !seen_pipe => target_len += ch.len_utf8(),
            _ => {}
        }
    }
    None
}

/// Byte ranges of the `` `code spans` `` on one line.
pub fn code_spans(text: &str) -> Vec<(usize, usize)> {
    let mut spans = Vec::new();
    let mut open: Option<usize> = None;

    for (index, ch) in text.char_indices() {
        if ch != '`' {
            continue;
        }
        match open {
            None => open = Some(index),
            Some(start) => {
                spans.push((start, index + 1));
                open = None;
            }
        }
    }
    spans
}

/// The link surrounding a byte offset, if the offset is inside one.
///
/// Returns the raw body — "target" or "target|alias" — ready for `parse`.
pub fn link_at(line: &str, offset: usize) -> Option<String> {
    ranges(line).into_iter().find_map(|(start, end)| {
        (offset >= start && offset <= end).then(|| line[start + 2..end - 2].to_string())
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn notes(paths: &[&str]) -> Vec<String> {
        paths.iter().map(|p| p.to_string()).collect()
    }

    #[test]
    fn parses_the_forms() {
        assert_eq!(parse("Ideas").label, "Ideas");
        assert_eq!(parse("Ideas").path, "Ideas");

        let aliased = parse("Ideas|my ideas");
        assert_eq!(aliased.path, "Ideas");
        assert_eq!(aliased.label, "my ideas");

        // The heading is display-only: it must not reach the lookup.
        let heading = parse("Ideas#Later");
        assert_eq!(heading.path, "Ideas");
        assert_eq!(heading.label, "Ideas#Later");

        assert_eq!(parse("projects/Ideas").path, "projects/Ideas");
    }

    #[test]
    fn an_explicit_path_wins_over_a_same_named_note() {
        let all = notes(&["Ideas.md", "projects/Ideas.md"]);
        assert_eq!(
            resolve("projects/Ideas", None, &all).as_deref(),
            Some("projects/Ideas.md")
        );
    }

    #[test]
    fn prefers_a_sibling_then_the_root_then_the_shallowest() {
        let all = notes(&["Ideas.md", "a/Ideas.md", "a/b/Ideas.md"]);
        // Beside the linking note.
        assert_eq!(
            resolve("Ideas", Some("a/note.md"), &all).as_deref(),
            Some("a/Ideas.md")
        );
        // From the root when there is no sibling.
        assert_eq!(
            resolve("Ideas", Some("z/note.md"), &all).as_deref(),
            Some("Ideas.md")
        );

        // With no root match, the shallowest wins over walk order.
        let deep = notes(&["a/b/c/Ideas.md", "a/Ideas.md"]);
        assert_eq!(
            resolve("Ideas", Some("z/note.md"), &deep).as_deref(),
            Some("a/Ideas.md")
        );
    }

    #[test]
    fn matching_is_case_insensitive_and_extension_optional() {
        let all = notes(&["Ideas.md"]);
        assert_eq!(resolve("ideas", None, &all).as_deref(), Some("Ideas.md"));
        assert_eq!(resolve("Ideas.md", None, &all).as_deref(), Some("Ideas.md"));
    }

    #[test]
    fn unresolved_links_are_none() {
        assert_eq!(resolve("Nothing", None, &notes(&["Ideas.md"])), None);
        assert_eq!(resolve("", None, &notes(&["Ideas.md"])), None);
    }

    #[test]
    fn finds_link_ranges_on_a_line() {
        let line = "see [[One]] and [[two|2]] here";
        let found = ranges(line);
        assert_eq!(found.len(), 2);
        assert_eq!(&line[found[0].0..found[0].1], "[[One]]");
        assert_eq!(&line[found[1].0..found[1].1], "[[two|2]]");
    }

    #[test]
    fn skips_links_inside_code_spans() {
        // A link in a code sample is a code sample, not a link.
        let line = "real [[One]] but `[[Two]]` is code";
        let found = ranges(line);
        assert_eq!(found.len(), 1);
        assert_eq!(&line[found[0].0..found[0].1], "[[One]]");
    }

    #[test]
    fn ignores_malformed_brackets() {
        assert!(ranges("[[unclosed").is_empty());
        assert!(ranges("[[]]").is_empty());
        assert!(ranges("[[a|b|c]]").is_empty());
        assert!(ranges("[[multi\nline]]").is_empty());
    }

    #[test]
    fn finds_the_link_under_a_cursor() {
        let line = "see [[One]] here";
        assert_eq!(link_at(line, 6).as_deref(), Some("One"));
        assert_eq!(link_at(line, 4).as_deref(), Some("One"));
        assert_eq!(link_at(line, 11).as_deref(), Some("One"));
        assert_eq!(link_at(line, 14), None);
        assert_eq!(link_at("no links", 3), None);
    }
}

// ── Autocomplete ──
//
// Typing `[[` offers the notes in the vault. The ranking and the shortest
// insertable form both live here because both are `resolve` seen from another
// angle: one finds candidates for what is being typed, the other asks which of
// a note's names still resolves back to it.

/// At most this many options are offered at once.
pub const MAX_COMPLETIONS: usize = 12;

/// An unclosed `[[` the cursor sits inside.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LinkContext {
    /// Where the `[[` starts, as a UTF-16 offset — what the editor indexes by.
    pub start: usize,
    /// What has been typed since the brackets.
    pub typed: String,
}

/// The link being typed at this cursor, if there is one.
///
/// `ch` is a UTF-16 offset, as every editor position is. Returns None once the
/// link is closed, so moving the cursor back through a finished link does not
/// reopen the list.
pub fn context_at(line: &str, ch: usize) -> Option<LinkContext> {
    let before = crate::utf16::truncate_at(line, ch);
    let open = before.rfind("[[")?;
    let typed = &before[open + 2..];

    // A `]]` means the link is already closed; a bare `[` means these are not
    // the brackets we think they are.
    if typed.contains("]]") || typed.contains('[') {
        return None;
    }

    Some(LinkContext {
        start: crate::utf16::offset_of(line, open),
        typed: typed.to_string(),
    })
}

/// The note's display name: its filename without the `.md`.
fn base_name(path: &str) -> &str {
    let file = path.rsplit('/').next().unwrap_or(path);
    file.strip_suffix(".md").unwrap_or(file)
}

/// Notes matching `query`, best first.
///
/// Matching falls back to the whole path, so "projects/id" finds
/// "projects/ideas.md", but ranking is on the filename, which is what people
/// are usually typing. (Only a contiguous substring matches — "proj/id" finds
/// nothing, despite what the comment in the JavaScript this replaces implies.)
/// Ties break on path length and then alphabetically, so the shortest and
/// nearest note wins rather than whichever the directory walk reached first.
pub fn complete(
    query: &str,
    notes: &[String],
    current: Option<&str>,
    max: usize,
) -> Vec<String> {
    let needle = query.trim().to_lowercase();

    let mut scored: Vec<(u8, &String)> = notes
        .iter()
        // Linking a note to itself is never what was meant.
        .filter(|path| Some(path.as_str()) != current)
        .filter_map(|path| {
            if needle.is_empty() {
                return Some((2, path));
            }

            let base = base_name(path).to_lowercase();
            if base.starts_with(&needle) {
                Some((0, path))
            } else if base.contains(&needle) {
                Some((1, path))
            } else if path.to_lowercase().contains(&needle) {
                Some((2, path))
            } else {
                None
            }
        })
        .collect();

    scored.sort_by(|(a_rank, a), (b_rank, b)| {
        a_rank
            .cmp(b_rank)
            .then_with(|| a.len().cmp(&b.len()))
            .then_with(|| a.cmp(b))
    });

    scored.into_iter().take(max).map(|(_, path)| path.clone()).collect()
}

/// The shortest text that still links back to `path`.
///
/// `resolve` run backwards: if the bare name already finds this note, the link
/// carries only the name, so links stay readable and gain a folder only when
/// they need one to be unambiguous.
pub fn shortest_form(path: &str, from_note: Option<&str>, notes: &[String]) -> String {
    let full = path.strip_suffix(".md").unwrap_or(path);
    let short = full.rsplit('/').next().unwrap_or(full);

    if resolve(short, from_note, notes).as_deref() == Some(path) {
        short.to_string()
    } else {
        full.to_string()
    }
}

#[cfg(test)]
mod completion_tests {
    use super::*;

    fn notes(paths: &[&str]) -> Vec<String> {
        paths.iter().map(|p| p.to_string()).collect()
    }

    #[test]
    fn finds_the_link_being_typed() {
        let found = context_at("see [[Ide", 9).unwrap();
        assert_eq!(found.typed, "Ide");
        assert_eq!(found.start, 4);
    }

    #[test]
    fn offers_everything_on_bare_brackets() {
        let found = context_at("see [[", 6).unwrap();
        assert_eq!(found.typed, "");
    }

    #[test]
    fn a_closed_link_does_not_reopen_the_list() {
        // The cursor has moved back through a finished link.
        assert_eq!(context_at("see [[Ideas]] here", 18), None);
    }

    #[test]
    fn a_stray_bracket_is_not_a_link() {
        assert_eq!(context_at("see [[Ide[a", 11), None);
        assert_eq!(context_at("no brackets", 5), None);
    }

    #[test]
    fn the_cursor_before_the_brackets_sees_nothing() {
        assert_eq!(context_at("see [[Ide", 3), None);
    }

    #[test]
    fn the_start_is_in_editor_units_not_bytes() {
        // The whole reason the helpers exist: an emoji before the brackets
        // makes the byte offset and the editor's offset disagree.
        let line = "a 🙂 [[Ide";
        let found = context_at(line, 10).unwrap();

        assert_eq!(found.typed, "Ide");
        assert_eq!(line.find("[["), Some(7));   // bytes
        assert_eq!(found.start, 5);             // what the editor means
    }

    #[test]
    fn ranks_name_prefix_above_name_substring_above_path() {
        let all = notes(&["ideas.md", "my-ideas.md", "ideas/other.md"]);
        let found = complete("ide", &all, None, 10);
        assert_eq!(found, ["ideas.md", "my-ideas.md", "ideas/other.md"]);
    }

    #[test]
    fn matches_on_the_path_but_ranks_on_the_name() {
        // "projects/id" appears in no filename, only in a full path.
        let all = notes(&["projects/ideas.md", "elsewhere.md"]);
        assert_eq!(complete("projects/id", &all, None, 10), ["projects/ideas.md"]);
    }

    #[test]
    fn path_matching_needs_a_contiguous_substring() {
        // Documents the real behaviour: the JavaScript's comment claims
        // "proj/id" works, but neither implementation can do that.
        let all = notes(&["projects/ideas.md"]);
        assert!(complete("proj/id", &all, None, 10).is_empty());
    }

    #[test]
    fn ties_break_on_length_then_alphabetically() {
        let all = notes(&["b/idea.md", "a/idea.md", "idea.md"]);
        let found = complete("idea", &all, None, 10);
        assert_eq!(found, ["idea.md", "a/idea.md", "b/idea.md"]);
    }

    #[test]
    fn the_open_note_is_never_offered() {
        let all = notes(&["ideas.md", "other.md"]);
        let found = complete("", &all, Some("ideas.md"), 10);
        assert_eq!(found, ["other.md"]);
    }

    #[test]
    fn an_empty_query_offers_everything_up_to_the_limit() {
        let all = notes(&["a.md", "b.md", "c.md"]);
        assert_eq!(complete("", &all, None, 2).len(), 2);
        assert_eq!(complete("  ", &all, None, 10).len(), 3);
    }

    #[test]
    fn matching_ignores_case() {
        let all = notes(&["Ideas.md"]);
        assert_eq!(complete("IDEAS", &all, None, 10), ["Ideas.md"]);
    }

    #[test]
    fn inserts_the_bare_name_when_it_already_resolves() {
        let all = notes(&["projects/Ideas.md"]);
        assert_eq!(shortest_form("projects/Ideas.md", None, &all), "Ideas");
    }

    #[test]
    fn qualifies_the_link_when_the_bare_name_is_ambiguous() {
        // Two notes share a name, and the bare form resolves to the other one,
        // so this link has to carry its folder.
        let all = notes(&["Ideas.md", "projects/Ideas.md"]);
        assert_eq!(
            shortest_form("projects/Ideas.md", None, &all),
            "projects/Ideas"
        );
        // The one the bare name does find keeps the short form.
        assert_eq!(shortest_form("Ideas.md", None, &all), "Ideas");
    }

    #[test]
    fn the_linking_note_decides_what_counts_as_ambiguous() {
        // From inside projects/, the sibling wins the bare name.
        let all = notes(&["Ideas.md", "projects/Ideas.md"]);
        assert_eq!(
            shortest_form("projects/Ideas.md", Some("projects/note.md"), &all),
            "Ideas"
        );
    }
}
