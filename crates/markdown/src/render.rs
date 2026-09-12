//! Rendering a note to the HTML the preview panel shows.
//!
//! Three things happen here that plain CommonMark rendering does not do:
//!
//! - Every top-level block, and every list item, carries the source line it
//!   came from, so clicking it can open an editor on exactly those bytes.
//! - Relative image paths are rewritten onto `/api/asset/`, because the browser
//!   would otherwise resolve them against the page URL.
//! - `[[wiki links]]` become anchors the app intercepts, and task checkboxes
//!   become live inputs that write back to the Markdown.
//!
//! The position tracking is the part worth calling out. The JavaScript this
//! replaces re-derived positions by running the lexer and then *searching* the
//! source for each token's raw text with a moving cursor. `into_offset_iter`
//! hands back the exact byte range of every event, so that search — and the
//! drift it could produce on repeated text — is gone.

use crate::assets;
use crate::source::{self, options};
use crate::wikilinks;
use pulldown_cmark::{Alignment, CodeBlockKind, Event, HeadingLevel, Parser, Tag, TagEnd};

/// What a note needs to render: which note it is (for relative image paths) and
/// which notes exist (for resolving wiki links).
pub struct RenderContext<'a> {
    pub note_path: Option<&'a str>,
    pub notes: &'a [String],
}

impl RenderContext<'_> {
    pub fn empty() -> RenderContext<'static> {
        RenderContext { note_path: None, notes: &[] }
    }
}

/// Render `markdown` to annotated HTML.
pub fn render(markdown: &str, context: &RenderContext) -> String {
    let mut out = String::with_capacity(markdown.len() * 2);
    let mut depth = 0usize;
    let mut block_index = 0usize;
    let mut task_index = 0usize;

    // Set while inside an image: its alt text is the events between Start and
    // End, which we collect rather than render, since an <img> has no children.
    let mut image: Option<(String, String)> = None;

    // Tracked here rather than read back off the pending events: a code block's
    // opening tag is one we write ourselves, so it never reaches that buffer.
    let mut in_code = false;

    // Table state. The library's writer takes <th> vs <td> and each cell's
    // alignment from the events it sees, so once we intercept the table at all
    // we have to see it through ourselves.
    let mut table = TableState::default();

    // Wiki links are located once over the whole source rather than by
    // scanning text events: pulldown-cmark splits "[[Ideas]]" into one event
    // per bracket, so no single event ever contains the syntax. Matching by
    // source range sidesteps that entirely — and because a link may not span a
    // newline, one document-wide scan is safe.
    let links = wikilinks::all_ranges(markdown);

    let parser = Parser::new_ext(markdown, options()).into_offset_iter();
    let mut pending: Vec<Event> = Vec::new();

    for (event, range) in parser {
        // Inside an image, everything up to its End is alt text.
        if let Some((_, alt)) = image.as_mut() {
            match event {
                Event::End(TagEnd::Image) => {
                    let (src, alt) = image.take().expect("inside an image");
                    depth = depth.saturating_sub(1);
                    // Whatever came before the image is still buffered; without
                    // this the image is written ahead of its own paragraph.
                    flush(&mut pending, &mut out);
                    out.push_str(&render_image(&src, &alt, context.note_path));
                    continue;
                }
                Event::Text(text) | Event::Code(text) => {
                    alt.push_str(&text);
                    continue;
                }
                _ => continue,
            }
        }

        match &event {
            Event::Start(tag) => {
                let line = source::line_index_at(markdown, range.start);
                // Only top-level blocks and list items are annotated: those are
                // the units the preview lets you click into.
                let annotate = depth == 0 || matches!(tag, Tag::Item);

                if let Tag::Image { dest_url, .. } = tag {
                    image = Some((dest_url.to_string(), String::new()));
                    depth += 1;
                    continue;
                }

                if matches!(tag, Tag::CodeBlock(_)) {
                    in_code = true;
                }
                table.note_start(tag);
                let block = matches!(tag, Tag::CodeBlock(_)) && depth == 0;
                match open_tag(
                    tag,
                    annotate.then_some(line),
                    block.then_some(block_index),
                    &table,
                ) {
                    Some(html) => {
                        flush(&mut pending, &mut out);
                        out.push_str(&html);
                    }
                    // A tag we don't annotate by hand (tables, footnotes) is
                    // left to the library's own writer.
                    None => pending.push(event.clone()),
                }

                if depth == 0 {
                    block_index += 1;
                }
                depth += 1;
            }

            Event::End(tag_end) => {
                depth = depth.saturating_sub(1);
                if matches!(tag_end, TagEnd::CodeBlock) {
                    in_code = false;
                }
                if matches!(tag_end, TagEnd::Image) {
                    continue;
                }
                match close_tag(tag_end, &table) {
                    Some(html) => {
                        flush(&mut pending, &mut out);
                        out.push_str(html);
                    }
                    None => pending.push(event.clone()),
                }
                table.note_end(tag_end);
            }

            Event::TaskListMarker(checked) => {
                flush(&mut pending, &mut out);
                out.push_str(&render_task(*checked, task_index));
                task_index += 1;
            }

            // A text run that overlaps a wiki link is rewritten; everything
            // else is left to the library. Inline code arrives as Event::Code
            // and fenced content as text inside a CodeBlock, so a link in a
            // code sample is never offered here — the exclusion the JavaScript
            // did by scanning for backticks falls out of the parse instead.
            Event::Text(text) if !in_code && overlaps(&links, &range) => {
                flush(&mut pending, &mut out);
                out.push_str(&render_text_with_links(
                    text, &range, markdown, &links, context,
                ));
            }

            _ => pending.push(event.clone()),
        }
    }

    flush(&mut pending, &mut out);
    out
}

/// Hand any events we didn't write ourselves to the library's HTML writer.
fn flush(pending: &mut Vec<Event>, out: &mut String) {
    if pending.is_empty() {
        return;
    }
    pulldown_cmark::html::push_html(out, pending.drain(..));
}

/// Where we are inside a table, which decides each cell's tag and alignment.
#[derive(Default)]
struct TableState {
    alignments: Vec<Alignment>,
    column: usize,
    in_head: bool,
    inside: bool,
}

impl TableState {
    fn note_start(&mut self, tag: &Tag) {
        match tag {
            Tag::Table(alignments) => {
                self.alignments = alignments.clone();
                self.inside = true;
            }
            Tag::TableHead => {
                self.in_head = true;
                self.column = 0;
            }
            Tag::TableRow => self.column = 0,
            _ => {}
        }
    }

    fn note_end(&mut self, tag: &TagEnd) {
        match tag {
            TagEnd::Table => {
                self.inside = false;
                self.alignments.clear();
            }
            TagEnd::TableHead => self.in_head = false,
            TagEnd::TableCell => self.column += 1,
            _ => {}
        }
    }

    /// The opening tag for the cell now starting.
    fn cell(&self) -> String {
        let name = if self.in_head { "th" } else { "td" };
        match self.alignments.get(self.column) {
            Some(Alignment::Left) => format!(r#"<{name} style="text-align: left">"#),
            Some(Alignment::Center) => format!(r#"<{name} style="text-align: center">"#),
            Some(Alignment::Right) => format!(r#"<{name} style="text-align: right">"#),
            _ => format!("<{name}>"),
        }
    }

    fn cell_close(&self) -> &'static str {
        if self.in_head {
            "</th>"
        } else {
            "</td>"
        }
    }
}

/// The opening tag for the blocks we annotate, or None to let the library
/// render this one.
fn open_tag(
    tag: &Tag,
    line: Option<usize>,
    block: Option<usize>,
    table: &TableState,
) -> Option<String> {
    let mut attrs = String::new();
    if let Some(line) = line {
        attrs.push_str(&format!(r#" data-md-line-index="{line}""#));
    }
    if let Some(block) = block {
        attrs.push_str(&format!(r#" data-md-block-index="{block}""#));
    }

    let html = match tag {
        Tag::Paragraph => format!("<p{attrs}>"),
        Tag::Heading { level, .. } => format!("<{}{attrs}>", heading_tag(*level)),
        Tag::BlockQuote(_) => format!("<blockquote{attrs}>"),
        Tag::List(Some(start)) if *start == 1 => format!("<ol{attrs}>"),
        Tag::List(Some(start)) => format!(r#"<ol{attrs} start="{start}">"#),
        Tag::List(None) => format!("<ul{attrs}>"),
        Tag::Item => format!("<li{attrs}>"),
        Tag::Table(_) => format!("<table{attrs}>"),
        Tag::TableHead => "<thead><tr>".to_string(),
        Tag::TableRow => "<tr>".to_string(),
        Tag::TableCell => table.cell(),
        Tag::CodeBlock(kind) => {
            let class = match kind {
                CodeBlockKind::Fenced(info) => info
                    .split_whitespace()
                    .next()
                    .filter(|lang| !lang.is_empty())
                    .map(|lang| format!(r#" class="language-{}""#, escape_attr(lang)))
                    .unwrap_or_default(),
                CodeBlockKind::Indented => String::new(),
            };
            format!("<pre{attrs}><code{class}>")
        }
        _ => return None,
    };
    Some(html)
}

fn close_tag(tag: &TagEnd, table: &TableState) -> Option<&'static str> {
    Some(match tag {
        TagEnd::Paragraph => "</p>\n",
        TagEnd::Heading(level) => match level {
            HeadingLevel::H1 => "</h1>\n",
            HeadingLevel::H2 => "</h2>\n",
            HeadingLevel::H3 => "</h3>\n",
            HeadingLevel::H4 => "</h4>\n",
            HeadingLevel::H5 => "</h5>\n",
            HeadingLevel::H6 => "</h6>\n",
        },
        TagEnd::BlockQuote(_) => "</blockquote>\n",
        TagEnd::List(true) => "</ol>\n",
        TagEnd::List(false) => "</ul>\n",
        TagEnd::Item => "</li>\n",
        TagEnd::CodeBlock => "</code></pre>\n",
        TagEnd::Table => "</tbody></table>\n",
        TagEnd::TableHead => "</tr></thead><tbody>\n",
        TagEnd::TableRow => "</tr>\n",
        TagEnd::TableCell => table.cell_close(),
        _ => return None,
    })
}

fn heading_tag(level: HeadingLevel) -> &'static str {
    match level {
        HeadingLevel::H1 => "h1",
        HeadingLevel::H2 => "h2",
        HeadingLevel::H3 => "h3",
        HeadingLevel::H4 => "h4",
        HeadingLevel::H5 => "h5",
        HeadingLevel::H6 => "h6",
    }
}

/// An image, with its path rewritten onto /api/asset when it is a vault path.
///
/// The original src becomes the title, so hovering says what the note actually
/// wrote rather than the rewritten URL.
fn render_image(src: &str, alt: &str, note_path: Option<&str>) -> String {
    format!(
        r#"<img src="{}" alt="{}" loading="lazy" title="{}">"#,
        escape_attr(&assets::url(src, note_path)),
        escape_attr(alt),
        escape_attr(src),
    )
}

/// A task checkbox the reader can actually click.
///
/// CommonMark renders these disabled, because HTML alone cannot persist a click
/// back to Markdown. The index is what lets a click find its marker in the
/// source, counted in the same source order `set_task_checked` walks.
fn render_task(checked: bool, index: usize) -> String {
    let (state, label) = if checked {
        (" checked", "Mark task incomplete")
    } else {
        ("", "Mark task complete")
    };
    // The trailing space is part of the rendering, not decoration: the parser
    // consumes it with the marker, and without it the label runs into the box.
    format!(
        r#"<input type="checkbox" class="interactive-task-checkbox" data-task-index="{index}"{state} aria-label="{label}"> "#
    )
}

/// Whether a source range touches any wiki link.
fn overlaps(links: &[(usize, usize)], range: &std::ops::Range<usize>) -> bool {
    links
        .iter()
        .any(|(start, end)| *start < range.end && range.start < *end)
}

/// Render one text run that touches at least one wiki link.
///
/// The events covering a link are emitted as nothing until the one that starts
/// it, which emits the whole anchor — so the brackets pulldown-cmark hands back
/// separately never reach the output.
fn render_text_with_links(
    text: &str,
    range: &std::ops::Range<usize>,
    markdown: &str,
    links: &[(usize, usize)],
    context: &RenderContext,
) -> String {
    // Offsets into the event's text are only comparable with source offsets
    // when the event is a verbatim slice. Entity-decoded text is not, so fall
    // back to plain escaping rather than slicing at the wrong place.
    if text.len() != range.end - range.start {
        return escape_html(text);
    }

    let mut out = String::new();
    let mut cursor = range.start;

    for (start, end) in links.iter().copied() {
        if end <= cursor || start >= range.end {
            continue;
        }
        // Text before the link, if this run holds any of it.
        if start > cursor {
            out.push_str(&escape_html(&markdown[cursor..start.min(range.end)]));
        }
        // The link itself is emitted once, by the run that opens it.
        if start >= range.start {
            let link = wikilinks::parse(&markdown[start + 2..end - 2]);
            out.push_str(&render_wikilink(&link, context));
        }
        cursor = end.max(cursor);
    }

    if cursor < range.end {
        out.push_str(&escape_html(&markdown[cursor..range.end]));
    }
    out
}

/// One wiki link.
///
/// The target goes in an attribute rather than an href: these open a note
/// inside the app, and a real href would navigate the page away.
fn render_wikilink(link: &wikilinks::WikiLink, context: &RenderContext) -> String {
    let target = wikilinks::resolve(&link.path, context.note_path, context.notes);
    let (class, title) = match &target {
        Some(path) => ("wikilink", path.clone()),
        None => ("wikilink is-missing", format!("{} — not created yet", link.path)),
    };

    format!(
        r#"<a class="{class}" data-wikilink="{}" title="{}">{}</a>"#,
        escape_attr(&link.path),
        escape_attr(&title),
        escape_html(&link.label),
    )
}

fn escape_html(text: &str) -> String {
    text.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;")
}

fn escape_attr(text: &str) -> String {
    escape_html(text).replace('"', "&quot;")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn render_plain(markdown: &str) -> String {
        render(markdown, &RenderContext::empty())
    }

    #[test]
    fn annotates_top_level_blocks_with_their_source_line() {
        let html = render_plain("# Title\n\nA paragraph.\n\nAnother.\n");
        assert!(html.contains(r#"<h1 data-md-line-index="0">"#), "{html}");
        assert!(html.contains(r#"<p data-md-line-index="2">"#), "{html}");
        assert!(html.contains(r#"<p data-md-line-index="4">"#), "{html}");
    }

    #[test]
    fn annotates_list_items_individually() {
        // Each <li> must edit only its own line, not the whole list.
        let html = render_plain("- one\n- two\n- three\n");
        assert!(html.contains(r#"<li data-md-line-index="0">"#), "{html}");
        assert!(html.contains(r#"<li data-md-line-index="1">"#), "{html}");
        assert!(html.contains(r#"<li data-md-line-index="2">"#), "{html}");
    }

    #[test]
    fn annotates_nested_list_items() {
        let html = render_plain("- one\n  - nested\n- two\n");
        assert!(html.contains(r#"<li data-md-line-index="1">"#), "{html}");
    }

    #[test]
    fn a_code_block_carries_a_block_index_that_matches_block_ranges() {
        let md = "# Title\n\nText.\n\n```rust\nfn main() {}\n```\n";
        let html = render_plain(md);
        assert!(html.contains(r#"data-md-block-index="2""#), "{html}");
        assert!(html.contains(r#"<code class="language-rust">"#), "{html}");

        // The index must address the same block the editor will write back to.
        let block = source::block_range(md, 2).unwrap();
        assert!(block.raw.contains("fn main()"));
    }

    #[test]
    fn task_markers_become_clickable_inputs_in_source_order() {
        let html = render_plain("- [ ] one\n- [x] two\n");
        assert!(html.contains(r#"data-task-index="0""#), "{html}");
        assert!(html.contains(r#"data-task-index="1""#), "{html}");
        assert!(html.contains("interactive-task-checkbox"), "{html}");
        // Rendered live, not disabled: a click has somewhere to go.
        assert!(!html.contains("disabled"), "{html}");
        assert!(html.contains(" checked"), "{html}");
    }

    #[test]
    fn relative_images_are_rewritten_and_external_ones_are_not() {
        let context = RenderContext { note_path: Some("projects/note.md"), notes: &[] };
        let html = render("![cat](cat.png)", &context);
        assert!(html.contains(r#"src="/api/asset/projects/cat.png""#), "{html}");
        assert!(html.contains(r#"alt="cat""#), "{html}");
        assert!(html.contains(r#"loading="lazy""#), "{html}");
        // The title says what the note wrote, not where it was rewritten to.
        assert!(html.contains(r#"title="cat.png""#), "{html}");

        let external = render("![x](https://example.com/a.png)", &context);
        assert!(external.contains(r#"src="https://example.com/a.png""#), "{external}");
    }

    #[test]
    fn wikilinks_resolve_against_the_note_list() {
        let notes = vec!["Ideas.md".to_string()];
        let context = RenderContext { note_path: None, notes: &notes };

        let hit = render("see [[Ideas]]", &context);
        assert!(hit.contains(r#"class="wikilink""#), "{hit}");
        assert!(hit.contains(r#"data-wikilink="Ideas""#), "{hit}");
        assert!(hit.contains(r#"title="Ideas.md""#), "{hit}");
        // An attribute, not an href: following it must not navigate away.
        assert!(!hit.contains("href="), "{hit}");

        let miss = render("see [[Nothing]]", &context);
        assert!(miss.contains("is-missing"), "{miss}");
        assert!(miss.contains("not created yet"), "{miss}");
    }

    #[test]
    fn an_aliased_wikilink_shows_the_alias() {
        let notes = vec!["Ideas.md".to_string()];
        let context = RenderContext { note_path: None, notes: &notes };
        let html = render("[[Ideas|my ideas]]", &context);
        assert!(html.contains(">my ideas</a>"), "{html}");
        assert!(html.contains(r#"data-wikilink="Ideas""#), "{html}");
    }

    #[test]
    fn wikilinks_inside_code_are_left_alone() {
        // This is the case the JavaScript needed a hand-rolled backtick scanner
        // for; here it falls out of the parse.
        let notes = vec!["Ideas.md".to_string()];
        let context = RenderContext { note_path: None, notes: &notes };

        let span = render("use `[[Ideas]]` to link", &context);
        assert!(!span.contains("wikilink"), "{span}");
        assert!(span.contains("[[Ideas]]"), "{span}");

        let fenced = render("```\n[[Ideas]]\n```\n", &context);
        assert!(!fenced.contains("wikilink"), "{fenced}");
    }

    #[test]
    fn escapes_html_in_text_and_attributes() {
        let html = render_plain("a < b & c");
        assert!(html.contains("&lt;"), "{html}");
        assert!(html.contains("&amp;"), "{html}");

        let notes: Vec<String> = Vec::new();
        let context = RenderContext { note_path: None, notes: &notes };
        let quoted = render(r#"[[a"b]]"#, &context);
        assert!(!quoted.contains(r#"data-wikilink="a"b""#), "{quoted}");
        assert!(quoted.contains("&quot;"), "{quoted}");
    }

    #[test]
    fn renders_ordinary_markdown_faithfully() {
        let html = render_plain("**bold** and *italic* and `code`\n");
        assert!(html.contains("<strong>bold</strong>"), "{html}");
        assert!(html.contains("<em>italic</em>"), "{html}");
        assert!(html.contains("<code>code</code>"), "{html}");

        let link = render_plain("[text](https://example.com)");
        assert!(link.contains(r#"<a href="https://example.com">text</a>"#), "{link}");
    }

    #[test]
    fn handles_an_empty_document() {
        assert_eq!(render_plain(""), "");
    }

    #[test]
    fn an_image_stays_in_its_place_in_the_text() {
        // Regression: the text before an image was still buffered when the
        // image was written, so every image jumped to the front of its
        // paragraph.
        let html = render_plain("before ![a](x.png) after");
        let img = html.find("<img").expect("an image");
        let before = html.find("before").expect("leading text");
        let after = html.find("after").expect("trailing text");
        assert!(before < img && img < after, "{html}");
    }

    #[test]
    fn blocks_after_an_image_are_still_annotated() {
        // Regression: an image incremented the nesting depth and never
        // decremented it, so every later block looked nested and lost its
        // line index — silently disabling click-to-edit for the rest of a note.
        let html = render_plain("![a](x.png)\n\nafter one\n\nafter two\n");
        assert!(html.contains(r#"<p data-md-line-index="2">"#), "{html}");
        assert!(html.contains(r#"<p data-md-line-index="4">"#), "{html}");
    }

    #[test]
    fn several_images_do_not_shift_later_blocks() {
        let html = render_plain("![a](1.png) ![b](2.png) ![c](3.png)\n\ntail\n");
        assert_eq!(html.matches("<img").count(), 3, "{html}");
        assert!(html.contains(r#"<p data-md-line-index="2">"#), "{html}");
    }

    #[test]
    fn a_task_checkbox_is_separated_from_its_label() {
        // Without the space the label runs into the box.
        let html = render_plain("- [ ] a task");
        assert!(html.contains("> a task"), "{html}");
    }

    #[test]
    fn tables_render_with_header_body_and_alignment() {
        let md = "| a | b | c |\n|:--|:-:|--:|\n| 1 | 2 | 3 |\n";
        let html = render_plain(md);

        // Header cells are th, body cells are td: half-intercepting the table
        // used to make every body cell a header.
        assert!(html.contains("<th"), "{html}");
        assert!(html.contains("<td"), "{html}");
        // Counted on the closing tags: "<th" also matches "<thead".
        assert_eq!(html.matches("</th>").count(), 3, "{html}");
        assert_eq!(html.matches("</td>").count(), 3, "{html}");

        // Alignment survives being rendered by hand.
        assert!(html.contains("text-align: left"), "{html}");
        assert!(html.contains("text-align: center"), "{html}");
        assert!(html.contains("text-align: right"), "{html}");

        // And the table can be clicked into, like any other block.
        assert!(html.contains(r#"<table data-md-line-index="0">"#), "{html}");
    }
}
