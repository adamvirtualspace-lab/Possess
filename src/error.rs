//! The error shape the frontend expects.
//!
//! FastAPI puts its message in `{"detail": ...}` and `js/api.js` `_detail()`
//! reads exactly that key to show "Not a folder: X" instead of a bare status
//! code. Anything we return on an error path has to keep that shape.

use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::Json;
use serde_json::json;

#[derive(Debug)]
pub struct AppError {
    pub status: StatusCode,
    pub detail: String,
}

impl AppError {
    pub fn new(status: u16, detail: impl Into<String>) -> Self {
        Self {
            status: StatusCode::from_u16(status).unwrap_or(StatusCode::INTERNAL_SERVER_ERROR),
            detail: detail.into(),
        }
    }

    pub fn bad_request(detail: impl Into<String>) -> Self {
        Self::new(400, detail)
    }

    pub fn forbidden(detail: impl Into<String>) -> Self {
        Self::new(403, detail)
    }

    pub fn not_found(detail: impl Into<String>) -> Self {
        Self::new(404, detail)
    }

    pub fn conflict(detail: impl Into<String>) -> Self {
        Self::new(409, detail)
    }

    pub fn unsupported_media(detail: impl Into<String>) -> Self {
        Self::new(415, detail)
    }

    pub fn internal(detail: impl Into<String>) -> Self {
        Self::new(500, detail)
    }
}

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        (self.status, Json(json!({ "detail": self.detail }))).into_response()
    }
}

/// Any io::Error that reaches a handler unhandled is a 500. Handlers that care
/// about a specific failure (missing file, permission denied) map it themselves
/// before it gets here.
impl From<std::io::Error> for AppError {
    fn from(err: std::io::Error) -> Self {
        AppError::internal(err.to_string())
    }
}

pub type AppResult<T> = Result<T, AppError>;
