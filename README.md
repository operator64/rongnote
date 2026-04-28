# rongnote

Self-hosted, end-to-end encrypted information hub. One tab to find your
notes, passwords, files, tasks, lists, snippets, and bookmarks. Built for
a small crew or just yourself.

> Status: **live** at [notes.ronglab.de](https://notes.ronglab.de) — single-user
> production, multi-user supported. See [ROADMAP](#roadmap) for what's next.

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
- **Share via link** — `/share/<token>#<key>`. Notes only (v1). Server stores
  re-encrypted ciphertext; the share key lives in the URL fragment, never
  reaches the server. Per-link expire + revoke.
- **Audit log** — every secret read, item write, share creation, and auth
  event recorded. Per-user view at `/items/audit`.
- **Encrypted backup** — one-click `.tar` export with all your encrypted data.
  Pipe through `age -p` for full at-rest privacy.

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
  `upload file`, `today's daily note`, `manage passkeys`, `audit log`,
  `export backup`, `share current note`, `version history`, theme/font
  controls, etc.
- **Mobile** (`<700px`): stack-mode (list OR detail, never both); sidebar
  becomes a slide-in drawer; hamburger + search buttons in the pane head;
  status bar drops non-essential controls.

### Cryptography

End-to-end. Server can never read note bodies, secret payloads, or file
contents — see [SECURITY.md](SECURITY.md) for the full scheme.

| Primitive | Use |
|---|---|
| Argon2id (libsodium INTERACTIVE) | passphrase / recovery-code KDF |
| XSalsa20-Poly1305 (`crypto_secretbox`) | item bodies + key wrapping + share-link payloads |
| BLAKE2b keyed | auth-hash + passkey-KEK derivation |
| X25519 keypair | reserved for team-share sealing |
| SHA-1 | HIBP k-anonymity prefix |
| WebAuthn PRF | passkey-derived KEK for vault unlock |

What the server **does** see: titles, tags, paths, due dates, task done
state, file sizes, timestamps. What it **does not**: passphrases, master
keys, private keys, note bodies, secret values, file contents, share-link
keys.

## Stack

- **Backend:** Rust + Axum + SQLx + Postgres 16. Single static binary.
- **Frontend:** SvelteKit 2 + Svelte 5 (runes) + CodeMirror 6 + Lucide icons +
  svelte-dnd-action, bundled into the same binary via
  [rust-embed](https://crates.io/crates/rust-embed).
- **Crypto on the client:** [libsodium-wrappers-sumo](https://github.com/jedisct1/libsodium.js).
- **WebAuthn:** [webauthn-rs](https://github.com/kanidm/webauthn-rs) on the server.
- **Image:** multi-stage build → `ghcr.io/operator64/rongnote-server:latest`.

## Quick start

Local dev:

```bash
cp .env.example .env                 # set NOTES_DB_PW
docker compose up -d notes-db
cd server && cargo run               # http://localhost:8080
cd web && npm install && npm run dev # http://localhost:5173 (proxies /api → :8080)
```

Production deploy: see [deploy.md](deploy.md). Short version: drop the
compose snippet into your stack, run `docker compose up -d`, point a
hostname at it. The CI workflow at
[`.github/workflows/image.yml`](.github/workflows/image.yml) builds and
publishes the image on every push to `main`.

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
                 │     /:id/versions       snapshots + restore │
                 │   /api/v1/files         encrypted blobs     │
                 │   /api/v1/audit_log     own activity        │
                 │   /api/v1/export        encrypted backup    │
                 │   /api/v1/share/<token> public read-only    │
                 ├─────────────────────────────────────────────┤
                 │  Postgres            sha256-addressed disk  │
                 │  ┌──────────────┐    ┌──────────────────┐  │
                 │  │ users        │    │ blobs/ab/cdef…   │  │
                 │  │ items        │    │ blobs/12/3456…   │  │
                 │  │ item_versions│    │ …                │  │
                 │  │ files_blobs  │    └──────────────────┘  │
                 │  │ passkeys     │                          │
                 │  │ share_links  │                          │
                 │  │ audit_log    │                          │
                 │  └──────────────┘                          │
                 └─────────────────────────────────────────────┘
```

## Roadmap

Done:

- [x] Auth — passphrase, recovery code, WebAuthn PRF, multi-passkey, auto-lock
- [x] E2E crypto — notes, secrets, lists, files, tasks, snippets, bookmarks
- [x] Tags + path tree + search + command palette
- [x] Pinned items, daily notes, templates
- [x] Wiki-links + autocomplete + backlinks panel
- [x] Theme (light/dark/auto) + adjustable font size
- [x] Trash with restore + hard-delete
- [x] Version history (snapshots + restore)
- [x] Share via link (notes only)
- [x] HIBP breach check
- [x] Audit log
- [x] Encrypted export
- [x] Mobile responsive layout
- [x] CI/CD: GitHub Actions → ghcr.io → docker compose

Open:

- [ ] **Team spaces + sharing** — schema is there, sealed-box wrap-per-member
      is the missing UI
- [ ] **CalDAV** — calendar items + iOS/macOS/Thunderbird sync
- [ ] Share files (currently notes only)
- [ ] CLI companion (`rong note new`, `rong pw get`, `rong ssh-add`)
- [ ] Browser extension (autofill)
- [ ] Vault import (counterpart to encrypted export — currently restore is
      manual SQL + blob copy per [deploy.md](deploy.md))

## Documentation

- [SECURITY.md](SECURITY.md) — threat model, crypto details, what we promise
- [deploy.md](deploy.md) — production deploy + backup retention
- [CLAUDE.md](CLAUDE.md) — project context for AI coding sessions

## License

[MIT](LICENSE).
