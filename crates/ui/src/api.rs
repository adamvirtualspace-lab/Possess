//! The typed API client.
//!
//! Every call goes through the same two helpers, so the error shape is handled
//! once: the server reports failures as `{"detail": ...}` and that message is
//! what the UI shows — "Not a folder: X" rather than a bare status code.
//!
//! Request and response types come from `possess_common`, shared with the
//! server, so a field renamed there fails this crate's build rather than
//! silently returning `undefined` at runtime the way `js/api.js` would.

// The client is written out in full ahead of the components that call it, so
// the API surface lives in one place rather than growing a call at a time.
#![allow(dead_code)]

use gloo_net::http::Request;
use possess_common::*;
use serde::de::DeserializeOwned;
use serde::Serialize;

/// Send a GET and decode the body, surfacing the server's own message on error.
async fn get<T: DeserializeOwned>(url: &str) -> Result<T, String> {
    let response = Request::get(url)
        .send()
        .await
        .map_err(|err| format!("Request failed: {err}"))?;

    decode(response).await
}

/// Send a JSON POST and decode the body.
async fn post<B: Serialize, T: DeserializeOwned>(url: &str, body: &B) -> Result<T, String> {
    let response = Request::post(url)
        .json(body)
        .map_err(|err| format!("Could not encode request: {err}"))?
        .send()
        .await
        .map_err(|err| format!("Request failed: {err}"))?;

    decode(response).await
}

/// Turn a response into either the decoded body or the server's `detail`.
async fn decode<T: DeserializeOwned>(response: gloo_net::http::Response) -> Result<T, String> {
    let status = response.status();
    let text = response
        .text()
        .await
        .map_err(|err| format!("Could not read response: {err}"))?;

    if !(200..300).contains(&status) {
        // Fall back to the status when the body isn't the shape we expect,
        // which is what a proxy or a panic would produce.
        return Err(serde_json::from_str::<ErrorBody>(&text)
            .map(|body| body.detail)
            .unwrap_or_else(|_| format!("Request failed: {status}")));
    }

    serde_json::from_str(&text).map_err(|err| format!("Could not decode response: {err}"))
}

fn encode(segment: &str) -> String {
    // Percent-encode everything that is not unreserved, so note names
    // containing #, ?, % or spaces address the right file.
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

// ── Vault ──

pub async fn get_vault() -> Result<VaultInfo, String> {
    get("/api/vault").await
}

pub async fn set_vault(path: &str) -> Result<VaultInfo, String> {
    post(
        "/api/vault",
        &SetVaultRequest { path: Some(path.to_string()) },
    )
    .await
}

pub async fn browse(path: Option<&str>) -> Result<BrowseResponse, String> {
    let url = match path {
        Some(path) => format!("/api/browse?path={}", encode(path)),
        None => "/api/browse".to_string(),
    };
    get(&url).await
}

// ── Notes ──

pub async fn list_folders(root: Option<&str>) -> Result<FoldersResponse, String> {
    let url = match root {
        Some(root) => format!("/api/folders?root={}", encode(root)),
        None => "/api/folders".to_string(),
    };
    get(&url).await
}

pub async fn get_file(filepath: &str) -> Result<FileResponse, String> {
    get(&format!("/api/file/{}", encode(filepath))).await
}

pub async fn save_file(filepath: &str, content: &str) -> Result<SaveFileResponse, String> {
    post(
        &format!("/api/file/{}", encode(filepath)),
        &SaveFileRequest { content: content.to_string() },
    )
    .await
}

pub async fn get_status(filepath: &str, mtime: f64) -> Result<StatusResponse, String> {
    get(&format!(
        "/api/status?filepath={}&mtime={mtime}",
        encode(filepath)
    ))
    .await
}

pub async fn create(kind: &str, parent: &str, name: &str) -> Result<CreateResponse, String> {
    post(
        "/api/create",
        &CreateRequest {
            kind: Some(kind.to_string()),
            parent: Some(parent.to_string()),
            name: Some(name.to_string()),
            content: String::new(),
        },
    )
    .await
}

pub async fn rename(path: &str, name: &str) -> Result<RenameResponse, String> {
    post(
        "/api/rename",
        &RenameRequest {
            path: Some(path.to_string()),
            name: Some(name.to_string()),
        },
    )
    .await
}

pub async fn remove(path: &str) -> Result<DeleteResponse, String> {
    post("/api/delete", &DeleteRequest { path: Some(path.to_string()) }).await
}

pub async fn restore(token: &str) -> Result<RestoreResponse, String> {
    post("/api/restore", &RestoreRequest { token: Some(token.to_string()) }).await
}

pub async fn search(query: &str) -> Result<SearchResponse, String> {
    get(&format!("/api/search?q={}", encode(query))).await
}

// ── Images ──

pub async fn paste_image(
    note: &str,
    name: Option<&str>,
    data: &str,
) -> Result<PasteImageResponse, String> {
    post(
        "/api/paste-image",
        &PasteImageRequest {
            note: Some(note.to_string()),
            name: name.map(str::to_string),
            data: Some(data.to_string()),
        },
    )
    .await
}

// ── Scripts ──

pub async fn list_scripts() -> Result<ScriptsResponse, String> {
    get("/api/scripts").await
}

pub async fn run_script(name: &str, note: Option<&str>) -> Result<RunScriptResponse, String> {
    post(
        "/api/scripts/run",
        &RunScriptRequest {
            name: Some(name.to_string()),
            note: note.map(str::to_string),
        },
    )
    .await
}

pub async fn set_hooks(enabled: bool) -> Result<SetHooksResponse, String> {
    post("/api/scripts/hooks", &SetHooksRequest { enabled }).await
}

pub async fn scaffold_scripts() -> Result<ScaffoldResponse, String> {
    post("/api/scripts/scaffold", &serde_json::json!({})).await
}
