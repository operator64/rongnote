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
| Frontend | SvelteKit 2 + Svelte 5 (runes) + CodeMirror 6 + Lucide + svelte-dnd-action |
| Crypto (client) | `libsodium-wrappers-sumo` |
| Crypto (server) | `argon2` (passphrase hash), `webauthn-rs` 0.5 |
| File storage | content-addressed sha256 on disk under `$DATA_DIR/blobs/` |
| Image | `ghcr.io/operator64/rongnote-server:latest`, built by GHA on push to main |
| Production | `notes.ronglab.de`, behind Cloudflare tunnel + Traefik |

## Repo layout

```
.
├── Cargo.toml             workspace root, members: server + cli
├── cli/
│   ├── Cargo.toml
│   └── src/
│       ├── main.rs        clap subcommands
│       ├── api.rs         typed HTTP client
│       ├── crypto.rs      mirrors web/src/lib/crypto.ts (RustCrypto)
│       └── config.rs      ~/.config/rongnote/session.json
├── server/
│   ├── Cargo.toml         includes `reqwest` (rustls-tls) for the transit proxy
│   ├── migrations/        sqlx::migrate! — applied at startup, never rolled back
│   └── src/
│       ├── main.rs        wiring + AppState + /api/v1/config + CORS layer
│       │                  + reqwest::Client + transit_cache in AppState
│       ├── auth.rs        register/precheck/login/logout/recovery/me
│       │                  (gated by config.registration_open)
│       ├── passkey.rs     WebAuthn register + discoverable login + list/delete
│       ├── items.rs       CRUD for notes/secrets/tasks/lists/files/snippets/
│       │                  bookmarks/events + /:id/move + version snapshots on
│       │                  body update + assert_can_modify helper (kiosk gate)
│       ├── spaces.rs      team spaces + members + atomic invite re-wrap
│       ├── shares.rs      /share/<token> public + /<token>/blob for files
│       ├── files.rs       blob upload + download
│       ├── audit.rs       record + list activity
│       ├── export.rs      tar bundle of all user data
│       ├── transit.rs     server-side VRR EFA proxy (browser can't reach EFA
│       │                  directly — no CORS). /departures + /nearby with a
│       │                  30 s per-stop in-memory cache.
│       ├── session.rs     cookie + sessions table + AuthUser extractor
│       ├── b64.rs         serde adapters: base64 + hex + date_iso (YYYY-MM-DD)
│       ├── error.rs       AppError → IntoResponse (+ BadGateway = 502 for
│       │                  upstream proxy failures)
│       ├── config.rs      env var parsing (DATABASE_URL, REGISTRATION_OPEN, …)
│       └── static_assets.rs   rust-embed of ../web/build
├── web/
│   ├── package.json       includes sharp devDep for `npm run build:icons`
│   ├── vite.config.ts     dev proxy /api → :8080, libsodium fix plugin,
│   │                       experimentalMinChunkSize: 50_000 (see gotcha 22)
│   ├── svelte.config.js   adapter-static, SPA fallback
│   ├── scripts/build-icons.mjs   renders static/app-icon.svg → PNG PWA icons
│   ├── static/
│   │   ├── favicon.svg           light/dark via prefers-color-scheme inside SVG
│   │   ├── app-icon.svg          source for the PWA icons
│   │   ├── icons/{icon-192,icon-512,apple-touch-icon}.png   generated
│   │   └── manifest.webmanifest  PWA manifest, start_url=/dashboard
│   └── src/
│       ├── app.html       inline FOUC-prevention script + PWA meta tags
│       │                   (manifest link, apple-touch-icon, apple-mobile-*)
│       │                   + inline window.__rongnote_lastError capture
│       ├── app.css        6-color theme, --base-font-size,
│       │                   .list-row { flex-shrink: 0 } so big lists scroll
│       ├── hooks.client.ts       handleError → stash real error on
│       │                          window.__rongnote_lastError before SvelteKit
│       │                          normalises it to "Internal Error"
│       └── lib/
│           ├── api.ts             single fetch wrapper
│           ├── crypto.ts          libsodium helpers + base32 + recovery code
│           ├── itemCrypto.ts      personal vs team-space wrap/unwrap helpers
│           ├── csvImport.ts       Firefox/Chrome/Bitwarden/1Password CSV → secrets
│           ├── webauthn.ts        navigator.credentials wrappers + PRF
│           ├── vault.svelte.ts    master_key state + sessionStorage + idle lock
│           ├── session.svelte.ts  /me cache
│           ├── spaces.svelte.ts   active space + member list cache
│           ├── prefs.svelte.ts    theme + font, persisted to localStorage
│           ├── items.svelte.ts    items list + filters + tag/path catalogs +
│           │                       SvelteMap decryptedNoteBodies (avoids effect loops)
│           ├── totp.ts            RFC 6238 via Web Crypto
│           ├── password.ts        random PW generator
│           ├── *Editor.svelte     one per type: Note, Secret, Task, List,
│           │                       Event, Snippet, Bookmark, File
│           ├── PasswordGenerator.svelte  inline popover in SecretEditor
│           ├── hibp.ts            k-anonymity SHA-1 prefix query
│           ├── ItemIcon.svelte    Lucide-icon-by-type
│           ├── TaskCheckbox.svelte themed checkbox (Square/SquareCheckBig)
│           ├── Sidebar.svelte
│           ├── CommandPalette.svelte
│           ├── dashboardSettings.svelte.ts   per-device localStorage (selected
│           │                                  list, lat/lon, stop IDs, walk
│           │                                  minutes) for /dashboard widgets
│           ├── dashboard/                    always-live dashboard widgets
│           │   ├── Widget.svelte             shared chrome (title/meta/actions)
│           │   ├── CalendarWidget.svelte     week-strip + agenda + +event modal
│           │   ├── ListWidget.svelte         pinned-list dropdown + inline toggle
│           │   ├── TasksWidget.svelte        open/done tasks + tap→TasksModal
│           │   ├── TasksModal.svelte         bigger tasks view (rename/delete/add)
│           │   ├── WeatherWidget.svelte      open-meteo current + 4-day forecast
│           │   ├── ClockWidget.svelte        big HH:MM + date + ISO week
│           │   ├── TransitWidget.svelte      2 stops via /api/v1/transit
│           │   ├── TransitStopModal.svelte   single-stop bigger view
│           │   └── SettingsModal.svelte      GPS + stop IDs + walk minutes
│           └── dev-seed.ts        gated on import.meta.env.DEV
├── web/src/routes/
│   ├── +layout.svelte     top-level auth gate + spaces bootstrap; kiosk-only
│   │                       users get bounced from /items* → /dashboard here.
│   │                       NO +layout.ts (see gotcha 22).
│   ├── +error.svelte      diagnostic error page — surfaces the real error
│   │                       message + captured stack from window.__rongnote_lastError
│   ├── dashboard/+page.svelte    standalone /dashboard route (top-level, no
│   │                              items chrome). Nested-split 2x2 grid: cell
│   │                              1 = calendar, cell 2 = list|tasks (2 cols),
│   │                              cell 3 = weather/clock (2 rows), cell 4 =
│   │                              transit. Pauses vault idle-lock on mount.
│   └── items/             regular items chrome (sidebar + list pane)
├── extension/                 Firefox/Chrome MV3 popup, separate npm + esbuild build
│   ├── package.json
│   ├── build.mjs              bundles src/{popup,options,background}.ts → dist/
│   └── src/
│       ├── manifest.json      MV3, "wasm-unsafe-eval" CSP for libsodium
│       ├── popup.html / popup.ts
│       ├── options.html / options.ts
│       ├── background.ts      idle-lock alarm
│       └── lib/{crypto,api,items,store,totp}.ts
├── landing/index.html         single static page → rongnote.ronglab.de via nginx
├── docker-compose.example.yml minimal public-consumption compose
├── deploy.md, SECURITY.md, README.md
└── .github/                   issue templates + workflows/image.yml
                               (multi-stage Docker build → ghcr.io)
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

The X25519 keypair (generated on register, wrapped private in
`encrypted_private_key`) wraps per-item keys for each member of a team
space (`crypto_box_seal` → `item_member_keys`). Personal-space items
keep using `master_key` secretbox wraps. Server returns whichever wrap
the caller can use in `item.wrapped_item_key`, with
`key_wrap='master'|'sealed'` as the discriminator. See
`web/src/lib/itemCrypto.ts` for the single decision point — every
editor goes through `encryptBodyForSpace` / `decryptItemBody`.

Item-key rotation differs by space: personal rotates on every save, team
**reuses** the existing key (otherwise version snapshots become
undecryptable, since `item_member_keys` only stores the *current* wrap).

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

- 0001 — initial schema
- 0002 — e2e crypto (TRUNCATE'd v0.1 data) — pre-1.0 only
- 0003 — recovery code refactor (TRUNCATE'd v0.2 data) — pre-1.0 only
- 0004 — trash (`items.deleted_at`)
- 0005 — passkeys table
- 0006 — files (`files_blobs` + `items.blob_sha256`)
- 0007 — tasks (`items.due_at`, `items.done`)
- 0008 — audit log
- 0009 — pinned (`items.pinned`)
- 0010 — share_links table
- 0011 — item_versions table
- 0012 — extend `items.type` CHECK to include `'list'`
- 0013 — `item_member_keys` table for sealed-box per-member wraps in team spaces
- 0014 — events: `items.start_at`, `end_at`, `all_day` + partial index on
         `(space_id, start_at)` for cheap calendar-range queries
- 0015 — kiosk role: extend `memberships.role` CHECK to include `'kiosk'`.
         Kiosk is a between-viewer-and-editor role — full read; can CREATE
         items; can UPDATE items they created OR of type `'list'`/`'task'`;
         cannot delete or move. Intended for always-on wall displays
         (`/dashboard`) where any household member should be able to tick
         off a shared list / task without giving them destructive access.

Going forward, never TRUNCATE in a migration. Add columns, backfill,
deprecate. **Use `--` for SQL comments**, not Rust-style `///` — the latter
fails parsing.

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
   numeric tuples. For `Date`, use `crate::b64::date_iso_option`
   (custom `[year]-[month]-[day]` format) — `Iso8601::DEFAULT` requires both
   date + time components and won't compile with `Date`.
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
   users testing the recovery flow get redirected to `/items`. `/share/*`
   is also auth-bypass via `isPublicPrefix(path)`.
10. **Vite dev server doesn't restart on `vite.config.ts` changes.** Kill
    + restart manually after editing the config.
11. **Editor "sync from store" effects can clobber unsaved local edits.**
    The TaskEditor's effect that copies `items.list` summary back into
    local state used to fire on every local change (`dueAt` was both read
    and written) and reverted to the stale store value. Gate the sync on
    `!saving && !dirty` so it only runs when the user is idle.
12. **HTML `<input type="date">` shows the OS placeholder ("TT.MM.JJJJ"
    in DE locale) when value is the empty string.** It always commits
    YYYY-MM-DD to the bound `$state` on commit, regardless of locale.
    Don't try to localize.
13. **PostgreSQL CHECK constraints can't be `ALTER`-ed in place.** To
    extend an enum-style CHECK (e.g. add `'list'` to `items.type`), drop
    + recreate it. See `0012_list_type.sql`.
14. **SQL comments must use `--`, not `///`.** sqlx's migrator passes the
    raw SQL to Postgres; `///` is a Rust thing and fails to parse.
15. **Mobile sidebar drawer:** the `items/+layout.svelte` wraps the
    `<Sidebar>` in a `.sidebar-wrap` that becomes `position: fixed` at
    `<700px`. The `drawerOpen` state is reset on every filter or
    navigation change via a `$effect` so taps in the drawer auto-close it.
16. **Cmd-K palette can be opened programmatically** by dispatching a
    synthetic `KeyboardEvent('keydown', { key: 'k', ctrlKey: true })` on
    `window` — the search button in the mobile pane head does exactly
    this since touch users don't have a keyboard.
17. **`$effect` self-loops via spread-and-assign on `$state` objects.**
    `setDecryptedBody` originally did `this.bodies = { ...this.bodies, [id]: body }`.
    Called from a NoteEditor `$effect`, the spread READ `bodies` (tracked
    dep) and the assign WROTE it (re-trigger) → `effect_update_depth_exceeded`.
    Two layered fixes: `untrack(...)` at the call site AND switch the
    underlying state to `SvelteMap` (fine-grained per-key reactivity).
    Apply both for any read-and-write inside an effect.
18. **Flex children shrink by default.** A vertical flex container with
    `overflow-y: auto` looks correct until total content exceeds the
    viewport — without `flex-shrink: 0` on each row, hundreds of items
    proportionally shrink to overlap, and overflow-y never engages.
    `.list-row` and `.task-row` need it explicitly.
19. **`mouseenter` fires when scroll changes which element is under a
    stationary pointer.** Cmd-K's arrow-key navigation triggers
    `scrollIntoView`; using `mouseenter` to highlight rows then re-set
    the cursor mid-keyboard-nav. Use `mousemove` instead — it only fires
    on actual pointer movement.
20. **Browser extension MV3 needs `'wasm-unsafe-eval'`** in CSP for
    libsodium to load. Default MV3 CSP blocks WASM compilation. Set
    `content_security_policy.extension_pages: "script-src 'self'
    'wasm-unsafe-eval'; object-src 'self'"` in `manifest.json`.
21. **Server CORS must let `moz-extension://` and `chrome-extension://`
    origins through with `credentials`** — the popup is a different
    origin from the API host, and the session cookie is SameSite=Lax.
    Firefox WebExtensions with `host_permissions` declared still send
    the cookie cross-origin if the server's `Access-Control-Allow-Origin`
    matches the extension's origin. See `cors_layer` in `server/src/main.rs`.
22. **Do not add `src/routes/+layout.ts`** — SvelteKit's autogenerated
    node file then contains `import * as universal from '../+layout.ts';
    export { universal }`. Vite/Rollup chunking sometimes places that
    binding on the wrong side of a module-eval cycle; V8 (desktop
    Chrome/Firefox/Safari-on-mac) tolerates the cycle by chance, JSC
    (iOS Safari) follows the spec strictly and TDZs on every reload
    with `Cannot access 'universal' before initialization`. Manifests
    as SvelteKit's default "500 / Internal Error" page on iOS only.
    adapter-static + `fallback: 'index.html'` handles SPA behaviour
    without an ssr=false option, so the file was pure liability.
    `vite.config.ts` also sets `experimentalMinChunkSize: 50_000` as
    defense-in-depth. If you need the same reload-diagnostic on a
    similar bug: check `/+error.svelte` + `hooks.client.ts` — they
    were built for exactly this and stayed after the fix.
23. **First body-save on a team-space item needs `member_keys` on the
    UPDATE, not just on the CREATE.** The `+` button in the sidebar
    creates an empty item (no body → no wrap yet), then the editor's
    first `saveNow` supplies both the encrypted body AND the per-member
    sealed wraps. `items.rs::update` allows `member_keys` iff
    `existing.encrypted_body_bytes().is_none()` and inserts them into
    `item_member_keys` before commit. Subsequent saves reuse the
    existing item_key + rows (that's what keeps version snapshots
    decryptable), so `member_keys` is forbidden then.
24. **Every editor / widget that saves a body must go through
    `encryptBodyForSpace`, not a hand-rolled `wrapItemKey`.** The
    dashboard `ListWidget` originally generated a fresh item_key on
    every save and shipped `member_keys` — server rejected with
    "team-space body update reuses existing member keys". The helper
    branches on `spaceId` kind + presence of `item.wrapped_item_key`:
    personal rotates, team-with-body reuses, team-first-body wraps.
    Anything else drifts.
25. **`spaces.refresh()` picks a default active space at first login,
    and for kiosk-only users that default must be the team space.**
    Every user gets a personal space at register time for crypto
    plumbing (their keypair lives there), but for a kiosk it stays
    empty forever. Defaulting activeId to personal would leave the
    dashboard staring at an empty items.list. See
    `spaces.svelte.ts::defaultActive` — team-space-first for kiosk,
    personal for everyone else.
26. **VRR EFA has no CORS.** Direct browser fetches to
    `efa.vrr.de/standard/*` are blocked — we proxy through
    `server/src/transit.rs`. `/api/v1/transit/departures?stop_id=…`
    and `/nearby?lat=…&lon=…`. Auth-gated so we're not running a
    free CORS proxy for VRR. 30s per-stop cache keeps EFA from being
    hit more than every half-minute even when multiple kiosks poll.
    Stop IDs are the VRR format (e.g. `20018235` for Düsseldorf Hbf),
    NOT db-rest/HAFAS IBNRs — `dashboardSettings.load()` auto-drops
    legacy `8\d{6,7}` IDs so the user re-runs "find nearest".

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

## CLI

`cli/` is a workspace member that builds a `rongnote` binary. Same E2E
crypto as the browser: same Argon2id-INTERACTIVE params (mem=64 MiB,
ops=2), same XSalsa20-Poly1305 secretbox layout (`nonce || ct || mac`),
same sealed-box construction (`eph_pk || box(nonce=BLAKE2b(eph_pk||rec_pk),
recipient_pk, eph_sk)`).

Don't depend on the `seal` feature of `crypto_box` — its API path moves
between minor versions. We hand-roll sealed-box on top of `SalsaBox`
(which is XSalsa20-Poly1305 over X25519 ECDH = libsodium's
`crypto_box_easy`). Tests in `cli/src/crypto.rs` round-trip everything.

Session cache (cookie + unwrapped master_key + privkey) lives at
`directories::ProjectDirs::from("", "rongnote", "rongnote")`. Set
`RONGNOTE_NO_PERSIST=1` to bypass.

## Browser extension

`extension/` is a Firefox/Chrome MV3 popup, separate from the workspace
(its own `package.json` + `node build.mjs` esbuild bundling, no Cargo
involvement). Loads via `about:debugging` → Load Temporary Add-on → pick
`extension/dist/manifest.json`.

Same crypto stack as the SPA — bundles `libsodium-wrappers-sumo` with
the same upstream-packaging-bug workaround as `web/vite.config.ts`
(esbuild plugin redirects the broken relative import). See
`extension/build.mjs`.

The popup runs its own login (passphrase → Argon2id KEK → unwrap
`master_key`) and stores the result in `browser.storage.session`,
which clears on browser close. An idle-lock alarm (15 min) clears it
sooner. Decrypted secret payloads are also cached in session storage
keyed by item id + `updated_at` so subsequent opens are instant; the
first cold load fetches and decrypts in parallel batches of 16.

Don't try to plumb auth through the SPA's tab — extension and SPA are
separate origins, and Firefox WebExtensions with `host_permissions`
make their own credentialed fetches anyway. Just keep them
independent.

## Calendar

`event` is a regular item type. The thing the calendar needs that other
items don't: queryable time fields. Migration 0014 added `start_at`,
`end_at` (timestamptz, UTC) and `all_day` (bool). They're plaintext on
the server — same trade-off as titles/tags/timestamps — so the
calendar view can ask "events between X and Y" without decrypting
every body.

`items.start_at` storage:
- Timed event: the actual start time, UTC
- All-day event: midnight UTC of the start day, end_at is **next** midnight
  UTC (iCal DTEND exclusive-end convention). The EventEditor displays the
  *inclusive* last day in the date picker and converts back on save.

`/api/v1/items?type=event&start_after=...&start_before=...` filters by
the partial index `(space_id, start_at)`. Useful only with a space
filter — without it, the index isn't used.

`/items/calendar` does N parallel listItems calls, one per space the
user is a member of, then unions + colour-codes per space. Personal is
always accent-blue; team spaces cycle through TEAM_COLORS ordered by
`created_at` so the same team keeps its colour across reloads.

The page's `$effect` tracks `items.list` so any mutation through
another route (editor save, sidebar +, palette) repaints the calendar
without a navigate-away-and-back. The server fetch itself doesn't
touch items.list, so no loop.

## Dashboard + kiosk

`/dashboard` is a standalone top-level route (NOT under `/items`), so
it renders without the sidebar / item-list chrome. Purpose: an
always-on wall display (iPad on the kitchen wall).

- Nested-split 2×2 grid: calendar · list+tasks (2 cols) · weather/clock
  (2 rows) · transit. Widgets live under `web/src/lib/dashboard/`.
- Vault idle-lock pauses on mount, resumes on destroy (via
  `vault.pauseIdle()` / `resumeIdle()` — refcount-stacked so multiple
  callers work).
- Each panel is tap-to-open-modal: `TransitStopModal`, `TasksModal`,
  and the existing list-edit modal. Bigger fonts, more entries.
  Inline elements (checkboxes) stopPropagation to keep their toggle
  behaviour.

**Kiosk role**: `memberships.role = 'kiosk'` (migration 0015). Server
gate is `assert_can_modify` in `items.rs`. Kiosk-only users:

- `spaces.svelte.ts::isKioskOnly` — every team membership is `'kiosk'`.
- Default active space becomes the first team (not the empty personal).
- Post-login redirect goes to `/dashboard` (top-level layout).
- Dashboard hides the "← items" and "🔒 lock" buttons for them.

Kiosk users register normally (they need their own keypair for the
sealed-box wraps). An owner then invites them into the team space
with role='kiosk'. `REGISTRATION_OPEN=false` locks the door again.

## PWA

`/dashboard` installs as a chromeless PWA — the whole point on iPad
Safari where "Add to Home Screen" launches without an address bar.

- `web/static/manifest.webmanifest` — `start_url=/dashboard`,
  `display=standalone`, icons at `/icons/{192,512,apple-touch-icon}.png`.
- `web/static/app-icon.svg` is the source; `npm run build:icons` renders
  the PNGs via `sharp` (devDep). Icons are committed — don't regenerate
  on every build.
- iOS-specific `apple-mobile-web-app-*` meta tags in `app.html`.
  `apple-mobile-web-app-status-bar-style` is `default` (opaque bar
  above the dashboard) not `black-translucent` (which would overlay
  the header). No service worker — not needed for iOS home-screen
  install, and adding one would just add offline complexity we don't
  have a use case for.
- `hooks.client.ts` + `+error.svelte` are diagnostic scaffolding for
  the class of bug that hit us during the PWA rollout (see gotcha 22).
  Keep them — surfacing the real underlying error is worth the ~30 LOC.

## Transit (VRR)

Server-side proxy at `/api/v1/transit/*` fetches Düsseldorf public
transport from `efa.vrr.de` (canonical source for VRR / Rheinbahn /
Stadtwerke; db-rest is unreliable and doesn't cover local transit
consistently).

- `departures?stop_id=<vrr_id>&limit=N` — up to 30 departures per stop.
- `nearby?lat=<f>&lon=<f>&limit=N` — up to 20 nearby stops by radius.
- Auth-gated (session cookie). 30 s in-memory cache per unique query.
- Response shape is the same as the SPA was already consuming from
  db-rest, so switching client-side was a one-liner: `api.transitDepartures`.

Stop IDs are VRR format (8 digits starting with a region prefix,
e.g. `20018235` for Düsseldorf Hbf, `20018224` for Engerstraße).

Walk-time (per stop, in `dashboardSettings.walk_minutes`) hides
departures the rider can't catch — the widget's minutes column
becomes "leave-by countdown" when a walk-time is set.

## CSV import

`/items/import` reads exported password CSVs (Firefox / Chrome /
Bitwarden / 1Password / KeePass) and creates one secret per row.
Header detection in `web/src/lib/csvImport.ts` — extend the candidate
lists in `find(...)` if a new format shows up.

Dedup is two-stage:
1. Parse-time, in-batch — multiple CSV rows with the same
   `(host, username)` become one (Firefox stores http+https variants
   of the same login as separate rows).
2. Pre-import scan — every existing secret in the active space gets
   fetched + decrypted to build a `(host, username)` set; CSV rows
   matching anything in that set are skipped. Lets re-imports be
   idempotent without title-uniqueness assumptions.

Imported items land at `/imported/YYYY-MM-DD` with tag `imported`.

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
