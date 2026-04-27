# rongnote

Self-hosted information hub: notes, secrets, keys, calendar, files. Multi-user,
team and personal spaces, monospace UI.

This is **v0.1** — auth + notes + the deploy pipeline. Secrets, keys, calendar,
files, E2E crypto, WebAuthn all come in later milestones (see `ROADMAP` below).

## Stack

- **Backend:** Rust, Axum, SQLx (Postgres), Argon2id for passwords.
- **Frontend:** SvelteKit (SPA mode), TypeScript, CodeMirror 6.
- **Database:** Postgres 16.
- **Deploy:** single Docker image with the SvelteKit build embedded into the Rust binary.

## Dev setup

Prereqs: Docker, Node 22, and (for fast iteration) Rust via `rustup`.
Without Rust locally, you can still run the whole stack via Docker — it'll just
be slower to iterate on the server.

```bash
cp .env.example .env
# edit NOTES_DB_PW

# Option A: everything in Docker (slow rebuilds)
docker compose up --build

# Option B: Postgres in Docker, server + frontend on host (fast)
docker compose up -d notes-db
cd server && cargo run               # http://localhost:8080
cd web && npm install && npm run dev # http://localhost:5173, proxies /api -> :8080
```

The frontend dev server proxies `/api/*` to `localhost:8080` (see `web/vite.config.ts`).

## Health check

```
GET /healthz  -> 200 OK
```

## ROADMAP

- **v0.1** (this milestone): auth, notes, single personal space, deploy pipeline.
- **v0.2:** client-side crypto for note bodies (libsodium). Per-user X25519 keypair,
  per-item key wrapping. Server stops seeing note content.
- **v0.3:** WebAuthn / Passkeys, TOTP fallback.
- **v0.4:** Secrets type (passwords + TOTP generator + breach check).
- **v0.5:** Keys type (SSH, API tokens, certs).
- **v0.6:** Files (upload, dedup, attachments).
- **v0.7:** Calendar + CalDAV endpoint.
- **v0.8:** Team spaces + sharing (sealed-box wrap per member).
- **v0.9:** Search (Postgres FTS), tags, tree, wiki-links, backlinks.
- **v1.0:** Audit log, recovery codes, encrypted export, polish.

## Layout

```
.
├── server/            Rust backend (Axum)
│   ├── src/
│   ├── migrations/    SQL migrations applied at startup
│   └── static/        SvelteKit build output, embedded into the binary (gitignored)
├── web/               SvelteKit frontend
│   └── src/
├── docker-compose.yml Local dev stack
├── Dockerfile         Production image (multi-stage)
└── deploy.md          Production deploy notes (TBD)
```
