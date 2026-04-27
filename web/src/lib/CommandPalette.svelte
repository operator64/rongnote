<script lang="ts">
  import { onDestroy, onMount, tick } from 'svelte';
  import { goto } from '$app/navigation';
  import { api } from '$lib/api';
  import { uploadFile } from '$lib/files';
  import { items } from '$lib/items.svelte';
  import { prefs } from '$lib/prefs.svelte';
  import { session } from '$lib/session.svelte';
  import { vault } from '$lib/vault.svelte';
  import { isPasskeySupported, registerPasskey } from '$lib/webauthn';

  let open = $state(false);
  let query = $state('');
  let cursor = $state(0);
  let inputEl: HTMLInputElement | undefined = $state();

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

  const ACTIONS: CommandAction[] = [
    {
      kind: 'action',
      label: 'new note',
      hint: 'create',
      run: async () => {
        const item = await api.createItem({ title: 'Untitled', type: 'note' });
        items.upsert(item);
        await goto(`/items/${item.id}`);
      }
    },
    {
      kind: 'action',
      label: 'new task',
      hint: 'create',
      run: async () => {
        const item = await api.createItem({ title: 'New task', type: 'task' });
        items.upsert(item);
        await goto(`/items/${item.id}`);
      }
    },
    {
      kind: 'action',
      label: 'new secret',
      hint: 'create',
      run: async () => {
        const item = await api.createItem({ title: 'New secret', type: 'secret' });
        items.upsert(item);
        await goto(`/items/${item.id}`);
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
      label: 'audit log',
      hint: 'security',
      run: async () => {
        await goto('/items/audit');
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
      label: 'register passkey',
      hint: 'security',
      run: async () => {
        if (!vault.masterKey) throw new Error('vault locked');
        const name = prompt('passkey name (e.g. "Yubikey personal")') ?? undefined;
        await registerPasskey({ masterKey: vault.masterKey, name: name || undefined });
        alert('passkey registered. you can now sign in with it.');
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

  let visibleItems = $derived.by<Item[]>(() => {
    const q = query.trim().toLowerCase();
    if (!q) {
      const recent = items.list.slice(0, 8).map<Item>((n) => ({
        kind: 'item',
        label: n.title || '(untitled)',
        hint: n.type === 'note' ? n.path : `${n.type} · ${n.path}`,
        id: n.id,
        type: n.type
      }));
      return [...recent, ...ACTIONS];
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
    const actionHits = ACTIONS.filter((a) => a.label.toLowerCase().includes(q));
    return [...itemHits, ...actionHits];
  });

  $effect(() => {
    if (cursor >= visibleItems.length) cursor = Math.max(0, visibleItems.length - 1);
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
      <div class="results">
        {#each visibleItems as item, i (item.kind + ':' + (item.kind === 'item' ? item.id : item.label))}
          <button
            type="button"
            class="result"
            class:active={i === cursor}
            onmouseenter={() => (cursor = i)}
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
