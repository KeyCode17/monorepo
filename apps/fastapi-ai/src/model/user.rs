//! User model.
//!
//! Mirrors `app/model/user.py::User`. The Python service uses
//! `SQLAlchemy` declarative classes; we use a plain `sqlx::FromRow`
//! struct because we don't need an ORM — the only place users are
//! touched in the current API surface is via raw queries.
//!
//! Schema (verified against `apps/fastapi-ai/migrations/`):
//! ```sql
//! CREATE TABLE users (
//!     id            SERIAL PRIMARY KEY,
//!     full_name     VARCHAR NOT NULL,
//!     username      VARCHAR NOT NULL UNIQUE,
//!     phone_number  VARCHAR NOT NULL,
//!     email         VARCHAR NOT NULL UNIQUE,
//!     password_hash VARCHAR NOT NULL
//! );
//! CREATE INDEX ix_users_username ON users (username);
//! CREATE INDEX ix_users_email    ON users (email);
//! ```

use serde::{Deserialize, Serialize};
use sqlx::FromRow;

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct User {
    pub id: i32,
    pub full_name: String,
    pub username: String,
    pub phone_number: String,
    pub email: String,
    pub password_hash: String,
}
