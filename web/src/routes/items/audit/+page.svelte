<script lang="ts">
  import { onMount } from 'svelte';
  import { goto } from '$app/navigation';
  import { api, ApiError, type AuditEntry } from '$lib/api';

  let entries = $state<AuditEntry[]>([]);
  let loading = $state(true);
  let error = $state('');

  onMount(async () => {
    try {
      entries = await api.listAuditLog(200);
    } catch (err) {
      error =
        err instanceof ApiError ? err.message : err instanceof Error ? err.message : 'load failed';
    } finally {
      loading = false;
    }
  });

  function fmtTime(s: string): string {
    const d = new Date(s);
    const today = new Date();
    if (d.toDateString() === today.toDateString()) {
      return d.toLocaleTimeString(undefined, { hour: '2-digit', minute: '2-digit', second: '2-digit' });
    }
    return d.toLocaleString(undefined, {
      month: 'short',
      day: 'numeric',
      hour: '2-digit',
      minute: '2-digit'
    });
  }

  function category(action: string): 'auth' | 'item' | 'secret' | 'other' {
    if (action.startsWith('auth.')) return 'auth';
    if (action.startsWith('secret.')) return 'secret';
    if (action.startsWith('item.')) return 'item';
    return 'other';
  }

  function describe(e: AuditEntry): string {
    const meta = e.meta ?? {};
    switch (e.action) {
      case 'auth.register':
        return 'registered account';
      case 'auth.login':
        return `signed in (${(meta.method as string) ?? 'unknown'})`;
      case 'auth.logout':
        return 'signed out';
      case 'auth.passphrase_reset':
        return 'reset passphrase via recovery code';
      case 'auth.passkey_register':
        return `registered passkey ${meta.name ? `"${meta.name}"` : ''}`.trim();
      case 'item.create':
        return `created ${(meta.type as string) ?? 'item'}`;
      case 'item.update':
        return `updated ${(meta.type as string) ?? 'item'}`;
      case 'item.soft_delete':
        return 'moved to trash';
      case 'item.hard_delete':
        return 'permanently deleted';
      case 'item.restore':
        return 'restored from trash';
      case 'secret.read':
        return 'read secret';
      default:
        return e.action;
    }
  }
</script>

<div class="audit">
  <div class="audit-head row">
    <button type="button" onclick={() => goto('/items')}>← back</button>
    <span class="grow"></span>
    <span class="muted">last 200 entries</span>
  </div>

  {#if loading}
    <div class="muted" style="padding: 16px;">…</div>
  {:else if error}
    <div class="danger" style="padding: 16px;">{error}</div>
  {:else if entries.length === 0}
    <div class="muted" style="padding: 16px;">no activity yet.</div>
  {:else}
    <table class="log">
      <thead>
        <tr>
          <th class="when">when</th>
          <th class="action">action</th>
          <th class="target">target</th>
        </tr>
      </thead>
      <tbody>
        {#each entries as e (e.id)}
          {@const cat = category(e.action)}
          <tr class="entry cat-{cat}">
            <td class="when muted">{fmtTime(e.ts)}</td>
            <td class="action">{describe(e)}</td>
            <td class="target">
              {#if e.item_id && e.item_title}
                <a href={`/items/${e.item_id}`} class="target-link">
                  {e.item_title}
                </a>
                {#if e.item_type}<span class="muted">· {e.item_type}</span>{/if}
              {:else if e.item_id}
                <span class="muted">item gone</span>
              {/if}
            </td>
          </tr>
        {/each}
      </tbody>
    </table>
  {/if}
</div>

<style>
  .audit {
    display: flex;
    flex-direction: column;
    height: 100%;
    overflow: hidden;
  }
  .audit-head {
    height: 32px;
    padding: 0 8px;
    border-bottom: 1px solid var(--border);
    gap: 8px;
  }
  .log {
    border-collapse: collapse;
    width: 100%;
    overflow: auto;
    flex: 1;
    display: block;
  }
  .log thead {
    position: sticky;
    top: 0;
    background: var(--bg);
    border-bottom: 1px solid var(--border);
  }
  .log th {
    text-align: left;
    padding: 6px 12px;
    font-weight: normal;
    color: var(--muted);
    font-size: 11px;
    text-transform: uppercase;
    letter-spacing: 0.05em;
  }
  .log td {
    padding: 4px 12px;
    border-bottom: 1px solid var(--border);
    vertical-align: top;
  }
  .when {
    width: 160px;
    white-space: nowrap;
    font-family: var(--font-mono);
    font-size: 12px;
  }
  .action {
    width: 220px;
  }
  .target {
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .target-link {
    color: var(--accent);
  }
  .cat-secret .action {
    color: var(--accent);
  }
</style>
