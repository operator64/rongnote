<script lang="ts">
  import { goto } from '$app/navigation';
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

  type TaskPayload = { description: string };

  function emptyPayload(): TaskPayload {
    return { description: '' };
  }

  type Props = { item: Item };
  let { item: initial }: Props = $props();

  // svelte-ignore state_referenced_locally
  let item = $state<Item>(initial);
  // svelte-ignore state_referenced_locally
  let title = $state(initial.title);
  // svelte-ignore state_referenced_locally
  let done = $state(initial.done);
  // svelte-ignore state_referenced_locally
  let dueAt = $state<string>(initial.due_at ?? '');
  // svelte-ignore state_referenced_locally
  let payload = $state<TaskPayload>(decryptPayload(initial));
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

  $effect(() => {
    if (initial.id !== item.id) {
      item = initial;
      title = initial.title;
      done = initial.done;
      dueAt = initial.due_at ?? '';
      payload = decryptPayload(initial);
      tagsInput = formatTagInput(initial.tags);
      pathInput = initial.path;
      dirty = false;
      error = '';
      lastSavedAt = new Date(initial.updated_at);
    }
  });

  /// Sync externally-driven changes (e.g. quick-toggle from the list) back
  /// into the editor's local state. The store's summary is the source of
  /// truth for fields that live outside encrypted_body.
  ///
  /// Skip while the user is actively editing — otherwise a $effect-pass
  /// triggered by the local change reads the still-stale summary and
  /// reverts the just-picked value.
  $effect(() => {
    if (saving || dirty) return;
    const summary = items.list.find((n) => n.id === item.id);
    if (!summary) return;
    if (summary.done !== done) done = summary.done;
    const sumDue = summary.due_at ?? '';
    if (sumDue !== dueAt) dueAt = sumDue;
  });

  function decryptPayload(it: Item): TaskPayload {
    if (!it.encrypted_body || !it.wrapped_item_key) return emptyPayload();
    if (!vault.masterKey || !vault.publicKey || !vault.privateKey) return emptyPayload();
    try {
      const text = decryptItemBody(it, vault.masterKey, vault.publicKey, vault.privateKey);
      const parsed = JSON.parse(text) as Partial<TaskPayload>;
      return { ...emptyPayload(), ...parsed };
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
    const doneSnap = done;
    const dueSnap = dueAt.trim() === '' ? null : dueAt;
    const payloadSnap = JSON.stringify(payload);
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
        ...wrap,
        update_due_at: true,
        due_at: dueSnap,
        done: doneSnap
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

  async function del() {
    if (!confirm('move this task to trash?')) return;
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
    if (!confirm('permanently delete this task?')) return;
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
  function onToggleDoneInHead() {
    if (trashed) return;
    done = !done;
    scheduleSave();
  }
  function onDueChange() {
    scheduleSave();
  }
  function onDescChange(_: Event) {
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
  <TaskCheckbox {done} disabled={trashed} size={18} onToggle={onToggleDoneInHead} />
  <input
    class="title-input grow"
    class:done-title={done}
    type="text"
    placeholder="task"
    value={title}
    oninput={onTitleInput}
    readonly={trashed}
  />
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
  <div class="form">
    <label class="field">
      <span class="lbl">due date (optional)</span>
      <input
        type="date"
        bind:value={dueAt}
        onchange={onDueChange}
        disabled={trashed}
      />
    </label>
    <label class="field grow-field">
      <span class="lbl">notes</span>
      <textarea
        rows="10"
        spellcheck="false"
        bind:value={payload.description}
        oninput={onDescChange}
        readonly={trashed}
      ></textarea>
    </label>
  </div>
</div>

<div class="status row">
  {#if error}
    <span class="danger">{error}</span>
  {:else}
    <span class="muted">{savedLabel}</span>
  {/if}
  <span class="grow"></span>
  <span class="muted">e2e · task · {item.id.slice(0, 8)}</span>
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
  .done-title {
    text-decoration: line-through;
    color: var(--muted);
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
  .form {
    padding: 12px 16px;
    display: flex;
    flex-direction: column;
    gap: 12px;
    max-width: 640px;
    height: 100%;
    box-sizing: border-box;
  }
  .field { display: flex; flex-direction: column; gap: 4px; }
  .field input, .field textarea { width: 100%; }
  .lbl { font-size: 11px; color: var(--muted); }
  .grow-field { flex: 1 1 auto; }
  .grow-field textarea { flex: 1 1 auto; height: 100%; min-height: 200px; }
  textarea {
    font: inherit; color: inherit; background: var(--bg);
    border: 1px solid var(--border); padding: 4px 8px; border-radius: 0;
    resize: vertical;
  }
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
</style>
