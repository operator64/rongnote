<script lang="ts">
  import { tick } from 'svelte';
  import { goto } from '$app/navigation';
  import { GripVertical, X } from '@lucide/svelte';
  import { dndzone, SOURCES } from 'svelte-dnd-action';
  import { api, type Item } from '$lib/api';
  import { decryptItemBody, encryptBodyForSpace } from '$lib/itemCrypto';
  import {
    formatTagInput,
    items,
    normalizePath,
    parseTagInput
  } from '$lib/items.svelte';
  import TaskCheckbox from '$lib/TaskCheckbox.svelte';
  import { vault } from '$lib/vault.svelte';

  type Entry = { id: string; text: string; done: boolean };
  type ListPayload = { entries: Entry[] };

  function emptyPayload(): ListPayload {
    return { entries: [] };
  }
  function newId(): string {
    return crypto.randomUUID().slice(0, 8);
  }

  type Props = { item: Item };
  let { item: initial }: Props = $props();

  // svelte-ignore state_referenced_locally
  let item = $state<Item>(initial);
  // svelte-ignore state_referenced_locally
  let title = $state(initial.title);
  // svelte-ignore state_referenced_locally
  let entries = $state<Entry[]>(decryptPayload(initial).entries);
  // svelte-ignore state_referenced_locally
  let tagsInput = $state(formatTagInput(initial.tags));
  // svelte-ignore state_referenced_locally
  let pathInput = $state(initial.path);
  // svelte-ignore state_referenced_locally
  let lastSavedAt = $state<Date | null>(new Date(initial.updated_at));
  let error = $state('');
  let saving = $state(false);
  let dirty = $state(false);
  let saveTimer: ReturnType<typeof setTimeout> | null = null;
  const SAVE_DEBOUNCE_MS = 600;

  let trashed = $derived(!!item.deleted_at);
  let progress = $derived.by(() => {
    const total = entries.length;
    const done = entries.filter((e) => e.done).length;
    return { done, total };
  });

  /// Refs to entry text inputs so Enter on the last row can focus the new
  /// row, and Backspace on an empty row can focus the previous.
  let inputRefs: Record<string, HTMLInputElement | undefined> = $state({});

  $effect(() => {
    if (initial.id !== item.id) {
      item = initial;
      title = initial.title;
      entries = decryptPayload(initial).entries;
      tagsInput = formatTagInput(initial.tags);
      pathInput = initial.path;
      dirty = false;
      error = '';
      lastSavedAt = new Date(initial.updated_at);
      inputRefs = {};
    }
  });

  function decryptPayload(it: Item): ListPayload {
    if (!it.encrypted_body || !it.wrapped_item_key) return emptyPayload();
    if (!vault.masterKey || !vault.publicKey || !vault.privateKey) return emptyPayload();
    try {
      const text = decryptItemBody(it, vault.masterKey, vault.publicKey, vault.privateKey);
      const parsed = JSON.parse(text) as Partial<ListPayload>;
      // Ensure every entry has a stable id (older serialisations might miss it).
      const fixed = (parsed.entries ?? []).map((e) => ({
        id: e.id ?? newId(),
        text: e.text ?? '',
        done: !!e.done
      }));
      return { entries: fixed };
    } catch {
      return emptyPayload();
    }
  }

  function scheduleSave() {
    if (trashed) return;
    dirty = true;
    if (saveTimer) clearTimeout(saveTimer);
    saveTimer = setTimeout(saveNow, SAVE_DEBOUNCE_MS);
  }

  async function saveNow() {
    if (saving || !dirty) return;
    if (!vault.masterKey) {
      error = 'vault locked — cannot save';
      return;
    }
    saving = true;
    const titleSnap = title;
    const payloadSnap = JSON.stringify({ entries });
    const tagsSnap = parseTagInput(tagsInput);
    const pathSnap = normalizePath(pathInput);
    dirty = false;
    try {
      if (!vault.publicKey || !vault.privateKey) throw new Error('vault locked');
      const wrap = await encryptBodyForSpace({
        body: payloadSnap,
        spaceId: item.space_id,
        masterKey: vault.masterKey,
        publicKey: vault.publicKey,
        privateKey: vault.privateKey,
        item
      });
      const updated = await api.updateItem(item.id, {
        title: titleSnap,
        tags: tagsSnap,
        path: pathSnap,
        update_body: true,
        ...wrap
      });
      item = updated;
      lastSavedAt = new Date(updated.updated_at);
      items.upsert(updated);
    } catch (err) {
      dirty = true;
      error = err instanceof Error ? err.message : 'save failed';
    } finally {
      saving = false;
      if (dirty) {
        if (saveTimer) clearTimeout(saveTimer);
        saveTimer = setTimeout(saveNow, SAVE_DEBOUNCE_MS);
      }
    }
  }

  // --- Entry mutations ---

  async function addEntry(after?: number) {
    const e: Entry = { id: newId(), text: '', done: false };
    if (after === undefined) entries = [...entries, e];
    else entries = [...entries.slice(0, after + 1), e, ...entries.slice(after + 1)];
    scheduleSave();
    await tick();
    inputRefs[e.id]?.focus();
  }

  async function removeEntry(idx: number) {
    if (idx < 0 || idx >= entries.length) return;
    const wasFirst = idx === 0;
    const focusTargetId = wasFirst ? entries[1]?.id : entries[idx - 1]?.id;
    entries = [...entries.slice(0, idx), ...entries.slice(idx + 1)];
    scheduleSave();
    if (focusTargetId) {
      await tick();
      inputRefs[focusTargetId]?.focus();
    }
  }

  function toggleDone(idx: number) {
    if (idx < 0 || idx >= entries.length) return;
    entries = entries.map((e, i) => (i === idx ? { ...e, done: !e.done } : e));
    scheduleSave();
  }

  function clearDone() {
    if (!entries.some((e) => e.done)) return;
    if (!confirm('remove all done items?')) return;
    entries = entries.filter((e) => !e.done);
    scheduleSave();
  }

  function onEntryInput(idx: number, e: Event) {
    const value = (e.target as HTMLInputElement).value;
    entries = entries.map((it, i) => (i === idx ? { ...it, text: value } : it));
    scheduleSave();
  }

  function onEntryKeydown(idx: number, e: KeyboardEvent) {
    if (e.key === 'Enter') {
      e.preventDefault();
      addEntry(idx);
    } else if (e.key === 'Backspace' && entries[idx].text === '' && entries.length > 1) {
      e.preventDefault();
      removeEntry(idx);
    }
  }

  // --- Drag-reorder (svelte-dnd-action) ---

  type DndEvent = CustomEvent<{ items: Entry[]; info: { source: string; trigger: string } }>;
  let dragDisabled = $state(true);

  function onConsider(e: DndEvent) {
    entries = e.detail.items;
  }
  function onFinalize(e: DndEvent) {
    entries = e.detail.items;
    if (e.detail.info.source === SOURCES.POINTER) {
      dragDisabled = true;
    }
    scheduleSave();
  }
  function startDrag() {
    dragDisabled = false;
  }
  function handleKeyToggleDrag(e: KeyboardEvent) {
    if (e.key === 'Enter' || e.key === ' ') {
      e.preventDefault();
      dragDisabled = !dragDisabled;
    }
  }

  // --- Header / lifecycle ---

  async function del() {
    if (!confirm('move this list to trash?')) return;
    try {
      await api.deleteItem(item.id);
      items.remove(item.id);
      goto('/items', { replaceState: true });
    } catch (err) {
      error = err instanceof Error ? err.message : 'delete failed';
    }
  }
  async function restoreItem() {
    try {
      const restored = await api.restoreItem(item.id);
      items.remove(item.id);
      goto(`/items/${restored.id}`, { replaceState: true });
    } catch (err) {
      error = err instanceof Error ? err.message : 'restore failed';
    }
  }
  async function hardDelete() {
    if (!confirm('permanently delete this list?')) return;
    try {
      await api.deleteItem(item.id, { hard: true });
      items.remove(item.id);
      goto('/items', { replaceState: true });
    } catch (err) {
      error = err instanceof Error ? err.message : 'delete failed';
    }
  }

  function onTitleInput(e: Event) {
    title = (e.target as HTMLInputElement).value;
    scheduleSave();
  }
  function onTagsInput(e: Event) {
    tagsInput = (e.target as HTMLInputElement).value;
    scheduleSave();
  }
  function onPathInput(e: Event) {
    pathInput = (e.target as HTMLInputElement).value;
    scheduleSave();
  }

  let savedLabel = $derived.by(() => {
    if (saving) return 'saving…';
    if (dirty) return 'unsaved';
    if (!lastSavedAt) return '';
    const diff = (Date.now() - lastSavedAt.getTime()) / 1000;
    if (diff < 5) return 'saved';
    if (diff < 60) return `saved ${Math.floor(diff)}s ago`;
    return `saved ${lastSavedAt.toLocaleTimeString()}`;
  });
</script>

{#if trashed}
  <div class="trash-banner row">
    <span class="grow">in trash · changes disabled</span>
    <button type="button" onclick={restoreItem}>restore</button>
    <button type="button" class="danger" onclick={hardDelete}>delete forever</button>
  </div>
{/if}
<div class="head row" class:dimmed={trashed}>
  <input
    class="title-input grow"
    type="text"
    placeholder="list title"
    value={title}
    oninput={onTitleInput}
    readonly={trashed}
  />
  {#if progress.total > 0}
    <span class="muted progress">{progress.done}/{progress.total}</span>
  {/if}
  {#if !trashed}
    <button class="danger" onclick={del}>delete</button>
  {/if}
</div>
<div class="meta-row row" class:dimmed={trashed}>
  <input
    class="meta-input"
    type="text"
    placeholder="tags"
    value={tagsInput}
    oninput={onTagsInput}
    autocomplete="off"
    readonly={trashed}
  />
  <input
    class="meta-input path"
    type="text"
    placeholder="/path"
    value={pathInput}
    oninput={onPathInput}
    autocomplete="off"
    readonly={trashed}
  />
</div>

<div class="body">
  <div class="entries">
    <ul
      class="dnd-zone"
      use:dndzone={{
        items: entries,
        flipDurationMs: 180,
        dragDisabled,
        dropTargetStyle: {}
      }}
      onconsider={onConsider}
      onfinalize={onFinalize}
    >
      {#each entries as entry, i (entry.id)}
        <li class="entry-row" class:done={entry.done}>
          <button
            type="button"
            class="grip"
            aria-label="reorder"
            onpointerdown={startDrag}
            onkeydown={handleKeyToggleDrag}
            disabled={trashed}
          >
            <GripVertical size={14} />
          </button>
          <TaskCheckbox done={entry.done} disabled={trashed} onToggle={() => toggleDone(i)} />
          <input
            class="entry-input"
            class:done-text={entry.done}
            type="text"
            placeholder="item"
            value={entry.text}
            oninput={(e) => onEntryInput(i, e)}
            onkeydown={(e) => onEntryKeydown(i, e)}
            bind:this={inputRefs[entry.id]}
            readonly={trashed}
          />
          {#if !trashed}
            <button
              type="button"
              class="entry-x"
              aria-label="remove item"
              onclick={() => removeEntry(i)}
            >
              <X size={14} />
            </button>
          {/if}
        </li>
      {/each}
    </ul>
    {#if !trashed}
      <div class="entry-actions row">
        <button type="button" class="add" onclick={() => addEntry()}>+ add item</button>
        <span class="grow"></span>
        {#if progress.done > 0}
          <button type="button" class="link" onclick={clearDone}>
            clear {progress.done} done
          </button>
        {/if}
      </div>
    {/if}
  </div>
</div>

<div class="status row">
  {#if error}
    <span class="danger">{error}</span>
  {:else}
    <span class="muted">{savedLabel}</span>
  {/if}
  <span class="grow"></span>
  <span class="muted">e2e · list · {item.id.slice(0, 8)}</span>
</div>

<style>
  .head {
    height: 32px;
    padding: 0 8px;
    border-bottom: 1px solid var(--border);
    gap: 6px;
  }
  .title-input {
    border: none;
    background: transparent;
    font-weight: 600;
    padding: 0 4px;
  }
  .title-input:focus { outline: none; }
  .progress {
    font-family: var(--font-mono);
    font-size: 12px;
  }
  .meta-row {
    height: 26px;
    padding: 0 8px;
    border-bottom: 1px solid var(--border);
    gap: 8px;
  }
  .meta-input {
    flex: 1 1 auto;
    border: none;
    background: transparent;
    font-size: 12px;
    color: var(--muted);
    padding: 0 4px;
    min-width: 0;
  }
  .meta-input:focus { outline: none; color: var(--fg); }
  .meta-input.path { flex: 0 1 200px; }
  .body { flex: 1; overflow: auto; }
  .entries {
    padding: 12px 16px;
    max-width: 720px;
  }
  .dnd-zone {
    list-style: none;
    margin: 0;
    padding: 0;
    display: flex;
    flex-direction: column;
    gap: 2px;
  }
  .entry-row {
    display: flex;
    align-items: center;
    gap: 6px;
    padding: 4px 0;
  }
  .grip {
    border: none;
    background: transparent;
    color: var(--muted);
    padding: 2px;
    cursor: grab;
    touch-action: none;
    display: inline-flex;
  }
  .grip:hover {
    color: var(--fg);
  }
  .grip:active {
    cursor: grabbing;
  }
  .grip:disabled {
    opacity: 0.4;
    cursor: default;
  }
  .entry-input {
    flex: 1 1 auto;
    border: none;
    background: transparent;
    padding: 4px 6px;
    border-radius: 0;
    min-width: 0;
  }
  .entry-input:focus {
    outline: 1px solid var(--accent);
    outline-offset: -1px;
  }
  .entry-input.done-text {
    text-decoration: line-through;
    color: var(--muted);
  }
  .entry-x {
    border: none;
    background: transparent;
    color: var(--muted);
    padding: 2px;
    cursor: pointer;
    display: inline-flex;
  }
  .entry-x:hover { color: var(--danger); }
  .entry-actions {
    margin-top: 12px;
    gap: 12px;
  }
  .add {
    background: transparent;
    color: var(--accent);
    border: 1px dashed var(--border);
    padding: 4px 12px;
  }
  .add:hover { border-color: var(--accent); }
  .link {
    border: none;
    background: none;
    padding: 0;
    color: var(--muted);
    cursor: pointer;
    font-size: 12px;
  }
  .link:hover { color: var(--fg); }
  .status {
    height: 22px; padding: 0 8px;
    border-top: 1px solid var(--border); font-size: 12px;
  }
  .trash-banner {
    background: rgba(127, 127, 127, 0.1);
    border-bottom: 1px solid var(--border);
    padding: 4px 8px; gap: 8px; font-size: 12px;
  }
  .dimmed { opacity: 0.7; }
  .entry-row.done .entry-input {
    text-decoration: line-through;
    color: var(--muted);
  }
</style>
