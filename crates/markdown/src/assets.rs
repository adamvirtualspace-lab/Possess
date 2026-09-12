//! Turning an image path written in a note into a URL the browser can fetch.
//!
//! A note saying `![](images/plan.png)` means "images/ next to this note", but
//! a browser resolves that against the page URL instead, so every relative
//! image 404s. Both the rendered preview and the editor's inline thumbnails
//! route through here to rewrite such paths onto `/api/asset/`.

/// Whether `src` is already a URL rather than a vault path.
///
/// Anything with a scheme, or a protocol-relative `//host/…`, is left alone —
/// rewriting those would break them.
pub fn is_external(src: &str) -> bool {
    if src.starts_with("//") {
        return true;
    }

    // A scheme is a letter followed by letters, digits, '+', '-' or '.', then
    // a colon. Checked by hand so a Windows path never reads as one.
    let mut chars = src.chars();
    match chars.next() {
        Some(first) if first.is_ascii_alphabetic() => {}
        _ => return false,
    }
    for ch in chars {
        match ch {
            ':' => return true,
            'a'..='z' | 'A'..='Z' | '0'..='9' | '+' | '-' | '.' => {}
            _ => return false,
        }
    }
    false
}

/// Resolve `src` against the folder holding `note_path`, vault-relative.
///
/// A leading slash means "from the vault root". Returns None if the path climbs
/// out of the vault, in which case the caller leaves it untouched and the
/// server would refuse it anyway.
pub fn resolve(src: &str, note_path: Option<&str>) -> Option<String> {
    let raw = src.trim();
    let raw = raw.split('#').next().unwrap_or(raw);
    let raw = raw.split('?').next().unwrap_or(raw);
    let raw = percent_decode(raw);

    let from_root = raw.starts_with('/');
    let base: Vec<&str> = match note_path {
        Some(note) if !from_root && note.contains('/') => {
            note[..note.rfind('/').unwrap()].split('/').collect()
        }
        _ => Vec::new(),
    };

    let mut parts: Vec<String> = base.into_iter().map(str::to_string).collect();
    for segment in raw.split('/') {
        match segment {
            "" | "." => continue,
            ".." => {
                // Climbing past the vault root is not a path we can serve.
                parts.pop()?;
            }
            other => parts.push(other.to_string()),
        }
    }

    (!parts.is_empty()).then(|| parts.join("/"))
}

/// The URL that serves `src` for a note at `note_path`.
pub fn url(src: &str, note_path: Option<&str>) -> String {
    if src.is_empty() || is_external(src) {
        return src.to_string();
    }

    match resolve(src, note_path) {
        // Each segment is encoded separately so slashes stay slashes while
        // spaces and '#' in real filenames survive the trip.
        Some(path) => {
            let encoded: Vec<String> = path.split('/').map(encode_segment).collect();
            format!("/api/asset/{}", encoded.join("/"))
        }
        None => src.to_string(),
    }
}

/// Percent-encode one path segment. Invalid encoding is left as written, which
/// is what a bare '%' in a filename needs.
fn encode_segment(segment: &str) -> String {
    segment
        .bytes()
        .map(|byte| match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                (byte as char).to_string()
            }
            _ => format!("%{byte:02X}"),
        })
        .collect()
}

/// Decode percent-escapes, returning the input unchanged if it isn't valid
/// encoding — a bare '%' in a filename is not an error here.
fn percent_decode(raw: &str) -> String {
    if !raw.contains('%') {
        return raw.to_string();
    }

    let bytes = raw.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut index = 0;

    while index < bytes.len() {
        if bytes[index] == b'%' && index + 2 < bytes.len() {
            let hex = std::str::from_utf8(&bytes[index + 1..index + 3]).ok();
            match hex.and_then(|h| u8::from_str_radix(h, 16).ok()) {
                Some(byte) => {
                    out.push(byte);
                    index += 3;
                    continue;
                }
                None => return raw.to_string(),
            }
        }
        out.push(bytes[index]);
        index += 1;
    }

    String::from_utf8(out).unwrap_or_else(|_| raw.to_string())
}

/// One markdown image: `![alt](src "title")`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InlineImage {
    pub alt: String,
    pub src: String,
}

/// The first markdown image on a line.
///
/// Bare-URL images (`<img>` tags, reference links) are out of scope: this feeds
/// the editor's inline thumbnails, where the point is to preview what the
/// writer just typed.
pub fn parse_image(line: &str) -> Option<InlineImage> {
    let start = line.find("![")?;
    let rest = &line[start + 2..];
    let close = rest.find(']')?;
    let alt = &rest[..close];

    // The alt text stops at the first ']', so a nested bracket means this is
    // not the simple form we handle.
    let after = rest[close + 1..].strip_prefix('(')?;
    let end = after.find(')')?;
    let inside = after[..end].trim();

    // <angle-bracketed src> may contain spaces; otherwise the src runs to the
    // first whitespace and anything after it is the title.
    let src = if let Some(stripped) = inside.strip_prefix('<') {
        stripped.split('>').next().unwrap_or("")
    } else {
        inside.split_whitespace().next().unwrap_or("")
    };

    (!src.is_empty()).then(|| InlineImage {
        alt: alt.to_string(),
        src: src.to_string(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn leaves_external_urls_alone() {
        for src in [
            "https://example.com/a.png",
            "http://example.com/a.png",
            "data:image/png;base64,AAAA",
            "//cdn.example.com/a.png",
        ] {
            assert!(is_external(src), "{src} should be external");
            assert_eq!(url(src, Some("note.md")), src);
        }
    }

    #[test]
    fn a_bare_filename_is_not_external() {
        assert!(!is_external("plan.png"));
        assert!(!is_external("images/plan.png"));
        assert!(!is_external("a-b.png"));
    }

    #[test]
    fn resolves_beside_the_note() {
        assert_eq!(
            resolve("plan.png", Some("projects/ideas.md")).as_deref(),
            Some("projects/plan.png")
        );
        assert_eq!(
            resolve("shots/plan.png", Some("projects/ideas.md")).as_deref(),
            Some("projects/shots/plan.png")
        );
    }

    #[test]
    fn a_leading_slash_means_the_vault_root() {
        assert_eq!(
            resolve("/plan.png", Some("projects/ideas.md")).as_deref(),
            Some("plan.png")
        );
    }

    #[test]
    fn walks_up_and_refuses_to_leave_the_vault() {
        assert_eq!(
            resolve("../plan.png", Some("a/b/note.md")).as_deref(),
            Some("a/plan.png")
        );
        assert_eq!(resolve("../../../plan.png", Some("a/note.md")), None);
    }

    #[test]
    fn strips_fragments_and_queries_and_decodes() {
        assert_eq!(resolve("plan.png#top", None).as_deref(), Some("plan.png"));
        assert_eq!(resolve("plan.png?v=2", None).as_deref(), Some("plan.png"));
        assert_eq!(resolve("my%20plan.png", None).as_deref(), Some("my plan.png"));
        // A bare '%' is a filename, not bad encoding.
        assert_eq!(resolve("100%.png", None).as_deref(), Some("100%.png"));
    }

    #[test]
    fn encodes_each_segment_but_keeps_slashes() {
        assert_eq!(
            url("my shots/a b.png", Some("note.md")),
            "/api/asset/my%20shots/a%20b.png"
        );
        assert_eq!(url("c#1.png", Some("note.md")), "/api/asset/c");
    }

    #[test]
    fn parses_inline_images() {
        assert_eq!(
            parse_image("![a cat](cat.png)"),
            Some(InlineImage { alt: "a cat".into(), src: "cat.png".into() })
        );
        assert_eq!(
            parse_image("text ![](x/y.png \"title\") more"),
            Some(InlineImage { alt: String::new(), src: "x/y.png".into() })
        );
        assert_eq!(
            parse_image("![a](<my file.png>)"),
            Some(InlineImage { alt: "a".into(), src: "my file.png".into() })
        );
        assert_eq!(parse_image("no image here"), None);
        assert_eq!(parse_image("[a link](x.md)"), None);
    }
}
