//! Rust port of the Zero One Group go-modular service (Phase D).
//!
//! Corrected-port rewrite of `apps/go-modular` (originally Go/Echo) to
//! Rust/axum. 19 HTTP endpoints (14 auth + 5 user) with 8 audit-driven
//! behavior fixes vs the Go original.
//!
//! ## Current checkpoint: D-INFRA-11 (infrastructure complete)
//!
//! At this checkpoint the crate has:
//! - Config loader (6 sections / 39 fields) via `figment`
//! - Database pool with retry (`sqlx::PgPoolOptions`)
//! - Observer scaffold (`tracing_subscriber`; `OTel` wiring deferred)
//! - Middleware stack (8 tower layers, honors `rate_limit_enabled`)
//! - Error envelope (`AppError` enum + `IntoResponse`)
//! - DI wiring (`AppState` with `from_config` + `from_parts` constructors)
//! - Server scaffold (`serve` + `build_router` + healthz/api-docs/404)
//! - 6 migration SQL files at `migrations/` (BYTEA harmonized,
//!   `uuidv7()` DB default stripped in favor of app-side generation)
//!
//! NOT yet landed (arriving in later phase tracks):
//! - `apputils::{jwt, password, generator, ...}` (D-AUTH-2)
//! - `modules::user::*` (D-USER-1..4)
//! - `modules::auth::{repository, service, handler}` (D-AUTH-3..13)
//! - `mailer::*` (D-SMTP-1..4)
//! - `cli::*` full clap tree (D-CLI-1..5)
//! - Integration tests (D-IT-1..6)
//!
//! Public surface for tests and the binary:
//! - [`serve`] — boot the HTTP server, block until shutdown
//! - [`build_router`] — build the axum router alone (for test harness)
//! - [`AppState`] — shared state (config + DB pool; services land later)

pub mod apputils;
pub mod cli;
pub mod config;
pub mod database;
pub mod domain;
pub mod mailer;
pub mod middleware;
pub mod modules;
pub mod observer;
pub mod server;

use std::sync::Arc;

use anyhow::Result;
use sqlx::PgPool;

use crate::config::Config;
use crate::modules::user::{UserRepository, UserService};

pub use crate::server::serve;

/// Application state shared with every axum handler via
/// `axum::extract::State`.
///
/// After D-USER-1..4 this holds `config`, `pool`, and the user
/// service. Auth services (`Arc<AuthService>`) and the mailer
/// (`Arc<Mailer>`) land as D-AUTH / D-SMTP phases complete.
#[derive(Clone)]
pub struct AppState {
    pub config: Arc<Config>,
    pub pool: PgPool,
    pub user_service: Arc<UserService>,
}

impl AppState {
    /// Construct from a fully-loaded [`Config`]. Opens the DB pool
    /// with the retry semantics from [`crate::database::connect_pool`]
    /// and wires the user service.
    pub async fn from_config(config: Config) -> Result<Self> {
        let pool = crate::database::connect_pool(&config.database).await?;
        Ok(Self::from_parts(config, pool))
    }

    /// Test-only constructor: inject an existing `PgPool` (from a
    /// testcontainer) alongside a pre-built `Config`.
    pub fn from_parts(config: Config, pool: PgPool) -> Self {
        let user_repo = Arc::new(UserRepository::new(pool.clone()));
        let user_service = Arc::new(UserService::new(user_repo));
        Self {
            config: Arc::new(config),
            pool,
            user_service,
        }
    }
}

/// Convenience wrapper for tests that want a ready-to-use router
/// without spinning up a real listener.
pub fn build_router(state: AppState) -> axum::Router {
    crate::server::router::build_router(state)
}
