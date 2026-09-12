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
