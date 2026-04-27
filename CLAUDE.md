# CLAUDE.md

Context for AI coding sessions on this repo. Read this before making
non-trivial changes — the gotchas section saves real time.

## What this is

End-to-end encrypted information hub. Personal info vault: notes,
passwords, files, tasks. Built for a small crew, currently single-user
in production.

## Stack at a glance

| | |
|---|---|
| Server | Rust 1.91 + Axum 0.7 + SQLx 0.8 + Postgres 16 |
| Frontend | SvelteKit 2 + Svelte 5 (runes) + CodeMirror 6 + Lucide |
| Crypto (client) | `libsodium-wrappers-sumo` |
| Crypto (server) | `argon2` (passphrase hash), `webauthn-rs` 0.5 |
| File storage | content-addressed sha256 on disk under `$DATA_DIR/blobs/` |
| Image | `ghcr.io/operator64/rongnote-server:latest`, built by GHA on push to main |
| Production | `notes.ronglab.de`, behind Cloudflare tunnel + Traefik |

## Repo layout

```
.
├── Cargo.toml             workspace root, single member: server
├── server/
│   ├── Cargo.toml
│   ├── migrations/        sqlx::migrate! — applied at startup, never rolled back
│   └── src/
│       ├── main.rs        wiring + AppState
│       ├── auth.rs        register/precheck/login/logout/recovery/me
│       ├── passkey.rs     WebAuthn register + discoverable login
│       ├── items.rs       CRUD for notes/secrets/tasks/files/...
│       ├── files.rs       blob upload + download
│       ├── audit.rs       record + list activity
│       ├── export.rs      tar bundle of all user data
│       ├── session.rs     cookie + sessions table + AuthUser extractor
│       ├── b64.rs         serde adapters: base64 + hex
│       ├── error.rs       AppError → IntoResponse
│       ├── config.rs      env var parsing
│       └── static_assets.rs   rust-embed of ../web/build
├── web/
│   ├── package.json
│   ├── vite.config.ts     dev proxy /api → :8080, libsodium fix plugin
│   ├── svelte.config.js   adapter-static, SPA fallback
│   └── src/
│       ├── app.html       inline FOUC-prevention script
│       ├── app.css        6-color theme, --base-font-size
│       └── lib/
│           ├── api.ts             single fetch wrapper
│           ├── crypto.ts          libsodium helpers + base32 + recovery code
│           ├── webauthn.ts        navigator.credentials wrappers + PRF
│           ├── vault.svelte.ts    master_key state + sessionStorage + idle lock
│           ├── session.svelte.ts  /me cache
│           ├── prefs.svelte.ts    theme + font, persisted to localStorage
│           ├── items.svelte.ts    items list + filters + tag/path catalogs
│           ├── totp.ts            RFC 6238 via Web Crypto
│           ├── password.ts        random PW generator
│           ├── *Editor.svelte     one per item type (Note, Secret, Task, File)
│           ├── ItemIcon.svelte    Lucide-icon-by-type
│           ├── TaskCheckbox.svelte themed checkbox (Square/SquareCheckBig)
│           ├── Sidebar.svelte
│           ├── CommandPalette.svelte
│           └── dev-seed.ts        gated on import.meta.env.DEV
├── deploy.md, SECURITY.md, README.md
└── .github/workflows/image.yml    multi-stage Docker build → ghcr.io
```

## Crypto invariants

Read [SECURITY.md](SECURITY.md) for the full scheme. Three things to never
break:

1. **Server never sees plaintext bodies, master keys, or private keys.**
   Server stores: titles, tags, paths, timestamps, item type, due dates,
   task done state, file sizes, audit-log actions, public keys, key
   *wrappings*. Never the plaintext.
2. **`master_key` is random**, generated on register. Two server-stored
   wrappings: `master_wrap_passphrase` (KEK from Argon2id of passphrase)
   and `master_wrap_recovery` (KEK from Argon2id of recovery code).
3. **`auth_hash` = BLAKE2b-keyed(master_key, "rongnote-auth-v1")** — what
   the server sees during login. Server stores Argon2id of that.

The X25519 keypair is reserved for v0.9 team-share sealing. Currently
unused at runtime; just generated on register and stored.

## Dev workflow

```bash
# Postgres in docker, server + frontend on host (fast)
docker compose up -d notes-db
cd server && cargo run
cd web && npm run dev
```

The Vite dev server (`:5173`) proxies `/api` to `:8080`, so cookies stay
same-origin from the browser's view. Hot-reload works for Svelte; the
Rust server needs manual restart.

`svelte-check`: `cd web && npm run check`. Run after every Svelte/TS
change — Svelte 5's a11y rules + rune warnings catch real bugs.

`cargo check --manifest-path server/Cargo.toml` for the server.

### Windows-specific

OpenSSL paths for `webauthn-rs`'s transitive `openssl-sys` dep live in
`.cargo/config.toml` (gitignored). Copy from
`.cargo/config.example.toml` after `winget install ShiningLight.OpenSSL.Dev`.
Linux + Docker builds get system OpenSSL via apt and ignore the file.

## Migrations

Numbered SQL files in `server/migrations/`, applied via `sqlx::migrate!()`
at startup, transactional, idempotent. **Never** roll back — to undo, write
a new forward migration. Production migrations are destructive in some
historical cases:

- 0002 nuked v0.1 plaintext bodies (TRUNCATE) — pre-1.0 only
- 0003 nuked v0.2 users (incompatible auth flow) — pre-1.0 only
- 0004 onward are non-destructive

Going forward, never TRUNCATE in a migration. Add columns, backfill,
deprecate.

## Conventions

### Comments

Default to none. Only when the *why* is non-obvious — a hidden constraint,
a workaround for a specific bug, behavior that would surprise a reader.
Don't explain *what* — well-named identifiers do that.

### Error handling

Server: `AppResult<T>` with explicit variants (NotFound, Unauthorized,
BadRequest(msg), Conflict(msg), Db, Other). `IntoResponse` does the JSON
envelope.

Client: `ApiError` with `status` + `code` + `message`. Pattern-match in
each call site.

### State

Svelte 5 runes everywhere — `$state`, `$derived`, `$effect`. No legacy
stores except the SvelteKit-provided `$page`. State classes (vault, items,
prefs, session) are singletons exported from `*.svelte.ts` files.

When seeding `$state` from a prop (editor `initial`), suppress the
`state_referenced_locally` warning with a `// svelte-ignore` comment —
the prop is read once, the effect handles re-sync on prop change.

### Crypto adapters

Use `crate::b64::{,option,hex_option}` serde modules for byte fields.
Time fields use `time::serde::rfc3339`.

## Common gotchas

These have all bit me. Don't repeat:

1. **Axum 0.7 nested routes don't match a trailing-slash request.** A
   sub-route at `/` mounted at `/notes` matches `/notes` (no slash) only,
   not `/notes/`. Frontend must call `/api/v1/items`, never `/api/v1/items/`.
2. **`time::OffsetDateTime` and `time::Date` default serde format isn't
   ISO 8601.** Without `#[serde(with = "time::serde::rfc3339")]` you get
   numeric tuples. For `Date`, use a custom format (`[year]-[month]-[day]`)
   since `Iso8601::DEFAULT` requires both date + time components.
3. **`libsodium-wrappers-sumo` ESM has a broken relative import.** The
   `vite.config.ts` plugin `fix-libsodium-relative-import` rewrites
   `./libsodium-sumo.mjs` to the sibling `libsodium-sumo` package. Don't
   remove it.
4. **libsodium uses top-level await.** `optimizeDeps.esbuildOptions.target`
   must be `es2022` minimum. Same for `build.target`.
5. **Cargo workspace target dir is at the workspace root, not the
   sub-crate.** `target/release/<bin>`, not `server/target/release/<bin>`.
   Dockerfile COPY learned this the hard way.
6. **`closeBrackets()` in CodeMirror breaks `[[` autocomplete.** It pairs
   `[` to `[]` automatically, so `[[` becomes `[[]]` and the wiki-link
   matchBefore regex fails. Don't add it back.
7. **Browser `<input type="checkbox">` doesn't theme well across Win/Mac.**
   Use `TaskCheckbox.svelte` (Lucide Square / SquareCheckBig) for any
   user-facing checkbox.
8. **register screen's recovery code disappears if `session.setUser()`
   fires too early.** Top-level layout's auth-redirect bumps logged-in
   users off `/register`. Hold the user view in `pendingUser` and only
   call `session.setUser` when the user clicks "continue".
9. **`/recovery` must be in `ALWAYS_ALLOW_ROUTES`.** Otherwise logged-in
   users testing the recovery flow get redirected to `/items`.
10. **Vite dev server doesn't restart on `vite.config.ts` changes.** Kill
    + restart manually after editing the config.

## Build + push image (CI)

`.github/workflows/image.yml` runs on every push to `main` and pushes to
`ghcr.io/operator64/rongnote-server`. It uses Docker buildx with GHA
cache. The image is **public** (intentional, source repo is private).

To deploy a fresh build:

```bash
ssh ronglab "cd /opt/notes && docker compose pull notes && docker compose up -d notes"
```

## Demo data

`web/src/lib/dev-seed.ts` is a Cmd-K action `seed demo data`, gated on
`import.meta.env.DEV`. Tree-shaken out of prod builds. Generates 6 notes
+ 6 secrets + tags spread across paths. Idempotent (skips existing
titles).

## Things to NOT do

- Don't add Cargo.lock to `.gitignore` — apps need reproducible builds.
- Don't add `.cargo/config.toml` to git — Windows-specific.
- Don't `TRUNCATE` in a migration.
- Don't add ItemView fields without updating SELECT/INSERT/RETURNING in
  every items.rs query (5+ places).
- Don't store secrets in Postgres logs — `RUST_LOG=sqlx=warn` is the
  default and intentional.
- Don't bundle marked content with `{@html}` from server-rendered text.
  Wiki-link preprocessing produces markdown that goes through `marked`
  client-side; the inputs are decrypted on the client and not from the
  server's database.
