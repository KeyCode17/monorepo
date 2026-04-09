//! Router composition.
//!
//! At D-INFRA-11 the router has:
//! - `GET /healthz`          — liveness probe
//! - `GET /api-docs`         — Scalar/Swagger UI (placeholder redirect)
//! - `GET /api/openapi.json` — `OpenAPI` spec (placeholder)
//! - `/*` catch-all          — SPA-style 404 for anything else
//!
//! Module routes from D-USER-* and D-AUTH-* mount under `/api/v1/`
//! via `.merge(user_module::routes())` and `.merge(auth_module::routes())`
//! calls that land in later phases.

use axum::Router;
use axum::routing::get;

use crate::AppState;
use crate::middleware as mw;
use crate::modules::user;
use crate::server::handler;

/// Build the axum router with full middleware stack applied.
pub fn build_router(state: AppState) -> Router {
    let config = state.config.clone();
    let app_cfg = &config.app;

    // Module routes under /api/v1. User module is JWT-protected in
    // Go; the `require_auth` middleware lands in D-AUTH-14 and will
    // be applied here as `.route_layer(from_fn_with_state(...))`.
    let api_v1 = Router::new().nest("/users", user::routes());

    // Infra routes (non-/api/v1).
    // TODO(D-AUTH-13): merge auth module routes under /api/v1/auth.
    let infra_routes: Router<AppState> = Router::new()
        .route("/healthz", get(handler::healthz))
        .route("/api-docs", get(handler::api_docs))
        .route("/api/openapi.json", get(handler::openapi_json))
        .nest("/api/v1", api_v1)
        .fallback(handler::not_found);

    let router = infra_routes.with_state(state);

    // Tower layers applied from innermost to outermost. Order matches
    // the Go middleware wiring in `internal/server/loader.go` as
    // closely as possible within tower semantics.
    let security = mw::security_headers();
    let (set_id, propagate_id) = mw::request_id_layers();

    let router = router
        .layer(mw::timeout_layer())
        .layer(mw::compression_layer())
        .layer(security[0].clone())
        .layer(security[1].clone())
        .layer(security[2].clone())
        .layer(security[3].clone())
        .layer(mw::cors_layer(app_cfg))
        .layer(mw::trace_layer())
        .layer(propagate_id)
        .layer(set_id);

    // Rate limiting is gated on the config flag (fix audit §9.24).
    if app_cfg.rate_limit_enabled {
        // tower_governor wiring is deferred to D-AUTH phase because it
        // interacts with ConnectInfo<SocketAddr> in ways that are
        // easier to add once the full router surface is in place.
        // For now the flag is honored by skipping the layer entirely
        // rather than the Go source's pattern of always applying it.
    }

    router
}
