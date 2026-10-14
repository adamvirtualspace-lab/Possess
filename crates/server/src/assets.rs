//! Images: serving them out of the vault, and saving pasted ones into it.

use crate::error::{AppError, AppResult};
use crate::notes::trim_rel;
use crate::vault::AppState;
use axum::body::Body;
use axum::extract::{Path as UrlPath, State};
use axum::http::{header, HeaderMap, HeaderValue, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::Json;
use base64::Engine;
use chrono::Local;
use possess_common::{PasteImageRequest, PasteImageResponse};
use std::path::{Path, PathBuf};

/// Extensions served by /api/asset. Deliberately an allow-list of media types:
/// the endpoint exists so notes can show their own pictures, not to be a
/// general "read any file in the vault" hatch.
const ASSET_SUFFIXES: &[&str] = &[
    "png", "jpg", "jpeg", "gif", "webp", "avif", "bmp", "svg", "ico",
];

/// Pasted images live in "<note stem>_images/" beside the note, so a note and
/// its pictures move, copy and delete as one obvious unit.
const IMAGES_SUFFIX: &str = "_images";
const PASTE_MAX_BYTES: usize = 25 * 1024 * 1024;

/// Serve an image from the vault so relative paths in notes resolve.
///
/// A note writing ![](images/plan.png) means "next to this note", but the
/// browser resolves that against the page URL. The frontend rewrites such paths
/// to point here, and this reads them back out of the vault.
pub async fn get_asset(
    State(state): State<AppState>,
    UrlPath(filepath): UrlPath<String>,
) -> AppResult<Response> {
    let full = state.resolve(&filepath)?;

    let suffix = full
        .extension()
        .map(|e| e.to_string_lossy().to_lowercase())
        .unwrap_or_default();

    if !ASSET_SUFFIXES.contains(&suffix.as_str()) {
        return Err(AppError::unsupported_media(format!("Not an image: {filepath}")));
    }
    if !full.is_file() {
        return Err(AppError::not_found(format!("Image not found: {filepath}")));
    }

    let bytes = std::fs::read(&full)?;
    let media_type = mime_guess::from_path(&full)
        .first_raw()
        .unwrap_or("application/octet-stream");

    let mut headers = HeaderMap::new();
    headers.insert(header::CONTENT_TYPE, HeaderValue::from_static(media_type));
    headers.insert(header::CACHE_CONTROL, HeaderValue::from_static("no-cache"));

    // An SVG is a document: opened directly in a tab, script inside it would
    // run on this origin. Rendering happens in <img>, where script never runs,
    // so lock the file itself down for the direct-navigation case too.
    if suffix == "svg" {
        headers.insert(
            header::CONTENT_SECURITY_POLICY,
            HeaderValue::from_static("default-src 'none'; style-src 'unsafe-inline'"),
        );
    }

    Ok((StatusCode::OK, headers, Body::from(bytes)).into_response())
}

/// Leading bytes are trusted over the browser's declared MIME type, which is
/// whatever the source app put on the clipboard.
const MAGIC: &[(&[u8], &str)] = &[
    (b"\x89PNG\r\n\x1a\n", ".png"),
    (b"\xff\xd8\xff", ".jpg"),
    (b"GIF87a", ".gif"),
    (b"GIF89a", ".gif"),
    (b"BM", ".bmp"),
];

/// The real extension for these bytes, or None if it isn't an image.
fn sniff_image(data: &[u8]) -> Option<&'static str> {
    for (magic, suffix) in MAGIC {
        if data.starts_with(magic) {
            return Some(suffix);
        }
    }

    // RIFF....WEBP and ....ftypavif carry their marker past the first bytes.
    if data.len() >= 12 && &data[..4] == b"RIFF" && &data[8..12] == b"WEBP" {
        return Some(".webp");
    }
    if data.len() >= 24 && &data[4..8] == b"ftyp" && contains(&data[8..24], b"avif") {
        return Some(".avif");
    }

    let head_len = data.len().min(400);
    let head: &[u8] = {
        let slice = &data[..head_len];
        let start = slice.iter().position(|b| !b.is_ascii_whitespace()).unwrap_or(head_len);
        &slice[start..]
    };

    let xml_len = data.len().min(2000);
    if head.starts_with(b"<svg") || (head.starts_with(b"<?xml") && contains(&data[..xml_len], b"<svg"))
    {
        return Some(".svg");
    }
    None
}

fn contains(haystack: &[u8], needle: &[u8]) -> bool {
    haystack.windows(needle.len()).any(|w| w == needle)
}

/// A path in `folder` that doesn't exist yet, suffixing -2, -3 ...
fn unique_path(folder: &Path, stem: &str, suffix: &str) -> PathBuf {
    let mut candidate = folder.join(format!("{stem}{suffix}"));
    let mut counter = 2;
    while candidate.exists() {
        candidate = folder.join(format!("{stem}-{counter}{suffix}"));
        counter += 1;
    }
    candidate
}

/// Save an image pasted into a note, and say how to link to it.
///
/// The client sends base64 because a clipboard image has no file on disk to
/// upload. Returns the path relative to the note, which is what goes in the
/// markdown — an absolute path would break if the vault moved.
pub async fn paste_image(
    State(state): State<AppState>,
    Json(body): Json<PasteImageRequest>,
) -> AppResult<Json<PasteImageResponse>> {
    let note = trim_rel(body.note.as_deref().unwrap_or(""));
    if note.is_empty() {
        return Err(AppError::bad_request(
            "No note given — open a note before pasting",
        ));
    }

    let note_path = state.resolve(&note)?;
    if !note_path.is_file() {
        return Err(AppError::not_found(format!("No such note: {note}")));
    }

    let mut raw = body.data.unwrap_or_default();
    if raw.starts_with("data:") {
        if let Some((_, rest)) = raw.split_once(',') {
            raw = rest.to_string();
        }
    }

    let data = base64::engine::general_purpose::STANDARD
        .decode(raw.as_bytes())
        .map_err(|_| AppError::bad_request("Image data was not valid base64"))?;

    if data.is_empty() {
        return Err(AppError::bad_request("Image data was empty"));
    }
    if data.len() > PASTE_MAX_BYTES {
        return Err(AppError::new(
            413,
            format!(
                "Image is {}MB; the limit is {}MB",
                data.len() / 1_048_576,
                PASTE_MAX_BYTES / 1_048_576
            ),
        ));
    }

    let suffix = sniff_image(&data)
        .ok_or_else(|| AppError::unsupported_media("That doesn't look like an image"))?;

    let stem_name = note_path.file_stem().unwrap_or_default().to_string_lossy().to_string();
    let folder = note_path
        .parent()
        .ok_or_else(|| AppError::bad_request(format!("No such note: {note}")))?
        .join(format!("{stem_name}{IMAGES_SUFFIX}"));
    std::fs::create_dir_all(&folder)?;

    let stem = paste_stem(body.name.as_deref());
    let target = unique_path(&folder, &stem, suffix);
    std::fs::write(&target, &data)?;

    let vault_path = state.vault_resolved();
    let rel = target
        .strip_prefix(&vault_path)
        .map(|p| {
            p.components()
                .map(|c| c.as_os_str().to_string_lossy())
                .collect::<Vec<_>>()
                .join("/")
        })
        .unwrap_or_else(|_| target.display().to_string());

    let folder_name = folder.file_name().unwrap_or_default().to_string_lossy();
    let file_name = target.file_name().unwrap_or_default().to_string_lossy();

    Ok(Json(PasteImageResponse {
        path: rel,
        markdown: format!("{folder_name}/{file_name}"),
        bytes: data.len(),
    }))
}

/// A safe filename stem: the source name when it's usable, else a stamp.
fn paste_stem(name: Option<&str>) -> String {
    if let Some(name) = name.filter(|n| !n.is_empty()) {
        let stem = Path::new(name)
            .file_stem()
            .unwrap_or_default()
            .to_string_lossy()
            .to_string();

        let mut cleaned = String::new();
        let mut in_run = false;
        for ch in stem.chars() {
            if ch.is_ascii_alphanumeric() || ch == '.' || ch == '_' || ch == '-' {
                cleaned.push(ch);
                in_run = false;
            } else if !in_run {
                cleaned.push('-');
                in_run = true;
            }
        }
        let cleaned = cleaned.trim_matches(['-', '.']).to_string();

        // "image" is what browsers call every clipboard bitmap; a timestamp
        // tells two screenshots apart where "image-7" doesn't.
        let lower = cleaned.to_lowercase();
        if !cleaned.is_empty() && !matches!(lower.as_str(), "image" | "screenshot" | "untitled") {
            return cleaned.chars().take(60).collect();
        }
    }

    format!("pasted-{}", Local::now().format("%Y%m%d-%H%M%S"))
}
