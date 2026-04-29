// Dev-only: seed a handful of realistic notes and secrets so the UI doesn't
// look empty during development. Tree-shaken out of prod builds because the
// only caller is gated on import.meta.env.DEV.

import { api, type ItemType } from './api';
import { encryptBodyForSpace } from './itemCrypto';
import { items } from './items.svelte';
import { spaces } from './spaces.svelte';
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
- notes deploy: docker-compose verifiziert, traefik labels stimmen
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
- caldav-only client für own calendar

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

// All values below are obvious DEMO PLACEHOLDERS. They look fake on
// purpose so a casual reader doesn't mistake them for real credentials
// being committed to source. dev-seed runs only when import.meta.env.DEV
// — never ships in prod.
const SECRETS: SecretSpec[] = [
  {
    type: 'secret',
    title: 'github.com',
    payload: {
      username: 'demo-user',
      password: 'demo-password-1',
      url: 'https://github.com',
      totp_seed: 'JBSWY3DPEHPK3PXP',
      notes: '2fa via app + recovery codes in password manager'
    },
    tags: ['dev', 'work'],
    path: '/work/accounts'
  },
  {
    type: 'secret',
    title: 'AWS root — example',
    payload: {
      username: 'demo-user@example.com',
      password: 'demo-password-2',
      url: 'https://signin.aws.amazon.com',
      totp_seed: 'KRSXG5BAONUW42LU',
      notes: 'root creds — billing/identity only. iam roles for everything else.'
    },
    tags: ['work', 'ops/aws'],
    path: '/work/accounts'
  },
  {
    type: 'secret',
    title: 'corp vpn',
    payload: {
      username: 'demo-user',
      password: 'demo-password-3',
      url: 'https://vpn.example.com',
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
      password: 'demo-password-4',
      url: 'https://example-bank.de',
      totp_seed: '',
      notes: 'login pin for online banking. tan via app.'
    },
    tags: ['personal', 'finanzen'],
    path: '/personal/finanzen'
  },
  {
    type: 'secret',
    title: 'mail — ops@',
    payload: {
      username: 'demo-user@example.com',
      password: 'demo-password-5',
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
      password: 'demo-token-6',
      url: 'https://dash.cloudflare.com',
      totp_seed: '',
      notes: 'scoped to zone:edit + dns:edit. used by ci.'
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
    if (!vault.publicKey || !vault.privateKey) throw new Error('vault locked');
    const targetSpaceId = spaces.activeId ?? spaces.personal()?.id ?? '';
    const wrap = await encryptBodyForSpace({
      body: bodyText,
      spaceId: targetSpaceId,
      masterKey: vault.masterKey,
      publicKey: vault.publicKey,
      privateKey: vault.privateKey
    });
    const item = await api.createItem({
      type: spec.type as ItemType,
      title: spec.title,
      tags: spec.tags,
      path: spec.path,
      ...wrap,
      space_id: targetSpaceId || undefined
    });
    items.upsert(item);
    created++;
  }

  return { created, skipped };
}
