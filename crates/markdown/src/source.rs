//! Locating and editing pieces of the Markdown source.
//!
//! The preview lets you click a rendered line to edit the Markdown behind it.
//! Which bytes to replace is worked out here, against the *current* content
//! rather than what was rendered — the note can have changed in between, and
//! writing back a stale range would corrupt it.

use pulldown_cmark::{Event, Options, Parser};
use std::ops::Range;

/// A located piece of the source and the text it holds.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SourceRange {
    pub start: usize,
    pub end: usize,
    pub raw: String,
}

/// The byte range of one line, excluding its newline (and any \r before it).
pub fn line_range(markdown: &str, wanted_line: usize) -> Option<SourceRange> {
    let mut start = 0;
    for _ in 0..wanted_line {
        start = markdown[start..].find('\n')? + start + 1;
    }

    let end = markdown[start..]
        .find('\n')
        .map(|offset| start + offset)
        .unwrap_or(markdown.len());

    // A CRLF file would otherwise hand back a trailing \r to edit.
    let content_end = if end > start && markdown.as_bytes()[end - 1] == b'\r' {
        end - 1
    } else {
        end
    };

    Some(SourceRange {
        start,
        end: content_end,
        raw: markdown[start..content_end].to_string(),
    })
}

/// The zero-based line a byte offset falls on.
pub fn line_index_at(markdown: &str, offset: usize) -> usize {
    markdown[..offset.min(markdown.len())]
        .bytes()
        .filter(|byte| *byte == b'\n')
        .count()
}

/// Parser options, in one place so the renderer and these helpers always agree
/// on what the document means.
pub fn options() -> Options {
    Options::ENABLE_TABLES
        | Options::ENABLE_FOOTNOTES
        | Options::ENABLE_STRIKETHROUGH
        | Options::ENABLE_TASKLISTS
}

/// The byte ranges of every top-level block, in source order.
///
/// This is the list the renderer indexes into when it stamps a code block with
/// `data-md-block-index`, so both sides agree on what "block 3" means by
/// construction rather than by coincidence.
pub fn block_ranges(markdown: &str) -> Vec<Range<usize>> {
    let mut depth = 0usize;
    let mut blocks = Vec::new();

    for (event, range) in Parser::new_ext(markdown, options()).into_offset_iter() {
        match event {
            Event::Start(_) => {
                if depth == 0 {
                    blocks.push(range);
                }
                depth += 1;
            }
            Event::End(_) => depth = depth.saturating_sub(1),
            _ => {}
        }
    }
    blocks
}

/// The source of one top-level block, by its index in `block_ranges`.
pub fn block_range(markdown: &str, wanted: usize) -> Option<SourceRange> {
    let range = block_ranges(markdown).into_iter().nth(wanted)?;
    Some(SourceRange {
        start: range.start,
        end: range.end,
        raw: markdown[range.start..range.end].to_string(),
    })
}

/// Change the Nth Markdown task marker, ignoring fenced code blocks.
///
/// Blockquoted, nested, unordered and ordered task-list items are all
/// supported. Returns None when there is no such task, which means the preview
/// and the editor have fallen out of step and the click should be undone
/// rather than written somewhere arbitrary.
pub fn set_task_checked(markdown: &str, wanted: usize, checked: bool) -> Option<String> {
    let mut lines: Vec<String> = markdown.split('\n').map(str::to_string).collect();
    let mut fence: Option<char> = None;
    let mut index = 0;

    for line in lines.iter_mut() {
        if let Some(marker) = fence_marker(line) {
            match fence {
                None => fence = Some(marker),
                // Only the same character closes a fence, so ``` inside a ~~~
                // block stays content.
                Some(open) if open == marker => fence = None,
                Some(_) => {}
            }
            continue;
        }
        if fence.is_some() {
            continue;
        }

        let Some(marker_at) = task_marker_offset(line) else { continue };
        if index == wanted {
            line.replace_range(marker_at..marker_at + 1, if checked { "x" } else { " " });
            return Some(lines.join("\n"));
        }
        index += 1;
    }

    None
}

/// The fence character if this line opens or closes a fenced block.
fn fence_marker(line: &str) -> Option<char> {
    let rest = strip_quote_and_space(line);
    ['`', '~']
        .into_iter()
        .find(|marker| rest.starts_with(&marker.to_string().repeat(3)))
}

/// Byte offset of the ' ' or 'x' inside a `- [ ]` marker on this line.
fn task_marker_offset(line: &str) -> Option<usize> {
    let rest = strip_quote_and_space(line);
    let consumed = line.len() - rest.len();

    // A bullet ('-', '+', '*') or an ordered marker ('1.', '1)').
    let after_bullet = match rest.chars().next()? {
        '-' | '+' | '*' => &rest[1..],
        '0'..='9' => {
            let digits = rest.find(|c: char| !c.is_ascii_digit())?;
            let after = &rest[digits..];
            if after.starts_with('.') || after.starts_with(')') {
                &after[1..]
            } else {
                return None;
            }
        }
        _ => return None,
    };

    // At least one space, then "[x]" or "[ ]".
    let trimmed = after_bullet.trim_start_matches([' ', '\t']);
    if trimmed.len() == after_bullet.len() {
        return None;
    }
    let state = trimmed.strip_prefix('[')?;
    let marker = state.chars().next()?;
    if !matches!(marker, ' ' | 'x' | 'X') || !state[marker.len_utf8()..].starts_with(']') {
        return None;
    }

    Some(consumed + (rest.len() - state.len()))
}

/// Drop leading whitespace and any `>` blockquote markers.
fn strip_quote_and_space(line: &str) -> &str {
    let mut rest = line.trim_start_matches([' ', '\t']);
    while let Some(stripped) = rest.strip_prefix('>') {
        rest = stripped.trim_start_matches([' ', '\t']);
    }
    rest
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_line_ranges() {
        let md = "one\ntwo\nthree";
        assert_eq!(line_range(md, 0).unwrap().raw, "one");
        assert_eq!(line_range(md, 1).unwrap().raw, "two");
        assert_eq!(line_range(md, 2).unwrap().raw, "three");
        assert_eq!(line_range(md, 3), None);
    }

    #[test]
    fn a_line_range_excludes_the_carriage_return() {
        let md = "one\r\ntwo";
        let first = line_range(md, 0).unwrap();
        assert_eq!(first.raw, "one");
        assert_eq!(&md[first.start..first.end], "one");
    }

    #[test]
    fn maps_offsets_back_to_lines() {
        let md = "one\ntwo\nthree";
        assert_eq!(line_index_at(md, 0), 0);
        assert_eq!(line_index_at(md, 4), 1);
        assert_eq!(line_index_at(md, 8), 2);
    }

    #[test]
    fn finds_top_level_blocks() {
        let md = "# Title\n\nA paragraph.\n\n```rust\nfn main() {}\n```\n";
        let blocks = block_ranges(md);
        assert_eq!(blocks.len(), 3);
        assert!(md[blocks[0].clone()].starts_with("# Title"));
        assert!(md[blocks[1].clone()].starts_with("A paragraph."));
        assert!(md[blocks[2].clone()].starts_with("```rust"));

        // The code block round-trips, which is what the block editor writes back.
        let code = block_range(md, 2).unwrap();
        assert_eq!(code.raw.trim_end(), "```rust\nfn main() {}\n```");
    }

    #[test]
    fn toggles_the_right_task() {
        let md = "- [ ] one\n- [ ] two\n- [ ] three";
        assert_eq!(
            set_task_checked(md, 1, true).unwrap(),
            "- [ ] one\n- [x] two\n- [ ] three"
        );
        assert_eq!(
            set_task_checked("- [x] one", 0, false).unwrap(),
            "- [ ] one"
        );
    }

    #[test]
    fn toggles_nested_quoted_and_ordered_tasks() {
        let md = "  - [ ] nested\n> - [ ] quoted\n1. [ ] ordered\n2) [ ] paren";
        for index in 0..4 {
            let out = set_task_checked(md, index, true).unwrap();
            assert_eq!(out.matches("[x]").count(), 1, "index {index}");
        }
    }

    #[test]
    fn ignores_tasks_inside_fenced_code() {
        // The checkbox in the fence is a code sample; the real one is after it.
        let md = "```\n- [ ] not a task\n```\n- [ ] real";
        let out = set_task_checked(md, 0, true).unwrap();
        assert_eq!(out, "```\n- [ ] not a task\n```\n- [x] real");
    }

    #[test]
    fn a_tilde_fence_is_not_closed_by_backticks() {
        let md = "~~~\n```\n- [ ] inside\n~~~\n- [ ] real";
        let out = set_task_checked(md, 0, true).unwrap();
        assert!(out.ends_with("- [x] real"));
    }

    #[test]
    fn a_missing_task_is_none() {
        assert_eq!(set_task_checked("- [ ] only", 5, true), None);
        assert_eq!(set_task_checked("no tasks here", 0, true), None);
    }
}
