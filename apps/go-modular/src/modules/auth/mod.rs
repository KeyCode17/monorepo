//! Auth module — 14 endpoints (fix-track).
//!
//! Scaffold placeholder. Real module lands during D-AUTH-1..15 with:
//! - Transactional signin (design 3.3)
//! - Rotate-on-refresh + reuse detection (3.1)
//! - Session-check middleware with <5ms p99 budget (3.2)
//! - argon2id with 16-byte salt (D-OPEN-2)
//! - Atomic email verification (3.8)
//! - Ownership checks on password endpoints (3.9)
