-- Translated from the original alembic migration:
-- apps/fastapi-ai/app/database/migrations/versions/989eb3ef44de_create_users_table.py
--
-- Schema preserved exactly: column order, types, nullability,
-- uniqueness, and the two B-tree indexes on username and email.

CREATE TABLE IF NOT EXISTS users (
    id            SERIAL PRIMARY KEY,
    full_name     VARCHAR NOT NULL,
    username      VARCHAR NOT NULL UNIQUE,
    phone_number  VARCHAR NOT NULL,
    email         VARCHAR NOT NULL UNIQUE,
    password_hash VARCHAR NOT NULL
);

CREATE INDEX IF NOT EXISTS ix_users_username ON users (username);
CREATE INDEX IF NOT EXISTS ix_users_email    ON users (email);
