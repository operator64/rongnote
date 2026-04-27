# Security model

Read this before trusting rongnote with anything you'd hate to leak. The
threat model is explicit — what we defend against, and what we don't.

## Threat model

The deploy assumption is **the server is semi-trusted** — it can be
compromised, the database can leak. Your data should still be safe.

What an attacker who steals the database AND the file storage can see:

- email addresses
- item titles, tags, paths, timestamps
- task due dates and done state
- file sizes (sha256 of encrypted bytes, not plaintext)
- audit log entries
- public keys, encrypted private keys, key wrappings

What they can NOT see — even with full DB + filesystem access:

- passphrases or master keys
- note bodies
- secret values (passwords, TOTP seeds, custom fields, secret notes)
- file contents
- recovery codes

Confirming this: `docker exec notes-db pg_dump -U notes notes | grep -i
'<your secret>'` returns nothing.

What we do **not** defend against:

- a malicious server pushing a hostile JS bundle that exfiltrates the
  master key as you log in (fix: subresource-integrity + code review of
  every deploy, or a native client; neither is implemented)
- timing side channels on the auth endpoint
- a compromised authenticator
- physical access to an unlocked browser tab (the vault sits in
  `sessionStorage` while unlocked)

## Cryptographic primitives

| Primitive | Library | Where |
|---|---|---|
| Argon2id (INTERACTIVE: ops=2, mem=64 MiB) | libsodium `crypto_pwhash` | client KEK derivation |
| Argon2id (default params) | `argon2` crate | server-side hash of `auth_hash` |
| XSalsa20-Poly1305 | libsodium `crypto_secretbox_easy` | item bodies + key wrapping |
| BLAKE2b keyed (32-byte output) | libsodium `crypto_generichash` | auth_hash + passkey-KEK derivation |
| X25519 keypair | libsodium `crypto_box_keypair` | reserved for v0.9 team-share sealing |
| HMAC-SHA1 | Web Crypto | TOTP (RFC 6238) |
| WebAuthn PRF extension | browser + authenticator | passkey-derived KEK |
| SHA-256 | sha2 crate | file blob content addressing |

Random bytes from `crypto.getRandomValues` (browser) or `OsRng` (server).

## Key hierarchy

```
                         passphrase                recovery code (24 char base32)
                              │                            │
                Argon2id ▼ pp_salt          Argon2id ▼ rec_salt
                              │                            │
                       passphrase_kek ─┐         ┌─ recovery_kek
                                       │         │
                                       ▼         ▼
              random ────────►   master_key (32 bytes, never leaves the client unwrapped)
                                       │
       ┌──────────────┬─────────────────┼───────────────┐
       │              │                 │               │
   secretbox      secretbox          BLAKE2b       wraps each
   wraps mk       wraps mk           keyed         per-item key
       │              │                 │               │
       ▼              ▼                 ▼               ▼
master_wrap_     master_wrap_      auth_hash      wrapped_item_key
passphrase       recovery          (sent to        (per item, in DB)
(in DB)          (in DB)           server, server
                                   stores Argon2id
                                   of it)

Item bodies:
   utf8(plaintext) → secretbox(item_key) → encrypted_body  [in DB or, for files, on disk]

Per-user keypair (X25519):
   private_key  →  secretbox(master_key)  →  encrypted_private_key  [in DB]
```

For passkey-equipped users, a third wrap exists:

```
WebAuthn PRF eval (with fixed app-wide salt) → prf_output
prf_output  →  BLAKE2b("rongnote-passkey-kek-v1") → passkey_kek
passkey_kek  →  secretbox(master_key)  →  master_wrap_passkey  [in passkeys table]
```

## What's encrypted vs. plaintext on the server

Per spec §4 of the original brief, item *metadata* (titles, tags, paths,
timestamps, types, file sizes, task due dates) lives in plaintext on the
server. This is a deliberate trade-off: full-text search and sidebar
grouping work without client-side decrypt, which keeps the app fast at
realistic data volumes.

| Field | Plaintext server-side? |
|---|---|
| `users.email` | yes |
| `users.passphrase_salt` / `recovery_salt` | yes (random salts, not secret) |
| `users.auth_hash` | yes (Argon2id of client's BLAKE2b derivation) |
| `users.master_wrap_passphrase` / `master_wrap_recovery` | yes (opaque ciphertext) |
| `users.public_key` | yes |
| `users.encrypted_private_key` | yes (opaque ciphertext) |
| `items.title` | yes |
| `items.tags`, `items.path` | yes |
| `items.type`, timestamps, `items.due_at`, `items.done` | yes |
| `items.encrypted_body` | **ciphertext** |
| `items.wrapped_item_key` | **ciphertext** |
| `items.blob_sha256` | yes (hash of ciphertext, doesn't leak content) |
| `files_blobs/*` (on disk) | **ciphertext** |
| `passkeys.credential` (WebAuthn) | yes (public credential) |
| `passkeys.master_wrap_passkey` | **ciphertext** |
| `audit_log` rows | yes |

## Auth flow

### Register

```
client                                       server
   ─ generate random master_key (32 bytes)
   ─ generate X25519 keypair
   ─ random passphrase_salt + recovery_salt + recovery_code
   ─ derive passphrase_kek = Argon2id(passphrase, passphrase_salt)
   ─ derive recovery_kek   = Argon2id(recovery_code, recovery_salt)
   ─ master_wrap_passphrase = secretbox(master_key, passphrase_kek)
   ─ master_wrap_recovery   = secretbox(master_key, recovery_kek)
   ─ encrypted_private_key  = secretbox(privkey, master_key)
   ─ auth_hash              = BLAKE2b-keyed(master_key, "rongnote-auth-v1")
   ── POST /auth/register ──────────────────►
       {email, passphrase_salt, recovery_salt,
        master_wrap_passphrase, master_wrap_recovery,
        auth_hash, public_key, encrypted_private_key}
                                               ─ store Argon2id(auth_hash)
                                               ─ create personal space
                                               ─ issue session cookie
                                            ◄── 201 + UserView
   ─ show recovery_code ONCE, then drop
```

### Login (passphrase)

```
client                                       server
   ── POST /auth/precheck {email}  ──────────►
                                            ◄── {passphrase_salt, master_wrap_passphrase}
   ─ derive passphrase_kek
   ─ master_key = secretbox_open(master_wrap_passphrase, passphrase_kek)
   ─ auth_hash = BLAKE2b-keyed(master_key, …)
   ── POST /auth/login {email, auth_hash} ──►
                                               ─ verify Argon2id(auth_hash)
                                               ─ issue session cookie
                                            ◄── UserView
   ─ install master_key + privkey in vault.svelte.ts
```

### Login (passkey + PRF)

```
client                                       server
   ── POST /auth/passkey/login/begin ───────►
                                            ◄── { state_id, options (with PRF eval) }
   ─ navigator.credentials.get(options)
   ─ prf_output = response.extensions.prf.results.first
   ─ passkey_kek = BLAKE2b-keyed(prf_output, "rongnote-passkey-kek-v1")
   ── POST /auth/passkey/login/finish ──────►
       {state_id, response (signed assertion)}
                                               ─ verify webauthn signature
                                               ─ issue session cookie
                                            ◄── { user, master_wrap_passkey }
   ─ master_key = secretbox_open(master_wrap_passkey, passkey_kek)
   ─ install vault
```

### Recovery

```
client                                       server
   ── POST /auth/recovery_init {email} ─────►
                                            ◄── {recovery_salt, master_wrap_recovery}
   ─ derive recovery_kek
   ─ master_key = secretbox_open(master_wrap_recovery, recovery_kek)
   ─ generate new passphrase_salt
   ─ derive new passphrase_kek
   ─ new_master_wrap_passphrase = secretbox(master_key, new_passphrase_kek)
   ─ auth_hash = BLAKE2b-keyed(master_key, …) (unchanged)
   ── POST /auth/reset_passphrase ──────────►
       {email, auth_hash, new_passphrase_salt, new_master_wrap_passphrase}
                                               ─ verify Argon2id(auth_hash)
                                               ─ update wrap + salt
                                               ─ DELETE all sessions for user
                                            ◄── 204
```

A user who forgets the passphrase AND loses the recovery code has
unrecoverable data. **The server cannot help.** This is intentional.

## Vault state on the client

The unlocked `master_key` lives in:

1. JS memory while the tab is alive
2. `sessionStorage` so reload-within-tab survives without re-prompting

It does **not** live in `localStorage` and is **never** sent to the
server. Auto-lock fires after 15 minutes of no mousemove/keydown/click.

XSS-class attacks can read sessionStorage and exfiltrate the master key
of a logged-in user. Defenses:

- HttpOnly Lax session cookie (no `document.cookie` access)
- No third-party scripts
- All HTML rendered via `{@html}` is `marked.parse(...)` of *decrypted*
  user-controlled content — same as any other markdown viewer

## Audit log

Every auth event and every secret read is recorded in `audit_log` per
spec §3. Notes / files / tasks reads are NOT recorded — too noisy and
the "is the server reading my notes?" anxiety isn't worth the data.
Recorded actions:

- `auth.register`, `auth.login` (with `method: passphrase|passkey`),
  `auth.logout`, `auth.passphrase_reset`, `auth.passkey_register`
- `item.create`, `item.update`, `item.soft_delete`, `item.hard_delete`,
  `item.restore`
- `secret.read`
- `export`

Each user sees their own log at `/items/audit`. Per-user, never anyone
else's.

## Backup

`Cmd-K → export backup` returns a tar of `manifest.json`, `users/me.json`
(salts + wraps + pubkey + encrypted privkey, hex-encoded), `items.jsonl`
(one item per line, bodies still ciphertext), and `blobs/<sha256>` for
all referenced file blobs.

The tar is **not outer-encrypted** — item titles/tags/paths/timestamps
are visible to anyone who reads it. Same exposure as a `pg_dump`. For
full at-rest privacy of the backup file, pipe it through age:

```bash
age -p backup.tar > backup.tar.age
```

Restore on a fresh server: see [deploy.md](deploy.md#encrypted-backup-per-user-export).

## Reporting

Open an issue at <https://github.com/operator64/rongnote/issues>. For
sensitive disclosure, contact the operator listed in the repo settings
out-of-band.
