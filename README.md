# rongnote

Self-hosted, end-to-end encrypted information hub. One tab to find your
notes, passwords, files, and tasks. Built for a small crew or just yourself.

> Status: **live** at [notes.ronglab.de](https://notes.ronglab.de) — single-user
> production, multi-user supported. See [ROADMAP](#roadmap) for what's next.

## Features

- **Notes** — Markdown, live preview, CodeMirror 6, `[[wiki-links]]` with
  autocomplete, broken-link detection.
- **Tasks** — checkboxes, due dates with overdue/today/future colour coding,
  one-click toggle from the list, sorted open-first.
- **Secrets** — passwords, TOTP codes (live with countdown), generator
  with charset toggles, copy-with-30s-clipboard-clear, reveal-with-10s-auto-hide.
- **Files** — drag & drop upload, encrypted on the client, content-addressed
  blobs on disk, inline preview for images / PDF / text. 50 MB cap (raise in
  config).
- **Tags + path tree + search** — sidebar with type / path / tag filters,
  Cmd-K command palette for everything.
- **Trash** — soft-delete with restore. Hard-delete clears blob refcount and
  removes orphans.
- **Audit log** — every secret read, item write, and auth event recorded.
  Per-user view at `/items/audit`.
- **Encrypted backup** — one-click `.tar` export with all your encrypted data.
  Pipe through `age -p` for full at-rest privacy.

### Auth

- Passphrase + recovery code (Argon2id, 24 chars base32 shown once).
- Optional **WebAuthn / Passkey** with PRF — sign in + unlock vault in one
  tap. Requires a PRF-capable authenticator (Yubikey 5+, Touch ID, recent
  Windows Hello, 1Password / Bitwarden).
- Auto-lock after 15 min idle.
- Sessions in HttpOnly Lax cookies, server-side store with TTL.

### Cryptography

End-to-end. Server can never read note bodies, secret payloads, or file
contents — see [SECURITY.md](SECURITY.md) for the full scheme.

| Primitive | Use |
|---|---|
| Argon2id (libsodium INTERACTIVE) | passphrase / recovery-code KDF |
| XSalsa20-Poly1305 (`crypto_secretbox`) | item bodies + key wrapping |
| BLAKE2b keyed | auth-hash derivation |
| X25519 keypair | reserved for v0.9 team-share sealing |
| WebAuthn PRF | passkey-derived KEK for vault unlock |

What the server **does** see: titles, tags, paths, due dates, task done state,
file sizes, timestamps. What it **does not**: passphrases, master keys,
private keys, note bodies, secret values, file contents.

## Stack

- **Backend:** Rust + Axum + SQLx + Postgres 16. Single static binary.
- **Frontend:** SvelteKit 2 + Svelte 5 (runes) + CodeMirror 6 + Lucide icons,
  bundled into the same binary via [rust-embed](https://crates.io/crates/rust-embed).
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
                 │   /api/v1/items         CRUD per type       │
                 │   /api/v1/files         encrypted blobs     │
                 │   /api/v1/audit_log     own activity        │
                 │   /api/v1/export        encrypted backup    │
                 ├─────────────────────────────────────────────┤
                 │  Postgres            sha256-addressed disk  │
                 │  ┌──────────────┐    ┌──────────────────┐  │
                 │  │ users        │    │ blobs/ab/cdef…   │  │
                 │  │ items        │    │ blobs/12/3456…   │  │
                 │  │ files_blobs  │    │ …                │  │
                 │  │ passkeys     │    └──────────────────┘  │
                 │  │ audit_log    │                          │
                 │  └──────────────┘                          │
                 └─────────────────────────────────────────────┘
```

## Roadmap

- [x] Auth — passphrase, recovery code, WebAuthn PRF, auto-lock
- [x] E2E crypto — notes, secrets, files, tasks
- [x] Items — notes, secrets (with TOTP + generator), tasks (with due
      dates), files (with previews)
- [x] Tags + path tree + search + command palette
- [x] Theme (light/dark/auto) + adjustable font size
- [x] Wiki-links + autocomplete in CodeMirror
- [x] Trash with restore + hard-delete
- [x] Audit log
- [x] Encrypted export
- [ ] **Team spaces + sharing** — schema is there, sealed-box wrap-per-member
      is the missing UI
- [ ] **CalDAV** — calendar items + iOS/macOS/Thunderbird sync
- [ ] Backlinks panel (requires decrypt-all index)
- [ ] Multi-passkey management
- [ ] HIBP breach check for passwords
- [ ] CLI companion (`rong note new`, `rong pw get`, `rong ssh-add`)
- [ ] Browser extension (autofill)

## Documentation

- [SECURITY.md](SECURITY.md) — threat model, crypto details, what we promise
- [deploy.md](deploy.md) — production deploy + backup retention
- [CLAUDE.md](CLAUDE.md) — project context for AI coding sessions

## License

[MIT](LICENSE).
