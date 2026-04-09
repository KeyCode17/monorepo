//! `apputils/` — port of `pkg/apputils/` from the Go source.
//!
//! Scaffold placeholder. Real modules land during D-AUTH-2:
//! - `jwt.rs`        — `JwtGenerator` with HS256 + rotation helpers
//! - `password.rs`   — argon2id with 16-byte salt (D-OPEN-2)
//! - `validation.rs` — validator error-map formatter
//! - `generator.rs`  — URL-safe token generator (38 alpha + 10-digit ts)
//! - `user_agent.rs` — UA parser port
