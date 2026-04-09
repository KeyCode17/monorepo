//! Response envelope shapes.
//!
//! Matches the Go source's `internal/server/handler.go` + the per-module
//! response structs byte-for-byte. Field order is significant — serde
//! preserves struct-field-order in its output, so tests can assert on
//! raw JSON bytes.

use serde::{Deserialize, Serialize};

/// Error envelope (no payload).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Response {
    pub code: u16,
    pub message: String,
}

/// Success envelope with a single object payload.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResponseSingleData<T: Serialize> {
    pub code: u16,
    pub data: T,
    pub message: String,
}

/// Success envelope with a paginated list payload.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResponseMultipleData<T: Serialize> {
    pub code: u16,
    pub data: Vec<T>,
    pub message: String,
}

/// Empty marker used as the `data` field when a success envelope has
/// no payload (e.g., DELETE responses).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Empty {}
