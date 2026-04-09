# Multi-stage Rust build for the go-modular service (Phase D).
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

COPY apps/go-clean/Cargo.toml apps/go-clean/Cargo.toml
RUN mkdir -p apps/go-clean/src \
    && echo "fn main() {}" > apps/go-clean/src/main.rs \
    && echo "" > apps/go-clean/src/lib.rs

# Cache go-modular dependencies in a layer separate from source.
COPY apps/go-modular/Cargo.toml apps/go-modular/Cargo.toml
RUN mkdir -p apps/go-modular/src \
    && echo "fn main() {}" > apps/go-modular/src/main.rs \
    && echo "" > apps/go-modular/src/lib.rs \
    && cargo build --release -p go-modular 2>/dev/null || true

# Real source.
COPY apps/go-modular/src apps/go-modular/src
COPY apps/go-modular/migrations apps/go-modular/migrations
RUN touch apps/go-modular/src/main.rs apps/go-modular/src/lib.rs \
    && cargo build --release -p go-modular

# ---------- Stage 2: runtime ----------
FROM debian:bookworm-slim AS runtime

RUN apt-get update && apt-get install -y --no-install-recommends \
        ca-certificates \
        libssl3 \
    && rm -rf /var/lib/apt/lists/* \
    && groupadd --system --gid 1000 gomodular \
    && useradd --system --uid 1000 --gid gomodular --create-home --shell /usr/sbin/nologin gomodular

WORKDIR /opt/go-modular

COPY --from=builder /workspace/target/release/go-modular /usr/local/bin/go-modular
COPY --from=builder /workspace/apps/go-modular/migrations /opt/go-modular/migrations

USER gomodular

ARG SERVER_HOST=0.0.0.0
ARG SERVER_PORT=8000
ENV SERVER_HOST=${SERVER_HOST}
ENV SERVER_PORT=${SERVER_PORT}

EXPOSE ${SERVER_PORT}

ENTRYPOINT ["/usr/local/bin/go-modular"]
CMD ["serve"]
