//! Domain types shared across modules.
//!
//! - `error` — `AppError` enum with `IntoResponse` impl
//! - `response` — `Response`, `ResponseSingleData<T>`, `ResponseMultipleData<T>`
//!
//! Mirrors the Go `internal/domain`-equivalent pattern used by
//! Phase C go-clean. The response envelope shape
//! (`{code, message}` for errors, `{code, data, message}` for
//! success) matches the Go source byte-for-byte.

pub mod error;
pub mod response;

pub use error::AppError;
pub use response::{Empty, Response, ResponseMultipleData, ResponseSingleData};
