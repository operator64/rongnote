<script lang="ts">
  import { goto } from '$app/navigation';
  import { api, type Item } from '$lib/api';
  import {
    fromBase64,
    generateItemKey,
    open as openSeal,
    seal,
    toBase64,
    utf8Decode,
    utf8Encode
  } from '$lib/crypto';
  import {
    formatTagInput,
    items,
    normalizePath,
    parseTagInput
  } from '$lib/items.svelte';
  import { vault } from '$lib/vault.svelte';

  type SnippetPayload = {
    language: string;
    code: string;
    description: string;
  };

  function emptyPayload(): SnippetPayload {
    return { language: '', code: '', description: '' };
  }

  type Props = { item: Item };
  let { item: initial }: Props = $props();

  // svelte-ignore state_referenced_locally
  let item = $state<Item>(initial);
  // svelte-ignore state_referenced_locally
  let title = $state(initial.title);
  // svelte-ignore state_referenced_locally
  let payload = $state<SnippetPayload>(decryptPayload(initial));
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

  let copyLabel = $state('copy');
  async function copyCode() {
    if (!payload.code) return;
    try {
      await navigator.clipboard.writeText(payload.code);
      copyLabel = 'copied';
      setTimeout(() => (copyLabel = 'copy'), 1500);
    } catch {
      copyLabel = 'select+copy';
    }
  }

  $effect(() => {
    if (initial.id !== item.id) {
      item = initial;
      title = initial.title;
      payload = decryptPayload(initial);
      tagsInput = formatTagInput(initial.tags);
      pathInput = initial.path;
      dirty = false;
      error = '';
      lastSavedAt = new Date(initial.updated_at);
    }
  });

  function decryptPayload(it: Item): SnippetPayload {
    if (!it.encrypted_body || !it.wrapped_item_key) return emptyPayload();
    if (!vault.masterKey) return emptyPayload();
    try {
      const itemKey = openSeal(fromBase64(it.wrapped_item_key), vault.masterKey);
      const bytes = openSeal(fromBase64(it.encrypted_body), itemKey);
      const parsed = JSON.parse(utf8Decode(bytes)) as Partial<SnippetPayload>;
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
    const payloadSnap = JSON.stringify(payload);
    const tagsSnap = parseTagInput(tagsInput);
    const pathSnap = normalizePath(pathInput);
    dirty = false;
    try {
      const itemKey = generateItemKey();
      const encryptedBody = seal(utf8Encode(payloadSnap), itemKey);
      const wrappedItemKey = seal(itemKey, vault.masterKey);
      const updated = await api.updateItem(item.id, {
        title: titleSnap,
        tags: tagsSnap,
        path: pathSnap,
        update_body: true,
        encrypted_body: toBase64(encryptedBody),
        wrapped_item_key: toBase64(wrappedItemKey)
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
    if (!confirm('move this snippet to trash?')) return;
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
    if (!confirm('permanently delete this snippet?')) return;
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
  function onField(_: Event) {
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
    placeholder="title"
    value={title}
    oninput={onTitleInput}
    readonly={trashed}
  />
  <button type="button" onclick={copyCode}>{copyLabel}</button>
  {#if !trashed}
    <button class="danger" onclick={del}>delete</button>
  {/if}
</div>
<div class="meta-row row" class:dimmed={trashed}>
  <input
    class="meta-input"
    type="text"
    placeholder="language (e.g. sh, py, ts)"
    bind:value={payload.language}
    oninput={onField}
    autocomplete="off"
    spellcheck="false"
    readonly={trashed}
    style="flex: 0 1 200px;"
  />
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
    <label class="field grow-field">
      <span class="lbl">code</span>
      <textarea
        class="code"
        spellcheck="false"
        bind:value={payload.code}
        oninput={onField}
        readonly={trashed}
      ></textarea>
    </label>
    <label class="field">
      <span class="lbl">description</span>
      <textarea
        rows="3"
        spellcheck="false"
        bind:value={payload.description}
        oninput={onField}
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
  <span class="muted">e2e · snippet · {item.id.slice(0, 8)}</span>
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
  .body { flex: 1; overflow: auto; display: flex; }
  .form {
    flex: 1;
    padding: 12px 16px;
    display: flex;
    flex-direction: column;
    gap: 12px;
    height: 100%;
    box-sizing: border-box;
  }
  .field { display: flex; flex-direction: column; gap: 4px; }
  .field textarea { width: 100%; }
  .lbl { font-size: 11px; color: var(--muted); }
  .grow-field { flex: 1 1 auto; }
  .grow-field textarea { flex: 1 1 auto; height: 100%; min-height: 240px; }
  textarea {
    font: inherit; color: inherit; background: var(--bg);
    border: 1px solid var(--border); padding: 4px 8px; border-radius: 0;
    resize: vertical;
  }
  textarea.code {
    font-family: var(--font-mono);
    white-space: pre;
    overflow: auto;
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
