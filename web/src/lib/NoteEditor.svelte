<script lang="ts">
  import { goto } from '$app/navigation';
  import { marked } from 'marked';
  import Editor from '$lib/Editor.svelte';
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

  type Props = { item: Item };
  let { item: initial }: Props = $props();

  // svelte-ignore state_referenced_locally
  let item = $state<Item>(initial);
  // svelte-ignore state_referenced_locally
  let title = $state(initial.title);
  // svelte-ignore state_referenced_locally
  let body = $state(decryptBody(initial));
  // svelte-ignore state_referenced_locally
  let tagsInput = $state(formatTagInput(initial.tags));
  // svelte-ignore state_referenced_locally
  let pathInput = $state(initial.path);
  let error = $state('');
  let saving = $state(false);
  let dirty = $state(false);
  // svelte-ignore state_referenced_locally
  let lastSavedAt = $state<Date | null>(new Date(initial.updated_at));

  let trashed = $derived(!!item.deleted_at);
  let preview = $state(false);

  let saveTimer: ReturnType<typeof setTimeout> | null = null;
  const SAVE_DEBOUNCE_MS = 600;

  $effect(() => {
    if (initial.id !== item.id) {
      item = initial;
      title = initial.title;
      body = decryptBody(initial);
      tagsInput = formatTagInput(initial.tags);
      pathInput = initial.path;
      dirty = false;
      lastSavedAt = new Date(initial.updated_at);
      error = '';
    }
  });

  function decryptBody(n: Item): string {
    if (!n.encrypted_body || !n.wrapped_item_key) return '';
    if (!vault.masterKey) throw new Error('vault locked');
    try {
      const itemKey = openSeal(fromBase64(n.wrapped_item_key), vault.masterKey);
      const bodyBytes = openSeal(fromBase64(n.encrypted_body), itemKey);
      return utf8Decode(bodyBytes);
    } catch (err) {
      console.error('decrypt failed', err);
      return '';
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
    const bodySnap = body;
    const tagsSnap = parseTagInput(tagsInput);
    const pathSnap = normalizePath(pathInput);
    dirty = false;
    try {
      const itemKey = generateItemKey();
      const encryptedBody = seal(utf8Encode(bodySnap), itemKey);
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
    if (!confirm('move this note to trash?')) return;
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
      items.remove(item.id); // drop from trash list view
      goto(`/items/${restored.id}`, { replaceState: true });
    } catch (err) {
      error = err instanceof Error ? err.message : 'restore failed';
    }
  }

  async function hardDelete() {
    if (!confirm('permanently delete this note? this cannot be undone.')) return;
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
  function onBodyChange(next: string) {
    body = next;
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

  function preprocessWikiLinks(md: string): string {
    return md.replace(
      /\[\[([^\[\]\n|]+)(?:\|([^\[\]\n]+))?\]\]/g,
      (_, target: string, alias?: string) => {
        const t = target.trim();
        const display = (alias ?? t).trim();
        const found = items.list.find(
          (n) => n.type === 'note' && n.title.toLowerCase() === t.toLowerCase()
        );
        if (found) return `[${display}](/items/${found.id})`;
        return `*${display}?*`;
      }
    );
  }

  let rendered = $derived.by(() => {
    if (!preview) return '';
    return marked.parse(preprocessWikiLinks(body), { async: false }) as string;
  });

  function onPreviewClick(e: MouseEvent) {
    const a = (e.target as HTMLElement).closest('a');
    if (!a) return;
    const href = a.getAttribute('href') ?? '';
    if (href.startsWith('/items/')) {
      e.preventDefault();
      goto(href);
    }
  }

  function noteTitles(): string[] {
    return items.list
      .filter((n) => n.type === 'note' && n.id !== item.id)
      .map((n) => n.title)
      .filter((t) => t.length > 0);
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
  <button onclick={() => (preview = !preview)}>{preview ? 'edit' : 'preview'}</button>
  {#if !trashed}
    <button class="danger" onclick={del}>delete</button>
  {/if}
</div>
<div class="meta-row row" class:dimmed={trashed}>
  <input
    class="meta-input"
    type="text"
    placeholder="tags (comma-separated, optional #)"
    value={tagsInput}
    oninput={onTagsInput}
    autocomplete="off"
    spellcheck="false"
    readonly={trashed}
  />
  <input
    class="meta-input path"
    type="text"
    placeholder="/path"
    value={pathInput}
    oninput={onPathInput}
    autocomplete="off"
    spellcheck="false"
    readonly={trashed}
  />
</div>
<div class="body">
  {#if preview}
    <!-- svelte-ignore a11y_no_noninteractive_element_interactions -->
    <!-- svelte-ignore a11y_click_events_have_key_events -->
    <article class="md" onclick={onPreviewClick} role="document">
      {@html rendered}
    </article>
  {:else}
    <Editor value={body} onChange={onBodyChange} wikiTitles={noteTitles} />
  {/if}
</div>
<div class="meta row">
  {#if error}<span class="danger">{error}</span>{:else}<span class="muted">{savedLabel}</span>{/if}
  <span class="grow"></span>
  <span class="muted" title="encrypted on this device">e2e · {item.id.slice(0, 8)}</span>
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
    overflow: hidden;
    display: flex;
  }
  .body :global(.cm-host) {
    flex: 1;
  }
  .meta {
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
  .dimmed input[readonly] {
    cursor: default;
  }
  .md {
    flex: 1;
    overflow: auto;
    padding: 12px 16px;
    font-family:
      ui-sans-serif,
      system-ui,
      -apple-system,
      "Segoe UI",
      sans-serif;
    line-height: 1.55;
  }
  .md :global(pre),
  .md :global(code) {
    font-family: var(--font-mono);
  }
  .md :global(pre) {
    background: rgba(127, 127, 127, 0.08);
    padding: 8px;
    overflow: auto;
  }
  .md :global(h1),
  .md :global(h2),
  .md :global(h3) {
    margin: 1em 0 0.4em;
  }
</style>
