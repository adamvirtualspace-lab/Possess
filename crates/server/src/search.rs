//! Full-text search across the vault's notes.

use crate::error::AppResult;
use crate::notes::walk_notes;
use crate::vault::AppState;
use axum::extract::{Query, State};
use axum::Json;
use possess_common::{SearchResponse, SearchResult};
use serde::Deserialize;

fn default_limit() -> usize {
    100
}

#[derive(Deserialize)]
pub struct SearchQuery {
    q: String,
    #[serde(default = "default_limit")]
    limit: usize,
}

/// Case-insensitive full-text search across the vault's .md files.
///
/// Returns one entry per matching file with the first matching line as a
/// snippet, so the sidebar can show context without shipping whole notes.
pub async fn search(
    State(state): State<AppState>,
    Query(params): Query<SearchQuery>,
) -> AppResult<Json<SearchResponse>> {
    let needle = params.q.trim().to_lowercase();
    if needle.is_empty() {
        return Ok(Json(SearchResponse { query: params.q, results: Vec::new() }));
    }

    let base = state.vault_resolved();
    let mut results: Vec<SearchResult> = Vec::new();

    // walkdir makes no ordering promise, so collect and sort for a stable
    // result list across calls.
    let mut paths: Vec<_> = walk_notes(&base).map(|e| e.into_path()).collect();
    paths.sort();

    for full in paths {
        let Ok(rel) = full.strip_prefix(&base) else { continue };
        let rel = rel
            .components()
            .map(|c| c.as_os_str().to_string_lossy())
            .collect::<Vec<_>>()
            .join("/");

        let name = full.file_name().unwrap_or_default().to_string_lossy().to_string();
        let name_hit = name.to_lowercase().contains(&needle);

        // read_to_string fails on invalid UTF-8 where Python used
        // errors="replace"; read the bytes and do the same.
        let Ok(bytes) = std::fs::read(&full) else { continue };
        let text = String::from_utf8_lossy(&bytes);

        let snippet = text
            .lines()
            .find(|line| line.to_lowercase().contains(&needle))
            .map(|line| truncate_chars(line.trim(), 160))
            .unwrap_or_default();

        if !snippet.is_empty() || name_hit {
            results.push(SearchResult { path: rel, name, snippet });
        }
        if results.len() >= params.limit {
            break;
        }
    }

    Ok(Json(SearchResponse { query: params.q, results }))
}

/// Python sliced by characters; slicing bytes would panic mid-codepoint.
fn truncate_chars(text: &str, limit: usize) -> String {
    text.chars().take(limit).collect()
}
