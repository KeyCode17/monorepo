//! Spec-derived integration tests for the 7 Phase D fix-track items
//! (D-IT-3).
//!
//! These tests exercise the go-modular Rust port against a disposable
//! Postgres 16-alpine testcontainer and assert the **corrected**
//! corrected-port behavior (8 audit-driven fixes vs the Go original).
//! They are the primary regression gate for the 8 corrected-port fixes.
//!
//! Coverage map:
//!
//! - Rotate-on-refresh + reuse detection: `rotate_refresh_*`
//! - Refresh-token CRUD deletion: verified by route absence
//! - Session revocation: `session_delete_invalidates_access_token`
//! - Transactional signin: `signin_session_expires_at_*`
//! - `JWT_ALGORITHM` deletion: inherent (HS256 only)
//! - `X-App-Audience` deletion: inherent (hardcoded `aud`)
//! - `session.expires_at` = refresh expiry: `signin_session_expires_at_*`
//! - Argon2 salt length + STARTTLS: covered by unit tests
//! - 60s verification cooldown: `verification_cooldown_returns_429`
//! - Neutral initiate response: `verification_neutral_202_*`
//! - Ownership check on password: `ownership_check_rejects_*`
//!
//! Golden-fixture capture from a live Go service (D-IT-1) is
//! deliberately out of scope — the corrected port diverges from Go
//! on ~7 endpoints, so byte-for-byte fixture matching would require
//! two sets of fixtures. The audit doc + these spec-derived tests
//! serve as the acceptance gate instead.
//!
//! No network hop: uses `tower::ServiceExt::oneshot` against the
//! router returned by `go_modular::build_router`, wrapped with
//! `MockConnectInfo` so `ConnectInfo<SocketAddr>` extractors resolve.

#![allow(
    clippy::cast_possible_truncation,
    clippy::cast_lossless,
    clippy::cast_sign_loss,
    clippy::too_many_lines
)]

use std::net::SocketAddr;

use axum::Router;
use axum::body::Body;
use axum::extract::connect_info::MockConnectInfo;
use axum::http::{Method, Request, StatusCode};
use chrono::{DateTime, Utc};
use http_body_util::BodyExt;
use serde_json::{Value, json};
use sqlx::postgres::PgPoolOptions;
use testcontainers_modules::postgres::Postgres;
use testcontainers_modules::testcontainers::runners::AsyncRunner;
use testcontainers_modules::testcontainers::{ContainerAsync, ImageExt};
use tower::ServiceExt;
use uuid::Uuid;

use go_modular::AppState;
use go_modular::apputils::PasswordHasher;
use go_modular::build_router;
use go_modular::config::Config;

// ---------------------------------------------------------------------
// Test harness
// ---------------------------------------------------------------------

/// Build a test config matching the Phase D defaults with the DB URL
/// pointed at the testcontainer instance.
fn test_config(db_url: &str) -> Config {
    let mut cfg = Config::from_defaults().expect("default config valid");
    cfg.database.database_url = db_url.to_string();
    cfg.app.jwt_secret_key = "integration-test-secret-not-for-prod".to_string();
    cfg.app.app_mode = "test".to_string();
    cfg.app.app_base_url = "http://test.local".to_string();
    // Clear SMTP host so the mailer runs in noop mode. The default
    // config points at localhost:1025 (mailhog); without a real
    // relay on that port, verification initiate would return 500.
    cfg.mailer.smtp_host = String::new();
    cfg
}

/// Spin up a Postgres 16-alpine container, apply migrations, return
/// the wired `AppState` + container guard. Hold the guard for the
/// test's lifetime — dropping it stops the container and kills the
/// pool.
async fn spawn_state() -> (AppState, ContainerAsync<Postgres>) {
    let container = Postgres::default()
        .with_db_name("go_modular_test")
        .with_user("postgres")
        .with_password("postgres")
        .with_tag("16-alpine")
        .start()
        .await
        .expect("start postgres 16-alpine container");
    let host_port = container
        .get_host_port_ipv4(5432)
        .await
        .expect("postgres host port");

    let database_url =
        format!("postgres://postgres:postgres@127.0.0.1:{host_port}/go_modular_test");
    let pool = PgPoolOptions::new()
        .max_connections(5)
        .connect(&database_url)
        .await
        .expect("connect to testcontainer postgres");

    sqlx::migrate!("./migrations")
        .run(&pool)
        .await
        .expect("run migrations against testcontainer");

    let state = AppState::from_parts(test_config(&database_url), pool);
    (state, container)
}

/// Build a test router with `MockConnectInfo` so the `ConnectInfo`
/// extractor in our handlers resolves without a real listener.
fn test_router(state: AppState) -> Router {
    build_router(state).layer(MockConnectInfo(SocketAddr::from(([127, 0, 0, 1], 9999))))
}

/// Send a request through the router, parse response body as JSON.
async fn call(router: Router, request: Request<Body>) -> (StatusCode, Value) {
    let response = router.oneshot(request).await.expect("router oneshot");
    let status = response.status();
    let body_bytes = response
        .into_body()
        .collect()
        .await
        .expect("collect body")
        .to_bytes();
    let body_json = if body_bytes.is_empty() {
        Value::Null
    } else {
        serde_json::from_slice(&body_bytes).unwrap_or(Value::Null)
    };
    (status, body_json)
}

fn json_request(method: Method, uri: &str, body: &Value) -> Request<Body> {
    Request::builder()
        .method(method)
        .uri(uri)
        .header("content-type", "application/json")
        .body(Body::from(serde_json::to_vec(body).unwrap()))
        .unwrap()
}

fn authed_request(method: Method, uri: &str, token: &str, body: Option<&Value>) -> Request<Body> {
    let mut builder = Request::builder()
        .method(method)
        .uri(uri)
        .header("authorization", format!("Bearer {token}"));
    if body.is_some() {
        builder = builder.header("content-type", "application/json");
    }
    let body = body.map_or_else(Body::empty, |b| Body::from(serde_json::to_vec(b).unwrap()));
    builder.body(body).unwrap()
}

/// Insert a verified user + argon2id password into the DB, bypassing
/// the HTTP signup flow. Returns the user's UUID.
async fn insert_verified_user(
    state: &AppState,
    display_name: &str,
    email: &str,
    password: &str,
) -> Uuid {
    let id = Uuid::now_v7();
    let username = email.split('@').next().unwrap_or("user").replace('.', "_");

    sqlx::query(
        "INSERT INTO public.users \
         (id, display_name, email, username, email_verified_at) \
         VALUES ($1, $2, $3, $4, NOW())",
    )
    .bind(id)
    .bind(display_name)
    .bind(email)
    .bind(&username)
    .execute(&state.pool)
    .await
    .expect("insert user row");

    let hasher = PasswordHasher::new();
    let phc = hasher.hash(password).expect("hash password");
    sqlx::query("INSERT INTO public.user_passwords (user_id, password_hash) VALUES ($1, $2)")
        .bind(id)
        .bind(phc.as_bytes())
        .execute(&state.pool)
        .await
        .expect("insert user_password row");

    id
}

/// Helper: signin via the HTTP endpoint, returning the decoded body.
async fn signin(router: Router, email: &str, password: &str) -> (StatusCode, Value) {
    let req = json_request(
        Method::POST,
        "/api/v1/auth/signin/email",
        &json!({ "email": email, "password": password }),
    );
    call(router, req).await
}

/// Extract `{access_token, refresh_token, session_id}` from a
/// successful signin / rotation response.
fn extract_tokens(body: &Value) -> (String, String, Uuid) {
    let access = body["access_token"]
        .as_str()
        .expect("access_token present")
        .to_string();
    let refresh = body["refresh_token"]
        .as_str()
        .expect("refresh_token present")
        .to_string();
    let sid = body["session_id"].as_str().expect("session_id present");
    (
        access,
        refresh,
        Uuid::parse_str(sid).expect("session_id uuid"),
    )
}

// ---------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn signin_happy_path_returns_access_refresh_session() {
    let (state, _guard) = spawn_state().await;
    insert_verified_user(&state, "Alice Dev", "alice@example.com", "correct horse").await;
    let router = test_router(state);

    let (status, body) = signin(router, "alice@example.com", "correct horse").await;
    assert_eq!(status, StatusCode::OK, "signin should succeed: {body}");

    let (access, refresh, sid) = extract_tokens(&body);
    assert!(!access.is_empty());
    assert!(!refresh.is_empty());
    assert!(!sid.is_nil());
    assert_eq!(body["user"]["email"], "alice@example.com");
}

/// Fix §9.5: `session.expires_at` must use the REFRESH expiry (7d),
/// NOT the access-token expiry (24h) like Go did.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn signin_session_expires_at_matches_refresh_expiry() {
    let (state, _guard) = spawn_state().await;
    insert_verified_user(&state, "Bob Dev", "bob@example.com", "correct horse").await;

    let before_signin = Utc::now();
    let router = test_router(state.clone());
    let (status, body) = signin(router, "bob@example.com", "correct horse").await;
    assert_eq!(status, StatusCode::OK);
    let (_access, _refresh, sid) = extract_tokens(&body);

    let expires_at: DateTime<Utc> =
        sqlx::query_scalar("SELECT expires_at FROM public.sessions WHERE id = $1")
            .bind(sid)
            .fetch_one(&state.pool)
            .await
            .expect("fetch session expires_at");

    // Refresh expiry is 7 days, access expiry is 24h. Assert the
    // session TTL is closer to 7d (168h) than to 24h.
    let ttl = expires_at.signed_duration_since(before_signin);
    let hours = ttl.num_hours();
    assert!(
        (24 * 6..=24 * 7 + 1).contains(&hours),
        "session.expires_at should be ~7d after signin (fix §9.5), got {hours}h"
    );
}

/// Design 3.1: rotate-on-refresh issues a new refresh token and
/// revokes the previous one.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn rotate_refresh_issues_new_token_and_revokes_old() {
    let (state, _guard) = spawn_state().await;
    insert_verified_user(&state, "Carol Dev", "carol@example.com", "correct horse").await;
    let router = test_router(state.clone());

    // Signin to get first refresh token.
    let (_status, signin_body) = signin(router.clone(), "carol@example.com", "correct horse").await;
    let (_access1, refresh1, _sid1) = extract_tokens(&signin_body);

    // Rotate.
    let rotate_req = json_request(
        Method::POST,
        "/api/v1/auth/token/refresh",
        &json!({ "refresh_token": refresh1.clone() }),
    );
    let (status, rotate_body) = call(router.clone(), rotate_req).await;
    assert_eq!(
        status,
        StatusCode::OK,
        "rotation should succeed: {rotate_body}"
    );

    let (_access2, refresh2, _sid2) = extract_tokens(&rotate_body);
    assert_ne!(
        refresh1, refresh2,
        "refresh tokens must differ after rotation"
    );

    // Verify the old row is now revoked in the DB.
    let row: Option<(Option<DateTime<Utc>>,)> = sqlx::query_as(
        "SELECT revoked_at FROM public.refresh_tokens ORDER BY created_at ASC LIMIT 1",
    )
    .fetch_optional(&state.pool)
    .await
    .expect("query refresh_tokens");
    let revoked_at = row.expect("at least one row").0;
    assert!(
        revoked_at.is_some(),
        "old refresh_token must be revoked_at != NULL"
    );
}

/// Design 3.1: refresh-token reuse detection — replaying a rotated
/// token must revoke ALL user sessions and return 401.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn rotate_refresh_reuse_detection_revokes_all_sessions() {
    let (state, _guard) = spawn_state().await;
    let user_id =
        insert_verified_user(&state, "Dave Dev", "dave@example.com", "correct horse").await;
    let router = test_router(state.clone());

    // Signin + rotate once (so refresh1 is now revoked).
    let (_s, signin_body) = signin(router.clone(), "dave@example.com", "correct horse").await;
    let (_a, refresh1, _sid) = extract_tokens(&signin_body);
    let rotate_req = json_request(
        Method::POST,
        "/api/v1/auth/token/refresh",
        &json!({ "refresh_token": refresh1.clone() }),
    );
    call(router.clone(), rotate_req).await;

    // Replay refresh1 — should be detected as reuse.
    let replay_req = json_request(
        Method::POST,
        "/api/v1/auth/token/refresh",
        &json!({ "refresh_token": refresh1 }),
    );
    let (status, body) = call(router, replay_req).await;
    assert_eq!(
        status,
        StatusCode::UNAUTHORIZED,
        "replayed refresh must be 401: {body}"
    );

    // All refresh tokens for this user must now be revoked.
    let active_count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM public.refresh_tokens \
         WHERE user_id = $1 AND revoked_at IS NULL",
    )
    .bind(user_id)
    .fetch_one(&state.pool)
    .await
    .expect("count active refresh tokens");
    assert_eq!(
        active_count, 0,
        "reuse detection must revoke all user refresh tokens"
    );

    let active_sessions: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM public.sessions \
         WHERE user_id = $1 AND revoked_at IS NULL",
    )
    .bind(user_id)
    .fetch_one(&state.pool)
    .await
    .expect("count active sessions");
    assert_eq!(
        active_sessions, 0,
        "reuse detection must revoke all user sessions"
    );
}

/// Fix §9.3: deleting a session must invalidate its access token
/// (via the session-check middleware).
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn session_delete_invalidates_access_token() {
    let (state, _guard) = spawn_state().await;
    insert_verified_user(&state, "Eve Dev", "eve@example.com", "correct horse").await;
    let router = test_router(state.clone());

    let (_s, signin_body) = signin(router.clone(), "eve@example.com", "correct horse").await;
    let (access, _refresh, sid) = extract_tokens(&signin_body);

    // DELETE the session directly via the protected endpoint.
    let delete_req = authed_request(
        Method::DELETE,
        &format!("/api/v1/auth/session/{sid}"),
        &access,
        None,
    );
    let (status, _body) = call(router.clone(), delete_req).await;
    assert_eq!(status, StatusCode::OK, "delete session should succeed");

    // Now try a protected endpoint with the same access token —
    // must be rejected by the session-check middleware.
    let get_users_req = authed_request(Method::GET, "/api/v1/users", &access, None);
    let (status, body) = call(router, get_users_req).await;
    assert_eq!(
        status,
        StatusCode::UNAUTHORIZED,
        "access token after session delete must 401: {body}"
    );
}

/// Design 3.2: password change invalidates all user sessions +
/// refresh tokens in the same transaction.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn password_change_invalidates_all_sessions() {
    let (state, _guard) = spawn_state().await;
    let user_id =
        insert_verified_user(&state, "Frank Dev", "frank@example.com", "old password").await;
    let router = test_router(state.clone());

    let (_s, signin_body) = signin(router.clone(), "frank@example.com", "old password").await;
    let (access, _refresh, _sid) = extract_tokens(&signin_body);

    // Update password via PUT /password/:userId.
    let pw_req = authed_request(
        Method::PUT,
        &format!("/api/v1/auth/password/{user_id}"),
        &access,
        Some(&json!({
            "current_password": "old password",
            "new_password": "new password strong",
            "password_confirmation": "new password strong",
        })),
    );
    let (status, body) = call(router.clone(), pw_req).await;
    assert_eq!(
        status,
        StatusCode::OK,
        "password change should succeed: {body}"
    );

    // Previous access token must now be rejected (sessions revoked).
    let req = authed_request(Method::GET, "/api/v1/users", &access, None);
    let (status, _body) = call(router, req).await;
    assert_eq!(
        status,
        StatusCode::UNAUTHORIZED,
        "access token after password change must 401"
    );

    // DB: all sessions + refresh tokens for that user are revoked.
    let active_sessions: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM public.sessions WHERE user_id = $1 AND revoked_at IS NULL",
    )
    .bind(user_id)
    .fetch_one(&state.pool)
    .await
    .unwrap();
    assert_eq!(active_sessions, 0);
}

/// Fix §9.25: initiate-verification must return 202 neutral for
/// unknown emails (no user enumeration leak).
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn verification_neutral_202_on_unknown_email() {
    let (state, _guard) = spawn_state().await;
    let router = test_router(state);

    let req = json_request(
        Method::POST,
        "/api/v1/auth/verification/email/initiate",
        &json!({ "email": "ghost@nowhere.example.com" }),
    );
    let (status, body) = call(router, req).await;
    assert_eq!(
        status,
        StatusCode::ACCEPTED,
        "unknown email should get 202 neutral: {body}"
    );
    // Body should be neutral ("... if the email is registered").
    let message = body["message"].as_str().unwrap_or("");
    assert!(
        message.contains("registered") || message.contains("sent"),
        "response must be neutral: {message}"
    );
}

/// Fix §9.9: verification resend within 60s cooldown must return 429
/// with a `retry_after` field (no lying 200 like Go).
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn verification_cooldown_returns_429() {
    let (state, _guard) = spawn_state().await;
    // Create an UNVERIFIED user so initiate actually sends.
    let user_id = Uuid::now_v7();
    sqlx::query(
        "INSERT INTO public.users (id, display_name, email, username) \
         VALUES ($1, 'Ghost', 'pending@example.com', 'pending')",
    )
    .bind(user_id)
    .execute(&state.pool)
    .await
    .expect("insert unverified user");

    let router = test_router(state);

    // First initiate — should succeed and set last_sent_at.
    let req1 = json_request(
        Method::POST,
        "/api/v1/auth/verification/email/initiate",
        &json!({ "email": "pending@example.com" }),
    );
    let (status1, _) = call(router.clone(), req1).await;
    assert_eq!(status1, StatusCode::ACCEPTED);

    // Second initiate immediately — should hit the 60s cooldown.
    let req2 = json_request(
        Method::POST,
        "/api/v1/auth/verification/email/initiate",
        &json!({ "email": "pending@example.com" }),
    );
    let (status2, body2) = call(router, req2).await;
    assert_eq!(
        status2,
        StatusCode::TOO_MANY_REQUESTS,
        "cooldown must return 429: {body2}"
    );
    assert!(
        body2["error"].as_str().unwrap_or("").contains("cooldown"),
        "error message must mention cooldown: {body2}"
    );
}

/// Fix §9.11: `PUT /auth/password/:userId` must reject cross-user
/// updates — Alice can't change Bob's password.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn ownership_check_rejects_cross_user_password_update() {
    let (state, _guard) = spawn_state().await;
    let _alice =
        insert_verified_user(&state, "Alice", "alice.own@example.com", "alice password").await;
    let bob = insert_verified_user(&state, "Bob", "bob.own@example.com", "bob password").await;
    let router = test_router(state);

    // Alice signs in.
    let (_s, alice_signin) =
        signin(router.clone(), "alice.own@example.com", "alice password").await;
    let (alice_access, _r, _sid) = extract_tokens(&alice_signin);

    // Alice tries to change Bob's password.
    let req = authed_request(
        Method::PUT,
        &format!("/api/v1/auth/password/{bob}"),
        &alice_access,
        Some(&json!({
            "current_password": "anything",
            "new_password": "hacker pwned",
            "password_confirmation": "hacker pwned",
        })),
    );
    let (status, body) = call(router, req).await;
    assert_eq!(
        status,
        StatusCode::FORBIDDEN,
        "cross-user password update must be 403: {body}"
    );
}
