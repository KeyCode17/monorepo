//! Infrastructure route handlers: healthz, api-docs, openapi.json, 404.
//!
//! Matches the Go `internal/server/handler.go` route set. The api-docs
//! and openapi.json responses are placeholders until D-DOC-1 wires
//! utoipa's surface.

use axum::Json;
use axum::extract::State;
use axum::http::StatusCode;
use axum::response::IntoResponse;
use serde_json::json;

use crate::AppState;
use crate::domain::{Empty, ResponseSingleData};

/// `GET /healthz` — liveness probe. Returns 200 with
/// `{code, data, message}` envelope.
pub async fn healthz(State(_state): State<AppState>) -> impl IntoResponse {
    (
        StatusCode::OK,
        Json(ResponseSingleData::<Empty> {
            code: 200,
            data: Empty {},
            message: "ok".to_string(),
        }),
    )
}

/// `GET /api-docs` — Scalar / Swagger UI. Placeholder for D-DOC-1.
pub async fn api_docs(State(state): State<AppState>) -> impl IntoResponse {
    if !state.config.app.enable_api_docs {
        return (
            StatusCode::NOT_FOUND,
            Json(json!({"code": 404, "message": "api docs disabled"})),
        );
    }
    (
        StatusCode::OK,
        Json(json!({
            "code": 200,
            "message": "API docs placeholder — full utoipa surface lands in D-DOC-1",
            "openapi_url": "/api/openapi.json"
        })),
    )
}

/// `GET /api/openapi.json` — `OpenAPI` spec. Placeholder for D-DOC-1.
pub async fn openapi_json(State(state): State<AppState>) -> impl IntoResponse {
    if !state.config.app.enable_api_docs {
        return (
            StatusCode::NOT_FOUND,
            Json(json!({"code": 404, "message": "api docs disabled"})),
        );
    }
    (
        StatusCode::OK,
        Json(json!({
            "openapi": "3.0.3",
            "info": {
                "title": "go-modular",
                "version": env!("CARGO_PKG_VERSION"),
                "description": "Phase D scaffold — full surface lands in D-DOC-1"
            },
            "paths": {}
        })),
    )
}

/// Catch-all 404.
pub async fn not_found() -> impl IntoResponse {
    (
        StatusCode::NOT_FOUND,
        Json(ResponseSingleData::<Empty> {
            code: 404,
            data: Empty {},
            message: "route not found".to_string(),
        }),
    )
}
