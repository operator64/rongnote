<script lang="ts">
  import { type ItemType } from '$lib/api';
  import ItemIcon from '$lib/ItemIcon.svelte';
  import { items } from '$lib/items.svelte';

  async function setAll() {
    if (items.view !== 'active') await items.setView('active');
    items.clearFilter();
  }
  async function setActiveView() {
    await items.setView('active');
  }
  async function setTrashView() {
    await items.setView('trash');
  }
  function setType(type: ItemType) {
    items.setFilter({ type });
  }
  function setTag(tag: string) {
    items.setFilter({ tag });
  }
  function setPath(path: string) {
    items.setFilter({ pathPrefix: path });
  }

  let activeType = $derived(items.filter.type);
  let activeTag = $derived(items.filter.tag);
  let activePath = $derived(items.filter.pathPrefix);
  let inTrash = $derived(items.view === 'trash');
  let isAllActive = $derived(items.view === 'active' && !items.hasActiveFilter);

  function typeLabel(t: ItemType): string {
    return t === 'note' ? 'notes' : t + 's';
  }
</script>

<aside class="sidebar">
  <button class="row-btn" class:active={isAllActive} onclick={setAll}>
    <span class="grow">all items</span>
    {#if !inTrash}<span class="count">{items.list.length}</span>{/if}
  </button>

  {#if !inTrash && items.typeCounts.length > 0}
    <div class="section-head">by type</div>
    {#each items.typeCounts as t (t.type)}
      <button
        class="row-btn"
        class:active={activeType === t.type}
        class:dim={t.count === 0}
        onclick={() => setType(t.type)}
      >
        <ItemIcon type={t.type} />
        <span class="grow">{typeLabel(t.type)}</span>
        <span class="count">{t.count}</span>
      </button>
    {/each}
  {/if}

  {#if !inTrash && items.pathTree.length > 0}
    <div class="section-head">paths</div>
    {#each items.pathTree as p (p.path)}
      <button
        class="row-btn"
        class:active={activePath === p.path}
        class:dim={p.directCount === 0}
        style="padding-left: {8 + p.depth * 12}px;"
        onclick={() => setPath(p.path)}
        title={p.path}
      >
        <span class="grow">{p.name}</span>
        <span class="count">{p.subtreeCount}</span>
      </button>
    {/each}
  {/if}

  {#if !inTrash && items.tagCounts.length > 0}
    <div class="section-head">tags</div>
    {#each items.tagCounts as t (t.tag)}
      <button
        class="row-btn"
        class:active={activeTag === t.tag}
        onclick={() => setTag(t.tag)}
      >
        <span class="grow">#{t.tag}</span>
        <span class="count">{t.count}</span>
      </button>
    {/each}
  {/if}

  <div class="spacer"></div>

  {#if inTrash}
    <button class="row-btn trash-row active" onclick={setActiveView}>
      <span class="grow">← back to items</span>
    </button>
    <button class="row-btn trash-row active">
      <span class="grow">trash</span>
      <span class="count">{items.list.length}</span>
    </button>
  {:else}
    <button class="row-btn trash-row muted" onclick={setTrashView}>
      <span class="grow">trash</span>
    </button>
  {/if}
</aside>

<style>
  .sidebar {
    display: flex;
    flex-direction: column;
    overflow-y: auto;
    border-right: 1px solid var(--border);
    background: var(--bg);
  }
  .section-head {
    padding: 8px 8px 2px;
    color: var(--muted);
    font-size: 11px;
    text-transform: uppercase;
    letter-spacing: 0.05em;
    border-top: 1px solid var(--border);
    margin-top: 4px;
  }
  .row-btn {
    display: flex;
    align-items: center;
    gap: 6px;
    height: 24px;
    padding: 0 8px;
    border: none;
    background: transparent;
    text-align: left;
    color: var(--fg);
    font: inherit;
    cursor: pointer;
    border-radius: 0;
    white-space: nowrap;
    overflow: hidden;
  }
  .row-btn:hover {
    background: rgba(127, 127, 127, 0.08);
  }
  .row-btn.active {
    background: rgba(127, 127, 127, 0.18);
  }
  .row-btn.dim {
    color: var(--muted);
  }
  .spacer {
    flex: 1 1 auto;
    min-height: 12px;
  }
  .trash-row {
    border-top: 1px solid var(--border);
  }
  .trash-row.muted {
    color: var(--muted);
  }
  .count {
    color: var(--muted);
    font-size: 11px;
    flex-shrink: 0;
  }
  .grow {
    flex: 1 1 auto;
    overflow: hidden;
    text-overflow: ellipsis;
  }
</style>
