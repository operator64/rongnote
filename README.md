# rongnote

Self-hosted, end-to-end encrypted information hub. One tab to find your
notes, passwords, files, tasks, lists, snippets, and bookmarks. Built for
a small crew or just yourself.

> **[rongnote.ronglab.de](https://rongnote.ronglab.de)** · landing + features
> **[notes.ronglab.de](https://notes.ronglab.de)** · live instance (single-user)

## Features

### Item types

- **Notes** — Markdown, live preview, CodeMirror 6, `[[wiki-links]]` with
  autocomplete + broken-link rendering, backlinks panel.
- **Tasks** — checkboxes, due dates with overdue/today/future colour coding,
  one-click toggle from the list, sorted open-first.
- **Lists** — checklists with drag-reorder, Enter to add, Backspace to remove,
  per-item checkboxes, "clear N done" sweep. Shopping lists, packing lists.
- **Secrets** — passwords, TOTP codes (live with countdown), generator with
  charset toggles, copy-with-30s-clipboard-clear, reveal-with-10s-auto-hide,
  HIBP breach check (k-anonymity prefix query, never sends the password).
- **Snippets** — code blocks with language tag, monospace editor, copy button.
- **Bookmarks** — URL + tags + notes, "open ↗" link.
- **Files** — drag & drop upload, encrypted on the client, content-addressed
  blobs on disk, inline preview for images / PDF / text. 50 MB cap.

### Cross-cutting

- **Tags + path tree + search** — sidebar with type / path / tag filters,
  Cmd-K command palette for everything.
- **Pinned items** — rise to the top of the list. Per-item toggle.
- **Daily notes** — one-click "today's daily note" auto-creates
  `Daily YYYY-MM-DD` at `/journal/YYYY-MM`.
- **Templates** — any note in `/_templates/*` becomes a "new from template"
  Cmd-K action; new note inherits the template's body and tags.
- **Wiki-links** — `[[note title]]` and `[[note|alias]]` syntax. Autocomplete
  on `[[`. Backlinks panel below the editor (cache fills as you navigate;
  "scan all" Cmd-K builds the full graph).
- **Trash** — soft-delete with restore. Hard-delete clears blob refcount and
  removes orphans.
- **Version history** — every body-save is a snapshot. Browse, preview,
  restore. Restore itself is versioned (reversible).
- **Share via link** — `/share/<token>#<key>`. Notes and files. Server stores
  re-encrypted ciphertext (notes) or the existing item-key wrapped under the
  share key (files); the share key lives in the URL fragment, never reaches
  the server. Per-link expire + revoke.
- **Team spaces** — invite editors / viewers; per-item key sealed once per
  member with libsodium `crypto_box_seal`. Atomic invite re-wrap. Move items
  between spaces from Cmd-K.
- **CSV import** — `/items/import` reads exports from Firefox, Chrome,
  Bitwarden, 1Password, KeePass; auto-detects the header shape. Each row
  becomes an encrypted secret in the active space. Smart dedup compares
  `(host, username)` against decrypted existing secrets, so re-imports
  don't double up.
- **Audit log** — every secret read, item write, share creation, and auth
  event recorded. Per-user view at `/items/audit`.
- **Encrypted backup** — one-click `.tar` export with all your encrypted data.
  Pipe through `age -p` for full at-rest privacy.
- **Closeable signups** — `REGISTRATION_OPEN=false` env flag locks
  `/register` after you've created your account. Existing users can still
  sign in.

### Auth

- Passphrase + recovery code (Argon2id, 24 chars base32 shown once).
- **WebAuthn / Passkey** with PRF — sign in + unlock vault in one tap.
  Multiple passkeys per account; manage at `/items/passkeys`. Requires a
  PRF-capable authenticator (Yubikey 5+, Touch ID via iPhone/iPad, recent
  Windows Hello, 1Password / Bitwarden).
- Auto-lock after 15 min idle. Unlock prompt offers passphrase OR passkey.
- Sessions in HttpOnly Lax cookies, server-side store with TTL.

### UI

- Monospace, six-colour theme (light/dark/auto), adjustable font size,
  persistent in `localStorage`.
- Cmd-K command palette: `new note/task/list/secret/snippet/bookmark`,
  `upload file`, `today's daily note`, `manage spaces`, `new team space`,
  `move current item to space…`, `share current item via link`,
  `import secrets from CSV`, `manage passkeys`, `audit log`,
  `export backup`, `version history`, theme/font controls. Arrow-key
  navigation scrolls active row into view.
- **Mobile** (`<700px`): stack-mode (list OR detail, never both); sidebar
  becomes a slide-in drawer; hamburger + search buttons in the pane head;
  status bar drops non-essential controls.

### CLI

A separate `rongnote` binary at [`cli/`](cli/) for headless workflows — same
E2E crypto as the browser, same `/api/v1/*` API.

```bash
cargo build --release -p rongnote-cli
./target/release/rongnote login                # email + passphrase
./target/release/rongnote ls --type=note
./target/release/rongnote cat <id>
echo "..." | ./target/release/rongnote new note "Standup 2026-05-01"
./target/release/rongnote spaces
```

Session (cookie + unwrapped master_key + privkey) cached at
`~/.config/rongnote/session.json` (chmod 600 on Unix). Set
`RONGNOTE_NO_PERSIST=1` to disable. `--server` / `RONGNOTE_SERVER`
overrides the target.

### Browser extension (Firefox / Chrome MV3)

A WebExtension at [`extension/`](extension/) — popup that surfaces secrets
matching the current tab's host with one-click copy of username, password,
TOTP. Same E2E crypto path as the SPA, separate session. Decrypted
payloads cached in `browser.storage.session` so subsequent popup opens
are instant; first-time decrypt of N secrets uses parallel batches.

```bash
cd extension
npm install
npm run build      # → extension/dist/
```

Firefox: `about:debugging` → **This Firefox** → **Load Temporary Add-on…** →
`extension/dist/manifest.json`. Chrome: `chrome://extensions` → **Load unpacked**
→ pick `extension/dist/`. Configure server URL in the extension's options
page once; signs in with passphrase, auto-locks after 15 min idle.

### Cryptography

End-to-end. Server can never read note bodies, secret payloads, or file
contents — see [SECURITY.md](SECURITY.md) for the full scheme.

| Primitive | Use |
|---|---|
| Argon2id (libsodium INTERACTIVE) | passphrase / recovery-code KDF |
| XSalsa20-Poly1305 (`crypto_secretbox`) | item bodies + key wrapping (personal) + share-link payloads |
| `crypto_box_seal` (sealed box) | per-member item-key wraps in team spaces |
| BLAKE2b keyed | auth-hash + passkey-KEK derivation |
| SHA-1 | HIBP k-anonymity prefix |
| WebAuthn PRF | passkey-derived KEK for vault unlock |

What the server **does** see: titles, tags, paths, due dates, task done
state, file sizes, timestamps. What it **does not**: passphrases, master
keys, private keys, note bodies, secret values, file contents, share-link
keys, team-space item keys.

## Stack

- **Backend:** Rust + Axum + SQLx + Postgres 16. Single static binary.
- **Frontend:** SvelteKit 2 + Svelte 5 (runes) + CodeMirror 6 + Lucide icons +
  svelte-dnd-action, bundled into the same binary via
  [rust-embed](https://crates.io/crates/rust-embed).
- **Crypto on the client:** [libsodium-wrappers-sumo](https://github.com/jedisct1/libsodium.js).
- **WebAuthn:** [webauthn-rs](https://github.com/kanidm/webauthn-rs) on the server.
- **CLI crypto:** RustCrypto stack (argon2 + crypto_secretbox + crypto_box + blake2).
- **Image:** multi-stage build → `ghcr.io/operator64/rongnote-server:latest`.

## Quick start

### Self-host

```bash
curl -O https://raw.githubusercontent.com/operator64/rongnote/main/docker-compose.example.yml
mv docker-compose.example.yml docker-compose.yml
echo "NOTES_DB_PW=$(openssl rand -base64 24)" > .env
docker compose up -d
# → http://localhost:8080
```

Put a TLS reverse proxy in front for production. See [deploy.md](deploy.md)
for a Cloudflare tunnel + Traefik example with backup retention notes.

After your first registration finishes, set `REGISTRATION_OPEN=false` in
`.env` and `docker compose up -d` again to lock the door. Existing users
can still log in; new accounts get a "registration closed" page.

### Local dev

```bash
cp .env.example .env                 # set NOTES_DB_PW
docker compose up -d notes-db
cd server && cargo run               # http://localhost:8080
cd web && npm install && npm run dev # http://localhost:5173 (proxies /api → :8080)
```

Hot-reload works for Svelte; the Rust server needs manual restart.

## Architecture

```
                 ┌─────────────────────────────────────────────┐
   Browser ◄────►│  SvelteKit SPA (embedded in Rust binary)    │
   (libsodium)   │                                             │
                 ├─────────────────────────────────────────────┤
                 │  Axum HTTP                                  │
                 │   /api/v1/auth          session cookies     │
                 │     /passkey            register/login/list │
                 │   /api/v1/items         CRUD per type       │
                 │     /:id/move           move between spaces │
                 │     /:id/versions       snapshots + restore │
                 │   /api/v1/spaces        team space mgmt     │
                 │     /:id/members        invite + re-wrap    │
                 │   /api/v1/files         encrypted blobs     │
                 │   /api/v1/audit_log     own activity        │
                 │   /api/v1/export        encrypted backup    │
                 │   /api/v1/share/<token> public read-only    │
                 │     /<token>/blob       file-share download │
                 ├─────────────────────────────────────────────┤
                 │  Postgres            sha256-addressed disk  │
                 │  ┌──────────────────┐    ┌────────────────┐│
                 │  │ users            │    │ blobs/ab/cdef…││
                 │  │ spaces / members │    │ blobs/12/3456…││
                 │  │ items            │    └────────────────┘│
                 │  │ item_member_keys │                       │
                 │  │ item_versions    │                       │
                 │  │ files_blobs      │                       │
                 │  │ passkeys         │                       │
                 │  │ share_links      │                       │
                 │  │ audit_log        │                       │
                 │  └──────────────────┘                       │
                 └─────────────────────────────────────────────┘
```

## Roadmap

Shipped:

- [x] Auth — passphrase, recovery code, WebAuthn PRF, multi-passkey, auto-lock
- [x] E2E crypto — notes, secrets, lists, files, tasks, snippets, bookmarks
- [x] Tags + path tree + search + command palette
- [x] Pinned items, daily notes, templates
- [x] Wiki-links + autocomplete + backlinks panel
- [x] Theme (light/dark/auto) + adjustable font size
- [x] Trash with restore + hard-delete
- [x] Version history (snapshots + restore)
- [x] Share via link — notes and files
- [x] HIBP breach check, audit log, encrypted export
- [x] Mobile responsive layout
- [x] **Team spaces** — invite, sealed-box-per-member wraps, atomic re-wrap on
      invite, move items between spaces
- [x] **CLI companion** — `rongnote login / ls / cat / new / spaces / use`
- [x] **Browser extension** (Firefox / Chrome MV3) — popup matches current
      tab host, copies user / pass / TOTP with auto-clear
- [x] **CSV import** — Firefox / Chrome / Bitwarden / 1Password / KeePass,
      smart `(host, username)` dedup
- [x] **Closeable signups** via `REGISTRATION_OPEN` env flag
- [x] CI/CD — GitHub Actions → ghcr.io → docker compose

Open:

- [ ] **CalDAV** — calendar items + iOS/macOS/Thunderbird sync
- [ ] Form-fill in the browser extension (currently copy-to-clipboard only)
- [ ] Save-from-page in the extension (capture new credentials from a login form)
- [ ] Vault import (counterpart to encrypted export — currently restore is
      manual SQL + blob copy per [deploy.md](deploy.md))
- [ ] Mozilla-signed extension build (currently temporary-add-on / unpacked)

## Documentation

- [SECURITY.md](SECURITY.md) — threat model, crypto details, what we promise
- [deploy.md](deploy.md) — production deploy + backup retention
- [CLAUDE.md](CLAUDE.md) — project context for AI coding sessions

## Contributing

Personal project; PRs and issues are welcome but I'm slow to review. If
you find a security bug, please open a private issue or email instead of
filing publicly.

## License

[MIT](LICENSE).
