<script lang="ts">
  import { onDestroy } from 'svelte';
  import { goto } from '$app/navigation';
  import { api, type Item } from '$lib/api';
  import {
    decryptFileMeta,
    downloadFileBytes,
    humanSize,
    type FileMeta
  } from '$lib/files';
  import {
    formatTagInput,
    items,
    normalizePath,
    parseTagInput
  } from '$lib/items.svelte';
  import { vault } from '$lib/vault.svelte';

  type Props = { item: Item };
  let { item: initial }: Props = $props();

  // svelte-ignore state_referenced_locally
  let item = $state<Item>(initial);
  // svelte-ignore state_referenced_locally
  let title = $state(initial.title);
  // svelte-ignore state_referenced_locally
  let tagsInput = $state(formatTagInput(initial.tags));
  // svelte-ignore state_referenced_locally
  let pathInput = $state(initial.path);
  // svelte-ignore state_referenced_locally
  let meta = $state<FileMeta>(safeDecryptMeta(initial));
  let error = $state('');
  let saving = $state(false);
  let dirty = $state(false);
  let saveTimer: ReturnType<typeof setTimeout> | null = null;
  const SAVE_DEBOUNCE_MS = 600;

  let trashed = $derived(!!item.deleted_at);

  /// Object URL for preview, freed when the item changes or on destroy.
  let previewUrl = $state<string | null>(null);
  let previewKind = $state<'image' | 'pdf' | 'text' | 'none'>('none');
  let previewError = $state('');
  let previewLoading = $state(false);
  let previewText = $state('');

  function safeDecryptMeta(it: Item): FileMeta {
    if (!vault.masterKey || !vault.publicKey || !vault.privateKey) {
      return { filename: it.title, mime: 'application/octet-stream', size: 0 };
    }
    try {
      return decryptFileMeta(it, vault.masterKey, vault.publicKey, vault.privateKey);
    } catch {
      return { filename: it.title, mime: 'application/octet-stream', size: 0 };
    }
  }

  $effect(() => {
    if (initial.id !== item.id) {
      item = initial;
      title = initial.title;
      tagsInput = formatTagInput(initial.tags);
      pathInput = initial.path;
      meta = safeDecryptMeta(initial);
      dirty = false;
      error = '';
      releasePreview();
      previewKind = pickPreviewKind(meta.mime);
    }
  });

  $effect(() => {
    // Initial preview kind on mount.
    void item.id;
    previewKind = pickPreviewKind(meta.mime);
  });

  function pickPreviewKind(mime: string): typeof previewKind {
    if (mime.startsWith('image/')) return 'image';
    if (mime === 'application/pdf') return 'pdf';
    if (mime.startsWith('text/') || mime === 'application/json') return 'text';
    return 'none';
  }

  function releasePreview() {
    if (previewUrl) {
      URL.revokeObjectURL(previewUrl);
      previewUrl = null;
    }
    previewText = '';
    previewError = '';
  }

  onDestroy(() => {
    releasePreview();
    if (saveTimer) clearTimeout(saveTimer);
  });

  async function loadPreview() {
    if (!vault.masterKey || !vault.publicKey || !vault.privateKey) {
      previewError = 'vault locked';
      return;
    }
    previewLoading = true;
    previewError = '';
    try {
      const bytes = await downloadFileBytes(item, vault.masterKey, vault.publicKey, vault.privateKey);
      if (previewKind === 'text') {
        previewText = new TextDecoder().decode(bytes);
      } else {
        const blob = new Blob([bytes as BlobPart], { type: meta.mime });
        previewUrl = URL.createObjectURL(blob);
      }
    } catch (err) {
      previewError = err instanceof Error ? err.message : 'preview failed';
    } finally {
      previewLoading = false;
    }
  }

  async function downloadAndSave() {
    if (!vault.masterKey || !vault.publicKey || !vault.privateKey) {
      error = 'vault locked';
      return;
    }
    try {
      const bytes = await downloadFileBytes(item, vault.masterKey, vault.publicKey, vault.privateKey);
      const blob = new Blob([bytes as BlobPart], { type: meta.mime });
      const url = URL.createObjectURL(blob);
      const a = document.createElement('a');
      a.href = url;
      a.download = meta.filename;
      a.click();
      setTimeout(() => URL.revokeObjectURL(url), 5000);
    } catch (err) {
      error = err instanceof Error ? err.message : 'download failed';
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
    saving = true;
    const titleSnap = title;
    const tagsSnap = parseTagInput(tagsInput);
    const pathSnap = normalizePath(pathInput);
    dirty = false;
    try {
      const updated = await api.updateItem(item.id, {
        title: titleSnap,
        tags: tagsSnap,
        path: pathSnap
      });
      item = updated;
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
    if (!confirm('move this file to trash?')) return;
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
    if (!confirm('permanently delete this file? bytes are unrecoverable.')) return;
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
  <button type="button" onclick={downloadAndSave}>download</button>
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
  <div class="meta">
    <dl>
      <dt>filename</dt>
      <dd>{meta.filename}</dd>
      <dt>mime</dt>
      <dd>{meta.mime}</dd>
      <dt>size</dt>
      <dd>{humanSize(meta.size)}</dd>
      <dt>blob</dt>
      <dd class="hash">{item.blob_sha256?.slice(0, 16)}…</dd>
    </dl>
  </div>

  <div class="preview">
    {#if previewKind === 'none'}
      <p class="muted">no inline preview for {meta.mime}. use download.</p>
    {:else if previewLoading}
      <p class="muted">decrypting…</p>
    {:else if previewError}
      <p class="danger">{previewError}</p>
    {:else if previewUrl && previewKind === 'image'}
      <img src={previewUrl} alt={meta.filename} />
    {:else if previewUrl && previewKind === 'pdf'}
      <iframe src={previewUrl} title={meta.filename}></iframe>
    {:else if previewKind === 'text' && previewText}
      <pre>{previewText}</pre>
    {:else}
      <button type="button" onclick={loadPreview}>load preview</button>
    {/if}
  </div>
</div>

<div class="status row">
  {#if error}
    <span class="danger">{error}</span>
  {:else if saving}
    <span class="muted">saving…</span>
  {:else if dirty}
    <span class="muted">unsaved</span>
  {/if}
  <span class="grow"></span>
  <span class="muted">e2e · file · {item.id.slice(0, 8)}</span>
</div>

<style>
  .head {
    height: 32px;
    padding: 0 8px;
    border-bottom: 1px solid var(--border);
  }
  .title-input {
    border: none;
    background: transparent;
    font-weight: 600;
    padding: 0 4px;
  }
  .title-input:focus {
    outline: none;
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
  .meta-input:focus {
    outline: none;
    color: var(--fg);
  }
  .meta-input.path {
    flex: 0 1 200px;
  }
  .body {
    flex: 1;
    overflow: auto;
    display: flex;
    flex-direction: column;
  }
  .meta {
    padding: 12px 16px;
    border-bottom: 1px solid var(--border);
  }
  .meta dl {
    display: grid;
    grid-template-columns: 80px 1fr;
    gap: 4px 12px;
    margin: 0;
  }
  .meta dt {
    color: var(--muted);
    font-size: 12px;
  }
  .meta dd {
    margin: 0;
    word-break: break-all;
  }
  .meta .hash {
    font-family: var(--font-mono);
    font-size: 11px;
    color: var(--muted);
  }
  .preview {
    flex: 1;
    padding: 12px 16px;
    display: flex;
    flex-direction: column;
    align-items: stretch;
    overflow: auto;
  }
  .preview img {
    max-width: 100%;
    max-height: 500px;
    object-fit: contain;
    align-self: flex-start;
  }
  .preview iframe {
    width: 100%;
    height: 600px;
    border: 1px solid var(--border);
  }
  .preview pre {
    background: rgba(127, 127, 127, 0.08);
    padding: 8px;
    overflow: auto;
    margin: 0;
    max-height: 500px;
  }
  .status {
    height: 22px;
    padding: 0 8px;
    border-top: 1px solid var(--border);
    font-size: 12px;
  }
  .trash-banner {
    background: rgba(127, 127, 127, 0.1);
    border-bottom: 1px solid var(--border);
    padding: 4px 8px;
    gap: 8px;
    font-size: 12px;
  }
  .dimmed {
    opacity: 0.7;
  }
</style>
