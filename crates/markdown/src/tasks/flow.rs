//! `::flow::`: a note that is a saved graph (SYNTAX.md section 7).
//!
//! ```text
//! ::flow::
//! - [[rig/hero]] @ 0, 40
//! - [[anim/sh040]]
//! ::flow::
//! ```
//!
//! Each list item holding a link is a node, with an optional `@ x, y`. The
//! position is the only part the app ever writes, and only on its own line.

use super::{Warning, WarningCode};

pub(crate) const FLOW_MARKER: &str = "::flow::";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FlowNode {
    /// 1-based line of the node's list item.
    pub line: usize,
    /// Vault-relative note path without `.md`, as the link wrote it.
    pub path: String,
    /// Canvas position, if the line has a valid one.
    pub pos: Option<(i32, i32)>,
    /// Bytes after the link's `]]` up to the end of the line's text: what a
    /// new position replaces.
    pub(crate) tail: std::ops::Range<usize>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Flow {
    /// 1-based line of the opening `::flow::`.
    pub line: usize,
    pub nodes: Vec<FlowNode>,
}

/// Is this line a flow marker? It must start the line; trailing spaces are
/// forgiven.
pub(crate) fn is_marker(text: &str) -> bool {
    text.trim_end() == FLOW_MARKER
}

/// `x, y` as two whole numbers.
fn position(raw: &str) -> Option<(i32, i32)> {
    let (x, y) = raw.split_once(',')?;
    Some((x.trim().parse().ok()?, y.trim().parse().ok()?))
}

/// Read the lines between the markers. Each item is (line number, byte offset
/// of the line in the file, the line's text).
pub(crate) fn parse_block(
    open_line: usize,
    lines: &[(usize, usize, &str)],
    warnings: &mut Vec<Warning>,
) -> Flow {
    let mut nodes: Vec<FlowNode> = Vec::new();
    for &(line, start, text) in lines {
        let indent = text.len() - text.trim_start().len();
        let body = &text[indent..];
        let Some(item) = body
            .strip_prefix("- ")
            .or_else(|| body.strip_prefix("* "))
            .or_else(|| body.strip_prefix("+ "))
        else {
            continue;
        };
        let item_start = indent + 2;
        let Some(inner) = item.strip_prefix("[[") else { continue };
        let Some(close) = inner.find("]]") else { continue };

        let target = inner[..close].split('|').next().unwrap_or("");
        let path = target.split('#').next().unwrap_or("").trim();
        let path = path.strip_suffix(".md").unwrap_or(path).to_string();
        if path.is_empty() {
            continue;
        }

        let tail_start = item_start + 2 + close + 2;
        let tail = text[tail_start..].trim();
        let pos = match tail.strip_prefix('@') {
            Some(raw) => {
                let parsed = position(raw);
                if parsed.is_none() {
                    warnings.push(Warning::new(
                        line,
                        WarningCode::PositionBad,
                        format!(
                            "\"{}\" isn't a position like @ 320, 40. The node is placed automatically.",
                            tail
                        ),
                    ));
                }
                parsed
            }
            None => None,
        };

        if nodes.iter().any(|n| n.path == path) {
            warnings.push(Warning::new(
                line,
                WarningCode::NodeDuplicate,
                format!("[[{path}]] is already in this flow. This line is ignored."),
            ));
            continue;
        }

        nodes.push(FlowNode {
            line,
            path,
            pos,
            tail: start + tail_start..start + text.len(),
        });
    }
    Flow { line: open_line, nodes }
}
