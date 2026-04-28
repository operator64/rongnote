<script lang="ts">
  import { onMount } from 'svelte';
  import { goto } from '$app/navigation';
  import { page } from '$app/stores';
  import { api, ApiError, type VersionSummary } from '$lib/api';
  import {
    fromBase64,
    open as openSeal,
    utf8Decode
  } from '$lib/crypto';
  import { items } from '$lib/items.svelte';
  import { vault } from '$lib/vault.svelte';

  let id = $derived($page.params.id);
  let versions = $state<VersionSummary[]>([]);
  let loading = $state(true);
  let error = $state('');
  let busy = $state(false);

  /// Decrypted previews keyed by version number.
  let previews = $state<Record<number, string>>({});
  let expanded = $state<number | null>(null);

  onMount(load);

  async function load() {
    if (!id) return;
    loading = true;
    error = '';
    try {
      versions = await api.listVersions(id);
    } catch (err) {
      error = err instanceof ApiError ? err.message : 'load failed';
    } finally {
      loading = false;
    }
  }

  async function expand(v: VersionSummary) {
    if (expanded === v.version) {
      expanded = null;
      return;
    }
    expanded = v.version;
    if (previews[v.version] !== undefined) return;
    if (!vault.masterKey) {
      error = 'vault locked';
      return;
    }
    try {
      const detail = await api.getVersion(id!, v.version);
      let text = '(empty)';
      if (detail.encrypted_body && detail.wrapped_item_key) {
        const ik = openSeal(fromBase64(detail.wrapped_item_key), vault.masterKey);
        text = utf8Decode(openSeal(fromBase64(detail.encrypted_body), ik));
      }
      previews = { ...previews, [v.version]: text };
    } catch (err) {
      error = err instanceof Error ? err.message : 'decrypt failed';
    }
  }

  async function restore(v: VersionSummary) {
    if (
      !confirm(
        `restore version ${v.version} (from ${new Date(v.created_at).toLocaleString()})?\n\nyour current content will be saved as a new version, then replaced.`
      )
    )
      return;
    busy = true;
    error = '';
    try {
      const updated = await api.restoreVersion(id!, v.version);
      items.upsert(updated);
      await goto(`/items/${id}`, { replaceState: true });
    } catch (err) {
      error = err instanceof ApiError ? err.message : 'restore failed';
    } finally {
      busy = false;
    }
  }

  function fmt(s: string): string {
    return new Date(s).toLocaleString();
  }
</script>

<div class="page">
  <div class="head row">
    <button type="button" onclick={() => goto(`/items/${id}`)}>← back</button>
    <span class="grow"></span>
    <span class="muted">version history</span>
  </div>

  {#if loading}
    <div class="muted center">…</div>
  {:else if error}
    <div class="danger center">{error}</div>
  {:else if versions.length === 0}
    <div class="muted center">
      no versions yet — saves create snapshots automatically.
    </div>
  {:else}
    <div class="versions">
      {#each versions as v (v.id)}
        <div class="version">
          <div class="row v-row">
            <span class="vno muted">v{v.version}</span>
            <span class="grow">{v.title || '(untitled)'}</span>
            <span class="when muted">{fmt(v.created_at)}</span>
            <button type="button" onclick={() => expand(v)}>
              {expanded === v.version ? 'hide' : 'preview'}
            </button>
            <button type="button" disabled={busy} onclick={() => restore(v)}>
              restore
            </button>
          </div>
          {#if expanded === v.version}
            <pre class="preview">{previews[v.version] ?? '…'}</pre>
          {/if}
        </div>
      {/each}
    </div>
  {/if}
</div>

<style>
  .page {
    display: flex;
    flex-direction: column;
    height: 100%;
    overflow: hidden;
  }
  .head {
    height: 32px;
    padding: 0 8px;
    border-bottom: 1px solid var(--border);
    gap: 8px;
  }
  .center {
    text-align: center;
    padding: 32px;
  }
  .versions {
    flex: 1;
    overflow: auto;
  }
  .version {
    border-bottom: 1px solid var(--border);
  }
  .v-row {
    padding: 6px 12px;
    gap: 8px;
  }
  .vno {
    font-family: var(--font-mono);
    font-size: 12px;
    width: 40px;
    text-align: right;
  }
  .when {
    font-size: 12px;
    white-space: nowrap;
  }
  .preview {
    background: rgba(127, 127, 127, 0.06);
    padding: 12px;
    margin: 0;
    max-height: 280px;
    overflow: auto;
    white-space: pre-wrap;
    word-break: break-word;
    border-top: 1px solid var(--border);
  }
</style>
