# Deploy notes

Production deploy lives at `notes.ronglab.de`. Image is built by the
`build-image` workflow and pushed to
`ghcr.io/operator64/rongnote-server:latest` on every push to `main`.

## Initial deploy on the ronglab VM

Assumes the existing ronglab compose stack at `/opt/ronglab/docker-compose.yml`
already runs Traefik on the `ronglab` network with TLS via Cloudflare.

1. **Pull access for ghcr.io.** The package starts private. Either flip it
   to public on GitHub (Settings → Packages → rongnote-server → Change
   visibility), or log the VM into ghcr:

   ```bash
   echo "$GHCR_PAT" | docker login ghcr.io -u operator64 --password-stdin
   ```

   `GHCR_PAT` is a personal-access-token with `read:packages`. A public
   image avoids this entirely.

2. **Add the env file** at `/opt/ronglab/.env` (create or append):

   ```ini
   NOTES_DB_PW=<random-32-chars>
   NOTES_PUBLIC_URL=https://notes.ronglab.de
   ```

3. **Append to the compose file** at `/opt/ronglab/docker-compose.yml`:

   ```yaml
   notes:
     image: ghcr.io/operator64/rongnote-server:latest
     container_name: notes
     restart: unless-stopped
     networks: [ronglab]
     environment:
       DATABASE_URL: postgres://notes:${NOTES_DB_PW}@notes-db:5432/notes
       DATA_DIR: /data
       PUBLIC_URL: ${NOTES_PUBLIC_URL}
       BIND_ADDR: 0.0.0.0:8080
       APP_ENV: production
       SESSION_TTL: 30d
       RUST_LOG: info,tower_http=info,sqlx=warn
     volumes:
       - notes-data:/data
     labels:
       - traefik.enable=true
       - traefik.http.routers.notes.rule=Host(`notes.ronglab.de`)
       - traefik.http.routers.notes.entrypoints=web
       - traefik.http.services.notes.loadbalancer.server.port=8080
     depends_on:
       notes-db:
         condition: service_healthy

   notes-db:
     image: postgres:16-alpine
     container_name: notes-db
     restart: unless-stopped
     networks: [ronglab]
     environment:
       POSTGRES_USER: notes
       POSTGRES_PASSWORD: ${NOTES_DB_PW}
       POSTGRES_DB: notes
     volumes:
       - notes-db-data:/var/lib/postgresql/data
     healthcheck:
       test: ["CMD-SHELL", "pg_isready -U notes -d notes"]
       interval: 5s
       timeout: 3s
       retries: 10
   ```

   And under the top-level `volumes:`:

   ```yaml
   notes-data:
   notes-db-data:
   ```

4. **Bring it up.** Migrations run automatically on first start.

   ```bash
   cd /opt/ronglab
   docker compose pull notes
   docker compose up -d notes-db notes
   docker compose logs -f notes   # watch for "starting rongnote-server"
   ```

5. **Verify.** From the VM:
   ```bash
   curl -fsS http://localhost:8080/healthz   # → ok
   ```
   From your browser:
   ```
   https://notes.ronglab.de/healthz          → ok
   ```

## Updating

```bash
cd /opt/ronglab
docker compose pull notes
docker compose up -d notes
```

The image rolls forward; migrations apply at startup. Database volume
(`notes-db-data`) and blob volume (`notes-data`) survive restarts.

## Encrypted backup (per-user export)

## Backups

Cron on the host:

```cron
# pg dump + blob tar, daily at 03:30
30 3 * * *  /opt/ronglab/bin/notes-backup.sh
```

Where `notes-backup.sh` is something like:

```bash
#!/bin/bash
set -euo pipefail
ts=$(date -u +%Y%m%d-%H%M)
out=/backups/notes
mkdir -p "$out"

docker exec notes-db pg_dump -U notes notes | gzip > "$out/notes-db-$ts.sql.gz"
tar -C /var/lib/docker/volumes/rongnote_notes-data/_data -czf "$out/notes-blobs-$ts.tar.gz" .

# Keep 30 days
find "$out" -type f -mtime +30 -delete
```

The DB dump is consistent under load thanks to MVCC. Blobs are
content-addressed (sha256), so a torn write produces an unreferenced blob,
not corruption.

## Encrypted backup (per-user export)

Cmd-K → "export backup" downloads `rongnote-backup-<date>.tar`. Contents:

- `manifest.json` — format version, counts, timestamp.
- `users/me.json` — passphrase salt + recovery salt + master-key wraps +
  encrypted private key. Restore needs the same passphrase to decrypt.
- `items.jsonl` — one JSON line per item; bodies are still ciphertext.
- `blobs/<sha256>` — file blobs as stored on disk (still ciphertext).

Item titles, tags, paths, timestamps, and `due_at`/`done` are clear-text in
the archive. Same set of fields the server already sees in the DB. For full
at-rest privacy, add an outer envelope locally:

```bash
gzip rongnote-backup-20260427-2230.tar
age -p rongnote-backup-20260427-2230.tar.gz \
    > rongnote-backup-20260427-2230.tar.gz.age
shred -u rongnote-backup-20260427-2230.tar.gz
```

`age` is widely available (apt/brew/scoop). The `-p` mode is passphrase-based;
pick something different from your account passphrase if you want
defense-in-depth (so a leaked recovery doesn't immediately unlock backups).

Restoring (manual; v1.0 will add an upload endpoint):
1. Install age, decrypt the archive: `age -d backup.tar.gz.age | tar xz`
2. On a fresh server, point at an empty DB.
3. Use the schema to insert `users/me.json` data, `items.jsonl` rows, and
   copy `blobs/*` into `$DATA_DIR/blobs/<first-2-of-hash>/<full-hash>`.
4. Sign in with the original passphrase. Vault unlocks; data decrypts.

## Trash retention

Soft-deleted items (`items.deleted_at IS NOT NULL`) live forever unless
purged on the host. Cron purges anything older than 30 days:

```cron
# trash cleanup, daily at 03:00
0 3 * * *  docker exec notes-db psql -U notes -d notes -c "DELETE FROM items WHERE deleted_at < NOW() - INTERVAL '30 days';"
```

This only deletes the row. The blob (when files land in v0.6) is reference-
counted; a separate sweep collects orphans.

## TLS / Cloudflare

Cloudflare terminates TLS at the edge. Internal hop is HTTP via the
`ronglab` docker network. Set `PUBLIC_URL=https://notes.ronglab.de` in the
container env so cookies get the `Secure` flag and WebAuthn RP-ID is right.

## Health check

`GET /healthz` → 200, no auth. Wire it into Traefik's health-check or
Uptime Kuma:

```yaml
- "traefik.http.services.notes.loadbalancer.server.port=8080"
- "traefik.http.services.notes.loadbalancer.healthcheck.path=/healthz"
- "traefik.http.services.notes.loadbalancer.healthcheck.interval=30s"
```

## Migrations

Applied automatically at startup via `sqlx::migrate!()`. Idempotent and
transactional. To roll forward, just restart with the new image. To roll
back: there's no auto-down — restore from the latest pg_dump and downgrade
the image.

## Secrets

Live in `/opt/ronglab/.env`, never in the repo. Required:

- `NOTES_DB_PW` — postgres password
- `PUBLIC_URL` — `https://notes.ronglab.de`
- `APP_ENV=production`

Optional once they exist:

- `HIBP_API_KEY` — for the breach-check k-anonymity calls (server proxy)
- `SMTP_*` — for password-reset mails (no, the server can't reset, but for
  account notifications when they exist)
