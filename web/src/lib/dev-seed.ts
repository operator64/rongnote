// Dev-only: seed a handful of realistic notes and secrets so the UI doesn't
// look empty during development. Tree-shaken out of prod builds because the
// only caller is gated on import.meta.env.DEV.

import { api, type ItemType } from './api';
import { generateItemKey, seal, toBase64, utf8Encode } from './crypto';
import { items } from './items.svelte';
import { vault } from './vault.svelte';

interface NoteSpec {
  type: 'note';
  title: string;
  body: string;
  tags: string[];
  path: string;
}

interface SecretSpec {
  type: 'secret';
  title: string;
  payload: {
    username: string;
    password: string;
    url: string;
    totp_seed: string;
    notes: string;
  };
  tags: string[];
  path: string;
}

const NOTES: NoteSpec[] = [
  {
    type: 'note',
    title: 'Migration plan — postgres 15 → 16',
    body: `# Postgres 15 → 16 migration

## Steps
1. Add replica, replicate from primary.
2. Switch over during low-traffic window (Sun 03:00 UTC).
3. Drop old primary after 48h burn-in.

## Rollback
- Keep WAL archive for 7d.
- Verified pg_dump runs on the old version before switch.

See [[deploy]] and \`docker-compose.yml\` for details.`,
    tags: ['work', 'ops/db'],
    path: '/work/projekte/migrate'
  },
  {
    type: 'note',
    title: 'Meeting — 2026-04-22 ops sync',
    body: `# Ops sync — 2026-04-22

attendees: jana, mo, tim

- alert fatigue: too many warning-level pages, propose tiering
- vendor-renewal cycle starts in 6w
- runbook for the auth service is stale (last touched 2025-11)

## todo
- [x] move alert thresholds doc to /work/ops
- [ ] schedule renewal review with finance
- [ ] tim writes runbook draft`,
    tags: ['work', 'meeting'],
    path: '/work/meetings'
  },
  {
    type: 'note',
    title: 'Reading queue',
    body: `# Reading queue

- *Designing Data-Intensive Applications* — Kleppmann (re-read ch. 7-9)
- *Crafting Interpreters* — Nystrom (online, free)
- *The Soul of a New Machine* — Kidder
- "End-to-end Arguments in System Design" (Saltzer/Reed/Clark, 1984)
- Tigerbeetle blog series on consensus`,
    tags: ['personal', 'books'],
    path: '/personal/lesen'
  },
  {
    type: 'note',
    title: 'Daily 2026-04-27',
    body: `# 2026-04-27 sun

## morgen
- ronglab notes deploy: docker-compose verifiziert, traefik labels stimmen
- inbox triagiert (37 → 3)

## offen
- [ ] backup-cron prüfen ob er läuft (zuletzt vorher per hand)
- [ ] mail an stefan wegen db-migration

## gelesen
- post-mortem von der letzten cloudflare störung — single-control-plane war hier wieder das problem`,
    tags: ['daily', 'journal'],
    path: '/journal/2026-04'
  },
  {
    type: 'note',
    title: 'Side project ideas',
    body: `# Side project ideas

## probably
- self-hosted notes/secrets app (this one)
- caldav-only client für ronglab calendar

## maybe
- markdown→pdf renderer mit besserem code-highlighting als typora
- rss reader minus die social features

## nope
- "yet another" task manager
- chat app`,
    tags: ['personal', 'ideen'],
    path: '/personal/projekte'
  },
  {
    type: 'note',
    title: 'Snippets — common shell',
    body: `# common shell snippets

## listen offen halten beim grep
\`\`\`sh
tail -f log | grep --line-buffered ERROR
\`\`\`

## json einrücken
\`\`\`sh
jq . file.json
\`\`\`

## ports anzeigen
\`\`\`sh
ss -tlnp
\`\`\``,
    tags: ['snippets', 'shell'],
    path: '/work/snippets'
  }
];

const SECRETS: SecretSpec[] = [
  {
    type: 'secret',
    title: 'github.com',
    payload: {
      username: 'rongops',
      password: 'kP9$mZ!2vL@xQ8nR4tH7',
      url: 'https://github.com',
      totp_seed: 'JBSWY3DPEHPK3PXP',
      notes: '2fa via app + recovery codes in 1password'
    },
    tags: ['dev', 'work'],
    path: '/work/accounts'
  },
  {
    type: 'secret',
    title: 'AWS root — ronglab',
    payload: {
      username: 'ops@ronglab.de',
      password: 'Tx#7yL2qV!nM8pW3zA9d',
      url: 'https://signin.aws.amazon.com',
      totp_seed: 'KRSXG5BAONUW42LU',
      notes: 'root creds — should only be used for billing/identity changes. iam roles for everything else.'
    },
    tags: ['work', 'ops/aws'],
    path: '/work/accounts'
  },
  {
    type: 'secret',
    title: 'corp vpn',
    payload: {
      username: 'jana.k',
      password: 'Bq4!fH7nC@2rT9sX5kP',
      url: 'https://vpn.ronglab.de',
      totp_seed: 'GEZDGNBVGY3TQOJQGEZDGNBVGY3TQOJQ',
      notes: 'split tunnel. office cidr is 10.40.0.0/16.'
    },
    tags: ['work'],
    path: '/work/accounts'
  },
  {
    type: 'secret',
    title: 'bank — girokonto',
    payload: {
      username: 'DE89370400440532013000',
      password: 'PrqZ2x7TbyMnH9wK',
      url: 'https://example-bank.de',
      totp_seed: '',
      notes: 'login pin for online banking. tan via app.'
    },
    tags: ['personal', 'finanzen'],
    path: '/personal/finanzen'
  },
  {
    type: 'secret',
    title: 'protonmail — ops@',
    payload: {
      username: 'ops@ronglab.de',
      password: 'wM3$kZ!8qB@vL5nT2yR',
      url: 'https://mail.proton.me',
      totp_seed: 'MFRGGZDFMZTWQ2LK',
      notes: 'shared with the ops crew. password rotates jan + jul.'
    },
    tags: ['work'],
    path: '/work/accounts'
  },
  {
    type: 'secret',
    title: 'cloudflare api — automation',
    payload: {
      username: '',
      password: 'cf_token_8nL7yP2qB4xV9mZ3kR5tH6w',
      url: 'https://dash.cloudflare.com',
      totp_seed: '',
      notes: 'scoped to zone:ronglab.de + dns:edit. used by gh actions.'
    },
    tags: ['dev', 'ops/dns'],
    path: '/work/accounts'
  }
];

export async function seedDemoData(): Promise<{
  created: number;
  skipped: number;
}> {
  if (!vault.masterKey) throw new Error('vault locked');

  // Skip titles that already exist so re-running doesn't duplicate.
  const existingTitles = new Set(items.list.map((i) => i.title));
  let created = 0;
  let skipped = 0;

  const all: Array<NoteSpec | SecretSpec> = [...NOTES, ...SECRETS];

  for (const spec of all) {
    if (existingTitles.has(spec.title)) {
      skipped++;
      continue;
    }
    const bodyText =
      spec.type === 'note' ? spec.body : JSON.stringify(spec.payload);
    const itemKey = generateItemKey();
    const encryptedBody = seal(utf8Encode(bodyText), itemKey);
    const wrappedItemKey = seal(itemKey, vault.masterKey);
    const item = await api.createItem({
      type: spec.type as ItemType,
      title: spec.title,
      tags: spec.tags,
      path: spec.path,
      encrypted_body: toBase64(encryptedBody),
      wrapped_item_key: toBase64(wrappedItemKey)
    });
    items.upsert(item);
    created++;
  }

  return { created, skipped };
}
