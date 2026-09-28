# Transfer to another machine

Quick notes for moving a Claude Code session to a fresh workspace.
Complements [README.md](README.md) (which is the general dev setup).

## What's in the archive

`git archive` produces a tarball of every tracked file (146 of them,
~1 MB gzipped). Everything else — build artifacts, deps, secrets —
regenerates on the new machine.

**Not** in the archive, on purpose:

- `target/` — 5+ GB of Rust build artifacts, all reproducible
- `web/node_modules/`, `extension/node_modules/` — reproducible via `npm install`
- `web/build/`, `web/.svelte-kit/`, `extension/dist/` — build outputs
- `.git/` — history and remotes stay on the origin machine (use `git bundle`
  if you want history too, see below)
- `.env`, `.cargo/config.toml` — machine-specific
- `landing/prototypes/`, `scratch/` — throwaways

## Unpack + first run

```bash
tar xzf rongnote.tar.gz
cd rongnote

# rehydrate deps
cd web && npm install && cd ..
cd extension && npm install && cd ..

# copy env template + set a DB password
cp .env.example .env
# edit NOTES_DB_PW=

# spin up postgres + run server in dev
docker compose up -d notes-db
cd server && cargo run    # http://localhost:8080
# in another shell:
cd web && npm run dev     # http://localhost:5173 (proxies /api → :8080)
```

`cargo run` will pull ~500 crates the first time — 5–10 min on a
fresh machine, cached forever after.

## If you want git history too

`git archive` gives only the working tree. To bring the full
commit history + remote pointer:

```bash
# on the origin machine
cd C:/Users/opera/src/rongnote
git bundle create rongnote.bundle --all

# on the target machine
git clone rongnote.bundle rongnote
cd rongnote
git remote set-url origin https://github.com/operator64/rongnote.git
git remote update
```

The `.git` directory is only ~3 MB (git already compresses well),
so the bundle path is generally the better choice if you plan to
keep committing on the new machine.

## Platform notes for the new machine

- **Linux/macOS**: nothing special. `webauthn-rs` builds against
  system OpenSSL via apt/brew — just `apt install pkg-config libssl-dev`
  or `brew install openssl`.
- **Windows**: needs `.cargo/config.toml` for OpenSSL paths — copy
  from `.cargo/config.example.toml` after `winget install ShiningLight.OpenSSL.Dev`.
  This file is intentionally gitignored (path is machine-specific).
- **Docker**: `Dockerfile` is a multi-stage build that ignores host
  toolchains entirely — the `image.yml` GHA workflow produces
  `ghcr.io/operator64/rongnote-server:latest` from the source tree
  alone. Local Docker builds work the same way.

## Deploy target

Prod is at `notes.ronglab.de` behind Cloudflare tunnel + Traefik on
the `ronglab` host. The moved workspace can still push to that
deploy path:

```bash
git push                                             # → GHA image build
ssh ronglab "cd /opt/notes && docker compose pull notes && docker compose up -d notes"
```

See [deploy.md](deploy.md) for the full production layout.

## Where to point Claude Code first

On the fresh machine, tell Claude Code to read:

1. [CLAUDE.md](CLAUDE.md) — project context, crypto invariants,
   migrations list, gotchas 1–26. **Start here.**
2. [SECURITY.md](SECURITY.md) — full crypto scheme + threat model.
3. [README.md](README.md) — user-facing feature list.
4. [deploy.md](deploy.md) — prod deployment specifics.
