<script lang="ts">
  import { onDestroy, onMount, tick } from 'svelte';
  import { goto } from '$app/navigation';
  import { page } from '$app/stores';
  import { api } from '$lib/api';
  import {
    generateItemKey,
    seal,
    toBase64,
    utf8Encode
  } from '$lib/crypto';
  import {
    decryptItemBody,
    encryptBodyForSpace,
    unwrapItemKey,
    wrapItemKey
  } from '$lib/itemCrypto';
  import { uploadFile } from '$lib/files';
  import { items } from '$lib/items.svelte';
  import { prefs } from '$lib/prefs.svelte';
  import { session } from '$lib/session.svelte';
  import { spaces } from '$lib/spaces.svelte';
  import { vault } from '$lib/vault.svelte';
  import { isPasskeySupported } from '$lib/webauthn';

  let open = $state(false);
  let query = $state('');
  let cursor = $state(0);
  let inputEl: HTMLInputElement | undefined = $state();
  let resultsEl: HTMLElement | undefined = $state();

  type CommandAction = {
    kind: 'action';
    label: string;
    hint?: string;
    run: () => void | Promise<void>;
  };
  type ItemHit = {
    kind: 'item';
    label: string;
    hint?: string;
    id: string;
    type: string;
  };
  type Item = CommandAction | ItemHit;

  /// All "new X" creation paths route through this so they consistently
  /// land in whichever space is currently selected in the sidebar — the
  /// server defaults to the user's personal space if space_id is omitted,
  /// which silently violates user intent when a team is active.
  function activeSpaceId(): string | undefined {
    return spaces.activeId ?? undefined;
  }

  const ACTIONS: CommandAction[] = [
    {
      kind: 'action',
      label: 'new note',
      hint: 'create',
      run: async () => {
        const item = await api.createItem({
          title: 'Untitled',
          type: 'note',
          space_id: activeSpaceId()
        });
        items.upsert(item);
        await goto(`/items/${item.id}`);
      }
    },
    {
      kind: 'action',
      label: 'new task',
      hint: 'create',
      run: async () => {
        const item = await api.createItem({
          title: 'New task',
          type: 'task',
          space_id: activeSpaceId()
        });
        items.upsert(item);
        await goto(`/items/${item.id}`);
      }
    },
    {
      kind: 'action',
      label: 'new list',
      hint: 'create',
      run: async () => {
        const item = await api.createItem({
          title: 'New list',
          type: 'list',
          space_id: activeSpaceId()
        });
        items.upsert(item);
        await goto(`/items/${item.id}`);
      }
    },
    {
      kind: 'action',
      label: 'new secret',
      hint: 'create',
      run: async () => {
        const item = await api.createItem({
          title: 'New secret',
          type: 'secret',
          space_id: activeSpaceId()
        });
        items.upsert(item);
        await goto(`/items/${item.id}`);
      }
    },
    {
      kind: 'action',
      label: 'new snippet',
      hint: 'create',
      run: async () => {
        const item = await api.createItem({
          title: 'New snippet',
          type: 'snippet',
          space_id: activeSpaceId()
        });
        items.upsert(item);
        await goto(`/items/${item.id}`);
      }
    },
    {
      kind: 'action',
      label: 'new bookmark',
      hint: 'create',
      run: async () => {
        const item = await api.createItem({
          title: 'New bookmark',
          type: 'bookmark',
          space_id: activeSpaceId()
        });
        items.upsert(item);
        await goto(`/items/${item.id}`);
      }
    },
    {
      kind: 'action',
      label: 'new event',
      hint: 'create',
      run: async () => {
        // Default: today, 9:00 — 10:00 local. The editor lets the user
        // adjust right after.
        const start = new Date();
        start.setHours(9, 0, 0, 0);
        const end = new Date(start);
        end.setHours(10, 0, 0, 0);
        const item = await api.createItem({
          title: 'New event',
          type: 'event',
          start_at: start.toISOString(),
          end_at: end.toISOString(),
          all_day: false,
          space_id: activeSpaceId()
        });
        items.upsert(item);
        await goto(`/items/${item.id}`);
      }
    },
    {
      kind: 'action',
      label: 'open calendar',
      hint: 'view',
      run: async () => {
        await goto('/items/calendar');
      }
    },
    {
      kind: 'action',
      label: 'open dashboard',
      hint: 'view',
      run: async () => {
        await goto('/dashboard');
      }
    },
    {
      kind: 'action',
      label: 'upload file',
      hint: 'create',
      run: async () => {
        if (!vault.masterKey) throw new Error('vault locked');
        const input = document.createElement('input');
        input.type = 'file';
        input.multiple = true;
        await new Promise<void>((resolve) => {
          input.onchange = async () => {
            try {
              if (input.files && input.files.length > 0) {
                let last;
                for (const f of Array.from(input.files)) {
                  last = await uploadFile({ file: f, masterKey: vault.masterKey! });
                }
                if (last && input.files.length === 1) {
                  await goto(`/items/${last.id}`);
                }
              }
            } finally {
              resolve();
            }
          };
          input.click();
        });
      }
    },
    {
      kind: 'action',
      label: 'lock vault',
      hint: 'security',
      run: () => vault.lock()
    },
    {
      kind: 'action',
      label: 'sign out',
      hint: 'security',
      run: async () => {
        vault.lock();
        await session.logout();
        await goto('/login', { replaceState: true });
      }
    },
    {
      kind: 'action',
      label: 'clear filter',
      hint: 'view',
      run: () => items.clearFilter()
    },
    {
      kind: 'action',
      label: 'pin / unpin current item',
      hint: 'view',
      run: async () => {
        const id = $page.params?.id;
        if (!id) throw new Error('open an item first');
        await items.togglePin(id);
      }
    },
    {
      kind: 'action',
      label: "today's daily note",
      hint: 'create',
      run: async () => {
        await openOrCreateDaily();
      }
    },
    {
      kind: 'action',
      label: 'scan all notes for backlinks',
      hint: 'view',
      run: async () => {
        await scanAllNotes();
      }
    },
    {
      kind: 'action',
      label: 'share current item via link',
      hint: 'create',
      run: async () => {
        const id = $page.params?.id;
        if (!id) throw new Error('open an item first');
        await shareCurrentItem(id);
      }
    },
    {
      kind: 'action',
      label: 'move current item to space…',
      hint: 'spaces',
      run: async () => {
        const id = $page.params?.id;
        if (!id) throw new Error('open an item first');
        await moveCurrentItem(id);
      }
    },
    {
      kind: 'action',
      label: 'version history of current item',
      hint: 'view',
      run: async () => {
        const id = $page.params?.id;
        if (!id) throw new Error('open an item first');
        await goto(`/items/${id}/history`);
      }
    },
    {
      kind: 'action',
      label: 'audit log',
      hint: 'security',
      run: async () => {
        await goto('/items/audit');
      }
    },
    {
      kind: 'action',
      label: 'import secrets from CSV',
      hint: 'create',
      run: async () => {
        await goto('/items/import');
      }
    },
    {
      kind: 'action',
      label: 'manage spaces',
      hint: 'spaces',
      run: async () => {
        await goto('/items/spaces');
      }
    },
    {
      kind: 'action',
      label: 'new team space',
      hint: 'spaces',
      run: async () => {
        const name = prompt('team space name:')?.trim();
        if (!name) return;
        const space = await api.createSpace(name);
        spaces.upsert(space);
        await goto(`/items/spaces/${space.id}`);
      }
    },
    {
      kind: 'action',
      label: 'members of current space',
      hint: 'spaces',
      run: async () => {
        const s = spaces.active;
        if (!s) throw new Error('no active space');
        if (s.kind !== 'team') throw new Error('personal space has no members');
        await goto(`/items/spaces/${s.id}`);
      }
    },
    {
      kind: 'action',
      label: 'export backup',
      hint: 'security',
      run: async () => {
        // Trigger a download via the same-origin API endpoint. The browser
        // sends the session cookie automatically.
        const a = document.createElement('a');
        a.href = '/api/v1/export';
        a.rel = 'noopener';
        a.click();
      }
    },
    { kind: 'action', label: 'theme: light', hint: 'theme', run: () => prefs.setTheme('light') },
    { kind: 'action', label: 'theme: dark', hint: 'theme', run: () => prefs.setTheme('dark') },
    { kind: 'action', label: 'theme: auto (system)', hint: 'theme', run: () => prefs.setTheme('auto') },
    { kind: 'action', label: 'font: smaller', hint: 'view', run: () => prefs.bumpFontSize(-1) },
    { kind: 'action', label: 'font: larger', hint: 'view', run: () => prefs.bumpFontSize(+1) },
    { kind: 'action', label: 'font: reset', hint: 'view', run: () => prefs.resetFontSize() }
  ];

  if (isPasskeySupported()) {
    ACTIONS.push({
      kind: 'action',
      label: 'manage passkeys',
      hint: 'security',
      run: async () => {
        await goto('/items/passkeys');
      }
    });
  }

  // Dev-only seed action. The dynamic import keeps demo data out of prod.
  if (import.meta.env.DEV) {
    ACTIONS.push({
      kind: 'action',
      label: 'seed demo data',
      hint: 'dev',
      run: async () => {
        const { seedDemoData } = await import('$lib/dev-seed');
        const r = await seedDemoData();
        console.info(`seed: ${r.created} created, ${r.skipped} skipped`);
      }
    });
  }

  function todayLocal(): { ymd: string; ym: string } {
    const d = new Date();
    const pad = (n: number) => n.toString().padStart(2, '0');
    return {
      ymd: `${d.getFullYear()}-${pad(d.getMonth() + 1)}-${pad(d.getDate())}`,
      ym: `${d.getFullYear()}-${pad(d.getMonth() + 1)}`
    };
  }

  /// Generate a fresh share key, decrypt the note body, re-encrypt with the
  /// share key, ship the ciphertext to the server. The key never leaves the
  /// browser — it lives only in the URL fragment we put on the clipboard.
  /// Re-wrap the current item's key for a different space and PATCH it
  /// across. Personal targets get a master-key secretbox; team targets get
  /// sealed-box-per-member.
  async function moveCurrentItem(id: string) {
    if (!vault.masterKey || !vault.publicKey || !vault.privateKey) {
      throw new Error('vault locked');
    }
    const item = await api.getItem(id);

    const candidates = spaces.list.filter((s) => s.id !== item.space_id);
    if (candidates.length === 0) {
      throw new Error('no other spaces available — create or join one first');
    }
    const list = candidates
      .map((s, i) => `  [${i + 1}] ${s.name} (${s.kind})`)
      .join('\n');
    const choice = prompt(
      `move to which space?\n\n${list}\n\nenter number 1-${candidates.length}:`,
      '1'
    );
    const n = parseInt(choice ?? '', 10);
    if (!Number.isFinite(n) || n < 1 || n > candidates.length) return;
    const target = candidates[n - 1];

    // For an item with a body: decrypt the existing item_key and re-wrap
    // for the target space. For an empty placeholder: just move, no wraps.
    const payload: {
      target_space_id: string;
      wrapped_item_key?: string;
      member_keys?: { user_id: string; sealed_item_key: string }[];
    } = { target_space_id: target.id };

    if (item.encrypted_body && item.wrapped_item_key) {
      const itemKey = unwrapItemKey(
        item,
        vault.masterKey,
        vault.publicKey,
        vault.privateKey
      );
      const wrap = await wrapItemKey(itemKey, target.id, vault.masterKey);
      Object.assign(payload, wrap);
    }

    const updated = await api.moveItem(id, payload);
    items.upsert(updated);
    alert(`moved to ${target.name}`);
  }

  async function shareCurrentItem(id: string) {
    if (!vault.masterKey || !vault.publicKey || !vault.privateKey) {
      throw new Error('vault locked');
    }
    const item = await api.getItem(id);
    if (item.type !== 'note' && item.type !== 'file') {
      throw new Error(`type ${item.type} cannot be shared via link`);
    }

    const days = parseInt(
      prompt('expire in how many days? (blank = never)', '7') ?? '',
      10
    );
    const expires_in_days = Number.isFinite(days) && days > 0 ? days : null;

    // generateItemKey gives 32 bytes — doubles as a share_key.
    const shareKey = generateItemKey();

    let payloadBytes: Uint8Array;
    if (item.type === 'note') {
      const body = decryptItemBody(item, vault.masterKey, vault.publicKey, vault.privateKey);
      payloadBytes = utf8Encode(body);
    } else {
      // For files: ship encrypted metadata + the original item_key so the
      // recipient can pull the ciphertext blob via /share/<token>/blob and
      // decrypt locally. The bytes never re-encrypt; saves bandwidth.
      const itemKey = unwrapItemKey(item, vault.masterKey, vault.publicKey, vault.privateKey);
      const meta = decryptItemBody(item, vault.masterKey, vault.publicKey, vault.privateKey);
      const parsed = JSON.parse(meta) as {
        filename: string;
        mime: string;
        size: number;
      };
      const sharePayload = {
        filename: parsed.filename,
        mime: parsed.mime,
        size: parsed.size,
        item_key_b64: toBase64(itemKey)
      };
      payloadBytes = utf8Encode(JSON.stringify(sharePayload));
    }

    const cipher = seal(payloadBytes, shareKey);
    const share = await api.createShare(id, {
      encrypted_payload: toBase64(cipher),
      expires_in_days
    });

    const fragment = toBase64(shareKey).replace(/\//g, '_').replace(/\+/g, '-');
    const url = `${location.origin}/share/${share.token}#${fragment}`;
    try {
      await navigator.clipboard.writeText(url);
      alert(`share link copied to clipboard:\n\n${url}`);
    } catch {
      prompt('copy this share link:', url);
    }
  }

  /// Bulk decrypt every note body so the backlinks panel sees the full
  /// graph. Runs in series — N small Argon2-free decrypts, fine for the
  /// realistic sizes we care about.
  async function scanAllNotes() {
    if (!vault.masterKey || !vault.publicKey || !vault.privateKey) {
      throw new Error('vault locked');
    }
    // items.list in active view excludes trash already
    const todo = items.list.filter((n) => n.type === 'note');
    let done = 0;
    for (const summary of todo) {
      try {
        const full = await api.getItem(summary.id);
        if (!full.encrypted_body || !full.wrapped_item_key) {
          items.setDecryptedBody(summary.id, '');
          continue;
        }
        const body = decryptItemBody(full, vault.masterKey, vault.publicKey, vault.privateKey);
        items.setDecryptedBody(summary.id, body);
      } catch (err) {
        console.warn('scan: failed to decrypt', summary.id, err);
      }
      done++;
    }
    alert(`scanned ${done} notes for backlinks.`);
  }

  /// Create a new note from a `/_templates/*` template. Decrypts the
  /// template's body, re-encrypts under a fresh per-item key for the new
  /// note, ships it.
  async function newFromTemplate(templateId: string) {
    if (!vault.masterKey || !vault.publicKey || !vault.privateKey) {
      throw new Error('vault locked');
    }
    const source = await api.getItem(templateId);
    const body = decryptItemBody(source, vault.masterKey, vault.publicKey, vault.privateKey);
    const targetSpaceId = spaces.activeId ?? spaces.personal()?.id ?? '';
    const wrap = await encryptBodyForSpace({
      body,
      spaceId: targetSpaceId,
      masterKey: vault.masterKey,
      publicKey: vault.publicKey,
      privateKey: vault.privateKey
    });
    const created = await api.createItem({
      type: 'note',
      title: `Untitled (from ${source.title})`,
      ...wrap,
      tags: source.tags.filter((t) => t !== 'template'),
      path: '/',
      space_id: targetSpaceId || undefined
    });
    items.upsert(created);
    await goto(`/items/${created.id}`);
  }

  async function openOrCreateDaily() {
    const { ymd, ym } = todayLocal();
    const title = `Daily ${ymd}`;
    const existing = items.list.find((n) => n.type === 'note' && n.title === title);
    if (existing) {
      await goto(`/items/${existing.id}`);
      return;
    }
    const item = await api.createItem({
      type: 'note',
      title,
      path: `/journal/${ym}`,
      tags: ['daily', 'journal'],
      space_id: activeSpaceId()
    });
    items.upsert(item);
    await goto(`/items/${item.id}`);
  }

  let templateActions = $derived.by<CommandAction[]>(() => {
    return items.list
      .filter(
        (n) =>
          n.type === 'note' &&
          (n.path === '/_templates' || n.path.startsWith('/_templates/'))
      )
      .map<CommandAction>((n) => ({
        kind: 'action',
        label: `new from template: ${n.title}`,
        hint: 'create',
        run: () => newFromTemplate(n.id)
      }));
  });

  let visibleItems = $derived.by<Item[]>(() => {
    const q = query.trim().toLowerCase();
    const allActions = [...ACTIONS, ...templateActions];
    if (!q) {
      const recent = items.list.slice(0, 8).map<Item>((n) => ({
        kind: 'item',
        label: n.title || '(untitled)',
        hint: n.type === 'note' ? n.path : `${n.type} · ${n.path}`,
        id: n.id,
        type: n.type
      }));
      return [...recent, ...allActions];
    }
    const itemHits = items.list
      .filter((n) => n.title.toLowerCase().includes(q))
      .slice(0, 12)
      .map<Item>((n) => ({
        kind: 'item',
        label: n.title || '(untitled)',
        hint: n.type === 'note' ? n.path : `${n.type} · ${n.path}`,
        id: n.id,
        type: n.type
      }));
    const actionHits = allActions.filter((a) => a.label.toLowerCase().includes(q));
    return [...itemHits, ...actionHits];
  });

  $effect(() => {
    if (cursor >= visibleItems.length) cursor = Math.max(0, visibleItems.length - 1);
  });

  /// Keep the active row in view when the user arrow-keys past the
  /// visible window. block:'nearest' makes mouse-hover a no-op.
  $effect(() => {
    void cursor;
    if (!resultsEl) return;
    const active = resultsEl.querySelector<HTMLElement>('.result.active');
    active?.scrollIntoView({ block: 'nearest' });
  });

  function onGlobalKey(e: KeyboardEvent) {
    if ((e.ctrlKey || e.metaKey) && e.key.toLowerCase() === 'k') {
      e.preventDefault();
      openPalette();
    } else if (e.key === 'Escape' && open) {
      closePalette();
    }
  }

  async function openPalette() {
    open = true;
    query = '';
    cursor = 0;
    await tick();
    inputEl?.focus();
  }

  function closePalette() {
    open = false;
  }

  async function run(item: Item) {
    closePalette();
    try {
      if (item.kind === 'action') {
        await item.run();
      } else {
        await goto(`/items/${item.id}`);
      }
    } catch (err) {
      const msg = err instanceof Error ? err.message : 'action failed';
      console.error('command palette action failed', err);
      alert(msg);
    }
  }

  function onInputKey(e: KeyboardEvent) {
    if (e.key === 'ArrowDown') {
      e.preventDefault();
      cursor = Math.min(visibleItems.length - 1, cursor + 1);
    } else if (e.key === 'ArrowUp') {
      e.preventDefault();
      cursor = Math.max(0, cursor - 1);
    } else if (e.key === 'Enter') {
      e.preventDefault();
      const item = visibleItems[cursor];
      if (item) run(item);
    }
  }

  onMount(() => window.addEventListener('keydown', onGlobalKey));
  onDestroy(() => window.removeEventListener('keydown', onGlobalKey));
</script>

{#if open}
  <div
    class="overlay"
    role="presentation"
    onclick={closePalette}
    onkeydown={(e) => e.key === 'Escape' && closePalette()}
  >
    <div
      class="palette"
      role="dialog"
      aria-modal="true"
      aria-label="command palette"
      tabindex="-1"
      onclick={(e) => e.stopPropagation()}
      onkeydown={(e) => e.stopPropagation()}
    >
      <input
        bind:this={inputEl}
        bind:value={query}
        onkeydown={onInputKey}
        placeholder="search items or run command…"
        autocomplete="off"
        spellcheck="false"
      />
      <div class="results" bind:this={resultsEl}>
        {#each visibleItems as item, i (item.kind + ':' + (item.kind === 'item' ? item.id : item.label))}
          <button
            type="button"
            class="result"
            class:active={i === cursor}
            onmousemove={() => {
              if (cursor !== i) cursor = i;
            }}
            onclick={() => run(item)}
          >
            <span class="kind muted">{item.kind === 'item' ? '·' : '›'}</span>
            <span class="grow">{item.label}</span>
            {#if item.hint}<span class="hint muted">{item.hint}</span>{/if}
          </button>
        {/each}
        {#if visibleItems.length === 0}
          <div class="empty muted">no matches</div>
        {/if}
      </div>
      <div class="footer muted">↑↓ navigate · ↵ run · esc close</div>
    </div>
  </div>
{/if}

<style>
  .overlay {
    position: fixed;
    inset: 0;
    background: rgba(0, 0, 0, 0.4);
    z-index: 100;
    display: flex;
    justify-content: center;
    align-items: flex-start;
    padding-top: 12vh;
  }
  .palette {
    width: min(560px, 90vw);
    background: var(--bg);
    border: 1px solid var(--border);
    display: flex;
    flex-direction: column;
    max-height: 70vh;
  }
  .palette input {
    border: none;
    border-bottom: 1px solid var(--border);
    padding: 10px 12px;
    width: 100%;
  }
  .palette input:focus {
    outline: none;
  }
  .results {
    overflow-y: auto;
    flex: 1;
  }
  .result {
    display: flex;
    align-items: center;
    gap: 8px;
    width: 100%;
    border: none;
    background: transparent;
    color: inherit;
    padding: 6px 12px;
    cursor: pointer;
    text-align: left;
    border-radius: 0;
  }
  .result.active {
    background: rgba(127, 127, 127, 0.16);
  }
  .kind {
    width: 10px;
    text-align: center;
  }
  .hint {
    font-size: 11px;
    flex-shrink: 0;
  }
  .empty {
    padding: 16px 12px;
  }
  .footer {
    padding: 6px 12px;
    border-top: 1px solid var(--border);
    font-size: 11px;
  }
  .grow {
    flex: 1 1 auto;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .muted {
    color: var(--muted);
  }
</style>
