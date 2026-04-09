# Multi-stage Rust build for the go-clean service.
#
# Stage 1: build the release binary inside a full Rust image.
# Stage 2: copy the binary into a minimal debian-slim runtime.
#
# Built from the monorepo root so the workspace Cargo.lock is visible
# to the build context. moon's `docker-build` task sets
# `runFromWorkspaceRoot: true`.

# ---------- Stage 1: builder ----------
FROM rust:1.90-slim-bookworm AS builder

WORKDIR /workspace

RUN apt-get update && apt-get install -y --no-install-recommends \
        pkg-config \
        libssl-dev \
        ca-certificates \
    && rm -rf /var/lib/apt/lists/*

COPY Cargo.toml Cargo.lock rust-toolchain.toml ./

# Workspace members not built in this image need stub Cargo.toml files
# so the resolver is happy without pulling them into the build.
COPY apps/templates-cli/Cargo.toml apps/templates-cli/Cargo.toml
RUN mkdir -p apps/templates-cli/src \
    && echo "fn main() {}" > apps/templates-cli/src/main.rs \
    && echo "" > apps/templates-cli/src/lib.rs

COPY apps/fastapi-ai/Cargo.toml apps/fastapi-ai/Cargo.toml
RUN mkdir -p apps/fastapi-ai/src \
    && echo "fn main() {}" > apps/fastapi-ai/src/main.rs \
    && echo "" > apps/fastapi-ai/src/lib.rs

# Cache go-clean dependencies in a layer separate from source.
COPY apps/go-clean/Cargo.toml apps/go-clean/Cargo.toml
RUN mkdir -p apps/go-clean/src \
    && echo "fn main() {}" > apps/go-clean/src/main.rs \
    && echo "" > apps/go-clean/src/lib.rs \
    && cargo build --release -p go-clean 2>/dev/null || true

# Real source.
COPY apps/go-clean/src apps/go-clean/src
COPY apps/go-clean/migrations apps/go-clean/migrations
RUN touch apps/go-clean/src/main.rs apps/go-clean/src/lib.rs \
    && cargo build --release -p go-clean

# ---------- Stage 2: runtime ----------
FROM debian:bookworm-slim AS runtime

RUN apt-get update && apt-get install -y --no-install-recommends \
        ca-certificates \
        libssl3 \
    && rm -rf /var/lib/apt/lists/* \
    && groupadd --system --gid 1000 goclean \
    && useradd --system --uid 1000 --gid goclean --create-home --shell /usr/sbin/nologin goclean

WORKDIR /opt/go-clean

COPY --from=builder /workspace/target/release/go-clean /usr/local/bin/go-clean
COPY --from=builder /workspace/apps/go-clean/migrations /opt/go-clean/migrations

USER goclean

ARG APP_HOST=0.0.0.0
ARG APP_PORT=8000
ENV APP_HOST=${APP_HOST}
ENV APP_PORT=${APP_PORT}

EXPOSE ${APP_PORT}

CMD ["/usr/local/bin/go-clean"]
