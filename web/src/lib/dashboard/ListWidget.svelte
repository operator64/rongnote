<script lang="ts">
  import { goto } from '$app/navigation';
  import { api, type Item } from '$lib/api';
  import { decryptItemBody, encryptBodyForSpace } from '$lib/itemCrypto';
  import { items } from '$lib/items.svelte';
  import { vault } from '$lib/vault.svelte';
  import { dashboardSettings } from '$lib/dashboardSettings.svelte';
  import Widget from './Widget.svelte';

  /// Pinned-list widget. Shows the user-selected pinned list (defaults to
  /// the first one). Checkbox toggles persist via api.updateItem with a
  /// re-encrypted body. If the vault is locked, the dropdown still works
  /// (item titles are plaintext) but list contents read 'unlock to view'.

  type Entry = { id: string; text: string; done: boolean };
  type ListPayload = { entries: Entry[] };

  function newId(): string {
    return crypto.randomUUID().slice(0, 8);
  }

  let pinnedLists = $derived(
    items.list
      .filter((it) => it.type === 'list' && it.pinned)
      .sort((a, b) => a.title.localeCompare(b.title))
  );

  /// Fall back to the first pinned list if the saved id is gone.
  let selectedId = $derived.by(() => {
    const wanted = dashboardSettings.s.list_id;
    if (wanted && pinnedLists.some((l) => l.id === wanted)) return wanted;
    return pinnedLists[0]?.id ?? null;
  });
  let selected = $derived(pinnedLists.find((l) => l.id === selectedId) ?? null);

  let fullItem = $state<Item | null>(null);
  let entries = $state<Entry[]>([]);
  let decryptedFor = $state<string | null>(null); // id we last decrypted for
  let decryptedAt = $state<string | null>(null);  // updated_at we last decrypted at
  let loadError = $state('');

  $effect(() => {
    if (!selectedId) {
      fullItem = null;
      entries = [];
      decryptedFor = null;
      return;
    }
    const sum = pinnedLists.find((l) => l.id === selectedId);
    if (!sum) return;
    // Re-load when id changes OR the summary's updated_at advanced.
    if (decryptedFor === selectedId && decryptedAt === sum.updated_at) return;
    void load(selectedId);
  });

  async function load(id: string) {
    loadError = '';
    try {
      const it = await api.getItem(id);
      fullItem = it;
      decryptedFor = id;
      decryptedAt = it.updated_at;
      if (
        !vault.masterKey ||
        !vault.publicKey ||
        !vault.privateKey ||
        !it.encrypted_body
      ) {
        entries = [];
        return;
      }
      const text = decryptItemBody(it, vault.masterKey, vault.publicKey, vault.privateKey);
      const parsed = JSON.parse(text) as Partial<ListPayload>;
      entries = (parsed.entries ?? []).map((e) => ({
        id: e.id ?? newId(),
        text: e.text ?? '',
        done: !!e.done
      }));
    } catch (err) {
      loadError = err instanceof Error ? err.message : 'load failed';
    }
  }

  let saving = $state(false);
  async function persist() {
    if (!fullItem) return;
    if (!vault.masterKey || !vault.publicKey || !vault.privateKey) {
      throw new Error('vault locked — unlock to edit');
    }
    saving = true;
    try {
      // encryptBodyForSpace handles the personal-vs-team wrap-shape +
      // first-save-vs-subsequent-save split. For a team-space list with
      // an existing wrap it reuses the item_key (no member_keys sent),
      // which is what the server's update path requires.
      const wrap = await encryptBodyForSpace({
        body: JSON.stringify({ entries }),
        spaceId: fullItem.space_id,
        masterKey: vault.masterKey,
        publicKey: vault.publicKey,
        privateKey: vault.privateKey,
        item: fullItem
      });
      const updated = await api.updateItem(fullItem.id, {
        update_body: true,
        ...wrap
      });
      fullItem = updated;
      decryptedAt = updated.updated_at;
      items.upsert(updated);
    } finally {
      saving = false;
    }
  }

  let writeBackTimer: ReturnType<typeof setTimeout> | null = null;
  function scheduleSave() {
    if (writeBackTimer) clearTimeout(writeBackTimer);
    writeBackTimer = setTimeout(() => {
      void persist().catch((err) => (loadError = err.message));
    }, 400);
  }

  function toggle(idx: number) {
    if (!vault.masterKey) {
      loadError = 'vault locked — open the editor to unlock';
      return;
    }
    entries[idx].done = !entries[idx].done;
    entries = entries; // Svelte5 array mutation
    scheduleSave();
  }

  // --- Modal state ---
  let modalOpen = $state(false);
  let modalEntries = $state<Entry[]>([]);

  function openModal() {
    modalEntries = entries.map((e) => ({ ...e }));
    modalOpen = true;
  }
  function closeModal(save: boolean) {
    if (save) {
      entries = modalEntries.map((e) => ({ ...e }));
      void persist().catch((err) => (loadError = err.message));
    }
    modalOpen = false;
  }
  function modalAdd() {
    modalEntries = [...modalEntries, { id: newId(), text: '', done: false }];
  }
  function modalRemove(i: number) {
    modalEntries = modalEntries.filter((_, j) => j !== i);
  }
  function modalMove(i: number, dir: -1 | 1) {
    const j = i + dir;
    if (j < 0 || j >= modalEntries.length) return;
    const next = modalEntries.slice();
    [next[i], next[j]] = [next[j], next[i]];
    modalEntries = next;
  }
  function modalSweep() {
    modalEntries = modalEntries.filter((e) => !e.done);
  }

  let progress = $derived.by(() => {
    const total = entries.length;
    const done = entries.filter((e) => e.done).length;
    return { done, total };
  });
</script>

<Widget
  title="liste"
  meta={selected ? `${progress.done} / ${progress.total}` : ''}
>
  {#snippet actions()}
    {#if pinnedLists.length > 0}
      <select
        value={selectedId ?? ''}
        onchange={(e) =>
          dashboardSettings.save({
            list_id: (e.currentTarget as HTMLSelectElement).value || null
          })}
        title="liste wählen"
      >
        {#each pinnedLists as l (l.id)}
          <option value={l.id}>{l.title}</option>
        {/each}
      </select>
    {/if}
    <button type="button" onclick={openModal} title="bearbeiten" disabled={!selected}>✎</button>
    <button
      type="button"
      onclick={() => selected && goto(`/items/${selected.id}`)}
      title="im editor öffnen"
      disabled={!selected}
    >↗</button>
  {/snippet}

  {#if pinnedLists.length === 0}
    <div class="muted empty">
      keine pinned lists. pin eine liste damit sie hier auftaucht.
    </div>
  {:else if !selected}
    <div class="muted empty">keine liste gewählt.</div>
  {:else if loadError}
    <div class="danger small">{loadError}</div>
  {:else if !vault.masterKey}
    <div class="muted small">vault gelocked. zum entsperren auf den ↗ klicken.</div>
  {:else}
    {#each entries as e, i (e.id)}
      <div
        class="entry"
        class:done={e.done}
        role="button"
        tabindex="0"
        onclick={openModal}
        onkeydown={(ev) => ev.key === 'Enter' && openModal()}
        title="für vergrößerte ansicht klicken"
      >
        <button
          type="button"
          class="box"
          onclick={(ev) => {
            ev.stopPropagation();
            toggle(i);
          }}
          aria-label={e.done ? 'als offen markieren' : 'als erledigt markieren'}
        >{e.done ? '✓' : ''}</button>
        <span class="text">{e.text || '(leer)'}</span>
      </div>
    {/each}
    {#if entries.length === 0}
      <div
        class="muted empty"
        role="button"
        tabindex="0"
        onclick={openModal}
        onkeydown={(ev) => ev.key === 'Enter' && openModal()}
      >
        leere liste — tippen um einträge hinzuzufügen
      </div>
    {/if}
  {/if}
</Widget>

{#if modalOpen}
  <div
    class="modal-overlay"
    role="presentation"
    onclick={() => closeModal(false)}
    onkeydown={(e) => e.key === 'Escape' && closeModal(false)}
  >
    <div
      class="modal"
      role="dialog"
      aria-modal="true"
      tabindex="-1"
      onclick={(e) => e.stopPropagation()}
      onkeydown={(e) => e.stopPropagation()}
    >
      <div class="modal-head">
        <strong>{selected?.title ?? 'liste'}</strong>
        <span class="grow"></span>
        <button type="button" onclick={modalSweep} title="erledigte entfernen">clear done</button>
        <button type="button" onclick={() => closeModal(false)}>cancel</button>
        <button type="button" class="primary" onclick={() => closeModal(true)} disabled={saving}>
          {saving ? 'saving…' : 'save'}
        </button>
      </div>
      <div class="modal-body">
        {#each modalEntries as e, i (e.id)}
          <div class="modal-row">
            <button
              type="button"
              class="boxbtn"
              onclick={() => (e.done = !e.done)}
              title="toggle done"
            >{e.done ? '✓' : ''}</button>
            <input
              class="modal-text"
              type="text"
              bind:value={e.text}
              class:done={e.done}
              placeholder="…"
            />
            <button type="button" class="iconbtn" onclick={() => modalMove(i, -1)} title="up">↑</button>
            <button type="button" class="iconbtn" onclick={() => modalMove(i, 1)} title="down">↓</button>
            <button type="button" class="iconbtn danger" onclick={() => modalRemove(i)} title="remove">×</button>
          </div>
        {/each}
        <button type="button" class="add-row" onclick={modalAdd}>+ neuer eintrag</button>
      </div>
    </div>
  </div>
{/if}

<style>
  .entry {
    display: flex;
    align-items: center;
    gap: 8px;
    width: 100%;
    padding: 5px 4px;
    border: none;
    background: transparent;
    color: inherit;
    text-align: left;
    cursor: pointer;
    font: inherit;
    border-bottom: 1px solid var(--border);
  }
  .entry:hover { background: rgba(127, 127, 127, 0.06); }
  .entry:last-child { border-bottom: none; }
  .entry .box {
    width: 22px; height: 22px;
    border: 1px solid var(--border);
    background: var(--bg);
    flex-shrink: 0;
    display: inline-flex; align-items: center; justify-content: center;
    color: var(--accent);
    font-size: 13px;
    cursor: pointer;
    padding: 0;
  }
  .entry .box:hover { border-color: var(--fg); }
  .entry.done .box { background: rgba(127,127,127,0.10); }
  .entry .text { flex: 1; min-width: 0; overflow: hidden; text-overflow: ellipsis; }
  .entry.done .text { text-decoration: line-through; color: var(--muted); }
  select {
    background: var(--bg);
    color: var(--fg);
    border: 1px solid var(--border);
    padding: 1px 4px;
    font: inherit;
    font-size: 11px;
    max-width: 140px;
  }
  .empty { padding: 16px 0; text-align: center; font-size: 12px; cursor: pointer; }
  .small { font-size: 11px; }

  .modal-overlay {
    position: fixed;
    inset: 0;
    background: rgba(0, 0, 0, 0.45);
    z-index: 100;
    display: flex;
    justify-content: center;
    align-items: flex-start;
    padding-top: 10vh;
  }
  .modal {
    width: min(560px, 90vw);
    max-height: 80vh;
    background: var(--bg);
    border: 1px solid var(--border);
    display: flex; flex-direction: column;
  }
  .modal-head {
    display: flex; align-items: center; gap: 6px;
    padding: 8px 10px;
    border-bottom: 1px solid var(--border);
  }
  .modal-head .grow { flex: 1; }
  .modal-head .primary {
    background: var(--accent); border-color: var(--accent); color: white;
  }
  .modal-body {
    overflow-y: auto;
    padding: 8px 10px;
    flex: 1;
  }
  .modal-row {
    display: flex; align-items: center; gap: 6px;
    padding: 3px 0;
  }
  .modal-text {
    flex: 1; min-width: 0;
    background: var(--bg);
    color: var(--fg);
    border: 1px solid var(--border);
    padding: 3px 6px;
  }
  .modal-text.done { color: var(--muted); text-decoration: line-through; }
  .boxbtn, .iconbtn {
    width: 24px; height: 24px;
    padding: 0;
    border: 1px solid var(--border);
    background: var(--bg);
    color: var(--fg);
    cursor: pointer;
    display: inline-flex; align-items: center; justify-content: center;
    font-size: 11px;
  }
  .boxbtn:hover, .iconbtn:hover { border-color: var(--fg); }
  .iconbtn.danger { color: var(--danger); }
  .add-row {
    width: 100%; margin-top: 8px;
    border: 1px dashed var(--border);
    background: transparent;
    color: var(--muted);
    padding: 6px;
    cursor: pointer;
  }
  .add-row:hover { color: var(--fg); border-color: var(--fg); }
</style>
