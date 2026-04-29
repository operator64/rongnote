# Multi-stage: build the SvelteKit static SPA, build the Rust server with the
# SPA embedded via rust-embed, ship a small runtime image.

# ---- Frontend build ----
FROM node:22-alpine AS web-builder
WORKDIR /web
COPY web/package.json web/package-lock.json* ./
RUN --mount=type=cache,target=/root/.npm \
    if [ -f package-lock.json ]; then npm ci; else npm install; fi
COPY web/ ./
RUN npm run build

# ---- Rust build ----
FROM rust:1.91-slim-bookworm AS server-builder
WORKDIR /src
RUN apt-get update && apt-get install -y --no-install-recommends pkg-config libssl-dev ca-certificates && rm -rf /var/lib/apt/lists/*

# Cache deps: copy manifests first. The workspace lists `cli` as a member,
# so cargo needs cli/Cargo.toml present even when we only build the server
# package. Stub cli's main.rs so the resolver doesn't choke.
COPY Cargo.toml Cargo.lock* ./
COPY server/Cargo.toml server/Cargo.toml
COPY cli/Cargo.toml cli/Cargo.toml
RUN mkdir -p server/src cli/src && \
    echo "fn main() {}" > server/src/main.rs && \
    echo "fn main() {}" > cli/src/main.rs && \
    cargo build --release -p rongnote-server && \
    rm -rf server/src

# Real source + frontend assets (cli stays stubbed — image is server-only).
COPY server/ server/
COPY --from=web-builder /web/build /src/server/static

RUN touch server/src/main.rs && cargo build --release -p rongnote-server

# ---- Runtime ----
FROM debian:bookworm-slim AS runtime
RUN apt-get update && apt-get install -y --no-install-recommends ca-certificates && rm -rf /var/lib/apt/lists/* && \
    useradd -u 10001 -r -s /usr/sbin/nologin notes && \
    mkdir -p /data && chown notes:notes /data
WORKDIR /app
COPY --from=server-builder /src/target/release/rongnote-server /usr/local/bin/rongnote-server
USER notes
EXPOSE 8080
ENTRYPOINT ["/usr/local/bin/rongnote-server"]
