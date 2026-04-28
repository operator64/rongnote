<script lang="ts">
  import { onMount } from 'svelte';
  import { goto } from '$app/navigation';
  import { page } from '$app/stores';
  import { Pin } from '@lucide/svelte';
  import CommandPalette from '$lib/CommandPalette.svelte';
  import ItemIcon from '$lib/ItemIcon.svelte';
  import Sidebar from '$lib/Sidebar.svelte';
  import TaskCheckbox from '$lib/TaskCheckbox.svelte';
  import { api, type ItemType } from '$lib/api';
  import { uploadFile } from '$lib/files';
  import { items } from '$lib/items.svelte';
  import { prefs } from '$lib/prefs.svelte';
  import { session } from '$lib/session.svelte';
  import { vault } from '$lib/vault.svelte';

  let { children } = $props();

  onMount(() => items.refresh());

  let activeId = $derived($page.params?.id ?? null);

  let filePicker: HTMLInputElement | undefined = $state();
  let uploadStatus = $state('');
  let dragHover = $state(false);

  /// Sidebar's active type filter decides what "+" creates / how to handle.
  async function newItem() {
    const type: ItemType = items.filter.type ?? 'note';
    if (type === 'file') {
      filePicker?.click();
      return;
    }
    const titleByType: Record<string, string> = {
      note: 'Untitled',
      secret: 'New secret',
      task: 'New task',
      snippet: 'New snippet',
      bookmark: 'New bookmark'
    };
    const item = await api.createItem({
      title: titleByType[type] ?? 'Untitled',
      type
    });
    items.upsert(item);
    await goto(`/items/${item.id}`);
  }

  async function onPickedFiles(e: Event) {
    const input = e.target as HTMLInputElement;
    const fs = input.files;
    if (!fs || fs.length === 0) return;
    await uploadMany(fs);
    input.value = '';
  }

  async function onDrop(e: DragEvent) {
    e.preventDefault();
    dragHover = false;
    const fs = e.dataTransfer?.files;
    if (!fs || fs.length === 0) return;
    await uploadMany(fs);
  }

  function onDragOver(e: DragEvent) {
    if (!e.dataTransfer) return;
    if (Array.from(e.dataTransfer.items).some((it) => it.kind === 'file')) {
      e.preventDefault();
      dragHover = true;
    }
  }
  function onDragLeave() {
    dragHover = false;
  }

  async function uploadMany(fs: FileList) {
    if (!vault.masterKey) {
      uploadStatus = 'vault locked — unlock first';
      return;
    }
    const total = fs.length;
    let done = 0;
    let last;
    for (const file of Array.from(fs)) {
      uploadStatus = `uploading ${file.name} (${++done}/${total})…`;
      try {
        last = await uploadFile({
          file,
          masterKey: vault.masterKey,
          path: items.filter.pathPrefix ?? '/'
        });
      } catch (err) {
        uploadStatus = err instanceof Error ? `failed: ${err.message}` : 'upload failed';
        return;
      }
    }
    uploadStatus = `${total} uploaded`;
    setTimeout(() => (uploadStatus = ''), 2500);
    if (last && total === 1) {
      await goto(`/items/${last.id}`);
    }
  }

  async function logout() {
    vault.lock();
    await session.logout();
    await goto('/login', { replaceState: true });
  }

  function fmtDate(s: string) {
    const d = new Date(s);
    const today = new Date();
    if (d.toDateString() === today.toDateString()) {
      return d.toLocaleTimeString(undefined, { hour: '2-digit', minute: '2-digit' });
    }
    return d.toLocaleDateString();
  }

  function dueClass(due: string): string {
    const today = new Date();
    today.setHours(0, 0, 0, 0);
    const d = new Date(due + 'T00:00:00');
    if (d < today) return 'due overdue';
    if (d.getTime() === today.getTime()) return 'due today';
    return 'due future';
  }
  function fmtDue(due: string): string {
    const today = new Date();
    today.setHours(0, 0, 0, 0);
    const d = new Date(due + 'T00:00:00');
    const diff = Math.round((d.getTime() - today.getTime()) / 86400000);
    if (diff === 0) return 'today';
    if (diff === 1) return 'tmrw';
    if (diff === -1) return 'yest';
    if (diff > 0 && diff < 7) return d.toLocaleDateString(undefined, { weekday: 'short' });
    return d.toLocaleDateString(undefined, { month: 'short', day: 'numeric' });
  }
  async function onToggleDone(e: MouseEvent, id: string) {
    e.preventDefault();
    e.stopPropagation();
    try {
      await items.toggleTaskDone(id);
    } catch (err) {
      console.error('toggle failed', err);
    }
  }

  function themeIcon(t: typeof prefs.theme): string {
    return t === 'dark' ? '●' : t === 'light' ? '○' : '◐';
  }

  let filterLabel = $derived.by(() => {
    if (items.view === 'trash') return 'trash';
    const f = items.filter;
    if (f.type) return f.type + 's';
    if (f.tag) return `#${f.tag}`;
    if (f.pathPrefix && f.pathPrefix !== '/') return f.pathPrefix;
    if (f.q) return `“${f.q}”`;
    return null;
  });

  let inTrash = $derived(items.view === 'trash');
</script>

<div class="shell">
  <Sidebar />

  <section
    class="list-pane"
    class:drag={dragHover}
    role="list"
    ondragover={onDragOver}
    ondragleave={onDragLeave}
    ondrop={onDrop}
  >
    <input
      type="file"
      bind:this={filePicker}
      onchange={onPickedFiles}
      multiple
      style="display: none;"
    />
    <div class="pane-head row">
      {#if inTrash}
        <span class="grow muted">trash</span>
      {:else if filterLabel}
        <button
          class="filter-chip"
          onclick={() => items.clearFilter()}
          title="clear filter"
        >
          {filterLabel} <span class="muted">×</span>
        </button>
        <span class="grow"></span>
      {:else}
        <span class="grow muted">items</span>
      {/if}
      {#if !inTrash}
        <button onclick={newItem} title="new {items.filter.type ?? 'note'}">+</button>
      {/if}
    </div>
    {#if items.loading && items.list.length === 0}
      <div class="muted" style="padding: 8px;">…</div>
    {:else if items.filteredList.length === 0}
      <div class="muted" style="padding: 8px;">
        {items.list.length === 0 ? 'no items yet' : 'nothing matches'}
      </div>
    {:else}
      {#each items.filteredList as n (n.id)}
        {#if n.type === 'task'}
          <div
            class="list-row task-row"
            class:active={n.id === activeId}
            class:task-done={n.done}
          >
            <TaskCheckbox done={n.done} onToggle={(e) => onToggleDone(e, n.id)} />
            <a
              class="task-link grow"
              href={`/items/${n.id}`}
              data-sveltekit-noscroll
            >
              {n.title || '(untitled)'}
            </a>
            {#if n.pinned}<Pin size={12} class="pinned-mark" />{/if}
            {#if n.due_at}
              <span class={dueClass(n.due_at)}>{fmtDue(n.due_at)}</span>
            {/if}
          </div>
        {:else}
          <a
            class="list-row"
            class:active={n.id === activeId}
            href={`/items/${n.id}`}
            data-sveltekit-noscroll
          >
            <ItemIcon type={n.type} />
            <span class="grow" style="overflow: hidden; text-overflow: ellipsis;">
              {n.title || '(untitled)'}
            </span>
            {#if n.pinned}<Pin size={12} class="pinned-mark" />{/if}
            <span class="tag">{fmtDate(n.updated_at)}</span>
          </a>
        {/if}
      {/each}
    {/if}
  </section>

  <main class="detail-pane">
    {@render children()}
  </main>
</div>

<div class="statusbar">
  {#if uploadStatus}
    <span class="muted">{uploadStatus}</span>
    <span class="sep">·</span>
  {/if}
  <span class="grow">{session.user?.email ?? ''}</span>
  <span title="press Ctrl-K">⌘K</span>
  <span>{items.filteredList.length}/{items.list.length}</span>
  <span class="sep">·</span>
  <button
    class="sb-btn"
    title={`font size ${prefs.fontSize}px — click to shrink`}
    onclick={() => prefs.bumpFontSize(-1)}
  >A−</button>
  <span class="muted small">{prefs.fontSize}</span>
  <button
    class="sb-btn"
    title="font size — click to grow"
    onclick={() => prefs.bumpFontSize(+1)}
  >A+</button>
  <button
    class="sb-btn"
    title={`theme: ${prefs.theme} — click to cycle`}
    onclick={() => prefs.cycleTheme()}
  >{themeIcon(prefs.theme)}</button>
  <span class="sep">·</span>
  <button class="link" onclick={logout}>sign out</button>
</div>

<CommandPalette />

<style>
  .shell {
    display: grid;
    grid-template-columns: 200px 280px 1fr;
    height: calc(100vh - 22px);
    border-bottom: 1px solid var(--border);
  }
  .list-pane {
    border-right: 1px solid var(--border);
    overflow-y: auto;
    display: flex;
    flex-direction: column;
    position: relative;
  }
  .list-pane.drag::after {
    content: 'drop to upload';
    position: absolute;
    inset: 0;
    background: rgba(127, 127, 127, 0.15);
    border: 2px dashed var(--accent);
    display: flex;
    align-items: center;
    justify-content: center;
    color: var(--fg);
    pointer-events: none;
    font-size: 14px;
  }
  .detail-pane {
    overflow: hidden;
    display: flex;
    flex-direction: column;
    min-width: 0;
  }
  .pane-head {
    height: 28px;
    padding: 0 8px;
    border-bottom: 1px solid var(--border);
  }
  .filter-chip {
    border: 1px solid var(--border);
    padding: 0 6px;
    height: 20px;
    font-size: 11px;
    background: rgba(127, 127, 127, 0.08);
  }
  .filter-chip:hover {
    border-color: var(--fg);
  }
  .task-row {
    display: flex;
    align-items: center;
    height: var(--row-height);
    padding: 0 8px;
    border-bottom: 1px solid var(--border);
    gap: 6px;
  }
  .task-row:hover {
    background: rgba(127, 127, 127, 0.08);
  }
  .task-row.active {
    background: rgba(127, 127, 127, 0.16);
  }
  .task-link {
    color: inherit;
    text-decoration: none;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .task-link:hover {
    text-decoration: none;
  }
  .task-done .task-link {
    text-decoration: line-through;
    color: var(--muted);
  }
  .due {
    font-size: 11px;
    flex-shrink: 0;
  }
  :global(.pinned-mark) {
    color: var(--accent);
    flex-shrink: 0;
  }
  .due.future {
    color: var(--muted);
  }
  .due.today {
    color: var(--accent);
    font-weight: 600;
  }
  .due.overdue {
    color: var(--danger);
  }
  .link {
    border: none;
    padding: 0;
    background: none;
    color: var(--muted);
  }
  .link:hover {
    color: var(--fg);
  }
  .sb-btn {
    border: none;
    background: none;
    padding: 0 4px;
    color: var(--muted);
    font: inherit;
    cursor: pointer;
  }
  .sb-btn:hover {
    color: var(--fg);
  }
  .sep {
    color: var(--border);
  }
  .small {
    font-size: 11px;
  }
  @media (max-width: 900px) {
    .shell {
      grid-template-columns: 240px 1fr;
    }
    .shell :global(.sidebar) {
      display: none;
    }
  }
  @media (max-width: 700px) {
    .shell {
      grid-template-columns: 1fr;
    }
    .list-pane {
      display: none;
    }
  }
</style>
