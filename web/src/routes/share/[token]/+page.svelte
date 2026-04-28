<script lang="ts">
  import { onMount } from 'svelte';
  import { page } from '$app/stores';
  import { marked } from 'marked';
  import { api, ApiError, type PublicShareView } from '$lib/api';
  import {
    ensureReady,
    fromBase64,
    open as openSeal,
    utf8Decode
  } from '$lib/crypto';

  let token = $derived($page.params.token);
  let view = $state<PublicShareView | null>(null);
  let plaintext = $state('');
  let error = $state('');
  let loading = $state(true);

  onMount(async () => {
    try {
      await ensureReady();
      // Read the share key from the URL fragment. Fragments are NOT sent in
      // HTTP requests, so the server never sees the key.
      const fragment = location.hash.startsWith('#') ? location.hash.slice(1) : '';
      const keyB64 = fragment.replace(/-/g, '+').replace(/_/g, '/');
      if (!keyB64) {
        error = 'missing decryption key — the link must include a #key fragment';
        return;
      }
      const key = fromBase64(keyB64);
      const fetched = await api.publicShare(token!);
      view = fetched;
      const cipher = fromBase64(fetched.encrypted_payload);
      plaintext = utf8Decode(openSeal(cipher, key));
    } catch (err) {
      if (err instanceof ApiError && err.status === 404) {
        error = 'link expired or revoked';
      } else {
        error = err instanceof Error ? err.message : 'failed to open';
      }
    } finally {
      loading = false;
    }
  });

  let rendered = $derived.by(() => {
    if (!plaintext) return '';
    return marked.parse(plaintext, { async: false }) as string;
  });
</script>

<div class="share-page">
  <header class="share-head">
    <span class="brand">rongnote · shared note</span>
    {#if view?.expires_at}
      <span class="muted">expires {new Date(view.expires_at).toLocaleString()}</span>
    {/if}
  </header>

  {#if loading}
    <div class="muted center">…</div>
  {:else if error}
    <div class="danger center">{error}</div>
  {:else if view}
    <h1 class="title">{view.item_title || '(untitled)'}</h1>
    <article class="md">{@html rendered}</article>
  {/if}
</div>

<style>
  .share-page {
    max-width: 760px;
    margin: 0 auto;
    padding: 16px 24px 64px;
  }
  .share-head {
    display: flex;
    justify-content: space-between;
    align-items: baseline;
    padding-bottom: 8px;
    border-bottom: 1px solid var(--border);
    margin-bottom: 16px;
    font-size: 12px;
  }
  .brand {
    color: var(--muted);
    font-family: var(--font-mono);
  }
  .title {
    font-size: 22px;
    margin: 8px 0 16px;
  }
  .md {
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
  .center {
    text-align: center;
    padding: 64px 0;
  }
</style>
