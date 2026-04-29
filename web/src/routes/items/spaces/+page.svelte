<script lang="ts">
  import { goto } from '$app/navigation';
  import { Home, Users, Plus, Trash2 } from '@lucide/svelte';
  import { api, ApiError, type Space } from '$lib/api';
  import { spaces } from '$lib/spaces.svelte';

  let creating = $state(false);
  let newName = $state('');
  let busy = $state(false);
  let error = $state('');

  async function createSpace() {
    const name = newName.trim();
    if (!name) return;
    busy = true;
    error = '';
    try {
      const space = await api.createSpace(name);
      spaces.upsert(space);
      newName = '';
      creating = false;
    } catch (err) {
      error =
        err instanceof ApiError ? err.message : err instanceof Error ? err.message : 'create failed';
    } finally {
      busy = false;
    }
  }

  async function deleteSpace(s: Space) {
    if (s.kind === 'personal') return;
    if (!confirm(`Delete space "${s.name}"? Items inside will be moved to your personal space.`)) {
      return;
    }
    busy = true;
    error = '';
    try {
      await api.deleteSpace(s.id);
      spaces.remove(s.id);
    } catch (err) {
      error =
        err instanceof ApiError ? err.message : err instanceof Error ? err.message : 'delete failed';
    } finally {
      busy = false;
    }
  }
</script>

<div class="spaces">
  <div class="head row">
    <button type="button" onclick={() => goto('/items')}>← back</button>
    <span class="grow"></span>
    {#if !creating}
      <button type="button" onclick={() => (creating = true)}>
        <Plus size={14} /> new team space
      </button>
    {/if}
  </div>

  {#if creating}
    <form
      class="create row"
      onsubmit={(e) => {
        e.preventDefault();
        void createSpace();
      }}
    >
      <!-- svelte-ignore a11y_autofocus -->
      <input
        type="text"
        placeholder="space name"
        bind:value={newName}
        disabled={busy}
        autofocus
      />
      <button type="submit" disabled={busy || !newName.trim()}>create</button>
      <button
        type="button"
        onclick={() => {
          creating = false;
          newName = '';
        }}>cancel</button
      >
    </form>
  {/if}

  {#if error}
    <div class="danger" style="padding: 8px 12px;">{error}</div>
  {/if}

  {#if spaces.loading && spaces.list.length === 0}
    <div class="muted" style="padding: 16px;">…</div>
  {:else if spaces.list.length === 0}
    <div class="muted" style="padding: 16px;">no spaces yet.</div>
  {:else}
    <ul class="list">
      {#each spaces.list as s (s.id)}
        <li class="row card" class:active={spaces.activeId === s.id}>
          <span class="icon">
            {#if s.kind === 'personal'}
              <Home size={14} />
            {:else}
              <Users size={14} />
            {/if}
          </span>
          <span class="name grow">
            {s.name}
            <span class="muted">· {s.kind === 'personal' ? 'personal' : `team · ${s.role}`}</span>
          </span>
          <span class="muted count">{s.member_count} {s.member_count === 1 ? 'member' : 'members'}</span>
          {#if s.kind === 'team'}
            <button type="button" onclick={() => goto(`/items/spaces/${s.id}`)}>members</button>
            {#if s.role === 'owner'}
              <button
                type="button"
                class="danger-btn"
                onclick={() => deleteSpace(s)}
                disabled={busy}
                title="delete space"
              >
                <Trash2 size={14} />
              </button>
            {/if}
          {/if}
        </li>
      {/each}
    </ul>
  {/if}
</div>

<style>
  .spaces {
    display: flex;
    flex-direction: column;
    height: 100%;
    overflow: auto;
  }
  .head {
    height: 32px;
    padding: 0 8px;
    border-bottom: 1px solid var(--border);
    gap: 8px;
    flex-shrink: 0;
  }
  .create {
    padding: 8px;
    gap: 6px;
    border-bottom: 1px solid var(--border);
  }
  .create input {
    flex: 1;
  }
  .list {
    list-style: none;
    margin: 0;
    padding: 8px;
    display: flex;
    flex-direction: column;
    gap: 4px;
  }
  .card {
    padding: 8px 10px;
    border: 1px solid var(--border);
    border-radius: 4px;
    gap: 8px;
    background: var(--bg);
  }
  .card.active {
    border-color: var(--accent);
  }
  .icon {
    color: var(--muted);
    display: inline-flex;
    align-items: center;
  }
  .count {
    font-size: 12px;
    flex-shrink: 0;
  }
  .danger-btn {
    color: var(--danger, #c33);
  }
</style>
