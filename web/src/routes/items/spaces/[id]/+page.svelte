<script lang="ts">
  import { onMount } from 'svelte';
  import { goto } from '$app/navigation';
  import { page } from '$app/stores';
  import { Plus, Trash2 } from '@lucide/svelte';
  import { api, ApiError, type Member, type Space } from '$lib/api';
  import { spaces } from '$lib/spaces.svelte';

  let spaceId = $derived($page.params.id as string);
  let space = $state<Space | null>(null);
  let members = $state<Member[]>([]);
  let loading = $state(true);
  let error = $state('');

  let inviting = $state(false);
  let inviteEmail = $state('');
  let inviteRole = $state<'editor' | 'viewer'>('editor');
  let busy = $state(false);

  let isOwner = $derived(space?.role === 'owner');

  onMount(load);

  async function load() {
    loading = true;
    error = '';
    try {
      const [s, mlist] = await Promise.all([
        api.getSpace(spaceId),
        api.listMembers(spaceId)
      ]);
      space = s;
      members = mlist;
      spaces.upsert(s);
    } catch (err) {
      error =
        err instanceof ApiError ? err.message : err instanceof Error ? err.message : 'load failed';
    } finally {
      loading = false;
    }
  }

  async function invite() {
    const email = inviteEmail.trim().toLowerCase();
    if (!email) return;
    busy = true;
    error = '';
    try {
      const m = await api.addMember(spaceId, email, inviteRole);
      members = [...members, m];
      if (space) {
        space = { ...space, member_count: space.member_count + 1 };
        spaces.upsert(space);
      }
      inviteEmail = '';
      inviting = false;
    } catch (err) {
      error =
        err instanceof ApiError ? err.message : err instanceof Error ? err.message : 'invite failed';
    } finally {
      busy = false;
    }
  }

  async function changeRole(m: Member, role: 'editor' | 'viewer') {
    if (m.role === 'owner') return;
    busy = true;
    error = '';
    try {
      await api.setMemberRole(spaceId, m.user_id, role);
      members = members.map((x) => (x.user_id === m.user_id ? { ...x, role } : x));
    } catch (err) {
      error =
        err instanceof ApiError ? err.message : err instanceof Error ? err.message : 'update failed';
    } finally {
      busy = false;
    }
  }

  async function remove(m: Member) {
    if (m.role === 'owner') return;
    if (!confirm(`Remove ${m.email} from "${space?.name}"?`)) return;
    busy = true;
    error = '';
    try {
      await api.removeMember(spaceId, m.user_id);
      members = members.filter((x) => x.user_id !== m.user_id);
      if (space) {
        space = { ...space, member_count: Math.max(0, space.member_count - 1) };
        spaces.upsert(space);
      }
    } catch (err) {
      error =
        err instanceof ApiError ? err.message : err instanceof Error ? err.message : 'remove failed';
    } finally {
      busy = false;
    }
  }

  function fmtDate(s: string): string {
    const d = new Date(s);
    return d.toLocaleDateString(undefined, { year: 'numeric', month: 'short', day: 'numeric' });
  }
</script>

<div class="members">
  <div class="head row">
    <button type="button" onclick={() => goto('/items/spaces')}>← back</button>
    <span class="grow">
      {#if space}<strong>{space.name}</strong> <span class="muted">· {space.role}</span>{/if}
    </span>
    {#if isOwner && !inviting}
      <button type="button" onclick={() => (inviting = true)}>
        <Plus size={14} /> invite
      </button>
    {/if}
  </div>

  {#if inviting}
    <form
      class="invite row"
      onsubmit={(e) => {
        e.preventDefault();
        void invite();
      }}
    >
      <!-- svelte-ignore a11y_autofocus -->
      <input
        type="email"
        placeholder="email of an existing user"
        bind:value={inviteEmail}
        disabled={busy}
        autofocus
      />
      <select bind:value={inviteRole} disabled={busy}>
        <option value="editor">editor</option>
        <option value="viewer">viewer</option>
      </select>
      <button type="submit" disabled={busy || !inviteEmail.trim()}>send</button>
      <button
        type="button"
        onclick={() => {
          inviting = false;
          inviteEmail = '';
        }}>cancel</button
      >
    </form>
  {/if}

  {#if error}
    <div class="danger" style="padding: 8px 12px;">{error}</div>
  {/if}

  {#if loading}
    <div class="muted" style="padding: 16px;">…</div>
  {:else if members.length === 0}
    <div class="muted" style="padding: 16px;">no members yet.</div>
  {:else}
    <table class="tbl">
      <thead>
        <tr>
          <th>email</th>
          <th class="role">role</th>
          <th class="joined">joined</th>
          <th class="actions"></th>
        </tr>
      </thead>
      <tbody>
        {#each members as m (m.user_id)}
          <tr>
            <td>{m.email}</td>
            <td class="role">
              {#if isOwner && m.role !== 'owner'}
                <select
                  value={m.role}
                  disabled={busy}
                  onchange={(e) =>
                    void changeRole(m, (e.currentTarget as HTMLSelectElement).value as 'editor' | 'viewer')}
                >
                  <option value="editor">editor</option>
                  <option value="viewer">viewer</option>
                </select>
              {:else}
                {m.role}
              {/if}
            </td>
            <td class="joined muted">{fmtDate(m.joined_at)}</td>
            <td class="actions">
              {#if isOwner && m.role !== 'owner'}
                <button
                  type="button"
                  class="danger-btn"
                  onclick={() => remove(m)}
                  disabled={busy}
                  title="remove"
                >
                  <Trash2 size={14} />
                </button>
              {/if}
            </td>
          </tr>
        {/each}
      </tbody>
    </table>
  {/if}

  <p class="hint muted">
    Phase A: members can be managed but item content is not yet shared. Phase B will seal each
    item's key per member.
  </p>
</div>

<style>
  .members {
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
  .invite {
    padding: 8px;
    gap: 6px;
    border-bottom: 1px solid var(--border);
  }
  .invite input {
    flex: 1;
  }
  .tbl {
    border-collapse: collapse;
    width: 100%;
  }
  .tbl th {
    text-align: left;
    padding: 6px 12px;
    font-weight: normal;
    color: var(--muted);
    font-size: 11px;
    text-transform: uppercase;
    letter-spacing: 0.05em;
    border-bottom: 1px solid var(--border);
  }
  .tbl td {
    padding: 6px 12px;
    border-bottom: 1px solid var(--border);
  }
  .role {
    width: 140px;
  }
  .joined {
    width: 140px;
    white-space: nowrap;
  }
  .actions {
    width: 60px;
    text-align: right;
  }
  .danger-btn {
    color: var(--danger, #c33);
  }
  .hint {
    padding: 12px;
    font-size: 12px;
  }
</style>
