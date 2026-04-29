<script lang="ts">
  import { onDestroy, onMount } from 'svelte';
  import { page } from '$app/stores';
  import { marked } from 'marked';
  import { api, ApiError, type PublicShareView } from '$lib/api';
  import {
    ensureReady,
    fromBase64,
    open as openSeal,
    utf8Decode
  } from '$lib/crypto';

  type FilePayload = {
    filename: string;
    mime: string;
    size: number;
    item_key_b64: string;
  };

  let token = $derived($page.params.token);
  let view = $state<PublicShareView | null>(null);
  let plaintext = $state('');
  let filePayload = $state<FilePayload | null>(null);
  let fileBlobUrl = $state<string | null>(null);
  let filePreviewKind = $state<'image' | 'pdf' | 'text' | 'none'>('none');
  let filePreviewText = $state('');
  let fileLoading = $state(false);
  let error = $state('');
  let loading = $state(true);

  /// Cached share key + sealed payload bytes — needed to decrypt blob on demand.
  let shareKey: Uint8Array | null = null;

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
      shareKey = fromBase64(keyB64);
      const fetched = await api.publicShare(token!);
      view = fetched;
      const cipher = fromBase64(fetched.encrypted_payload);
      const opened = openSeal(cipher, shareKey);

      if (fetched.item_type === 'note') {
        plaintext = utf8Decode(opened);
      } else if (fetched.item_type === 'file') {
        filePayload = JSON.parse(utf8Decode(opened)) as FilePayload;
        filePreviewKind = pickPreviewKind(filePayload.mime);
      } else {
        error = `unsupported share type: ${fetched.item_type}`;
      }
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

  function pickPreviewKind(mime: string): typeof filePreviewKind {
    if (mime.startsWith('image/')) return 'image';
    if (mime === 'application/pdf') return 'pdf';
    if (mime.startsWith('text/') || mime === 'application/json') return 'text';
    return 'none';
  }

  /// Pull the encrypted blob from the public route, decrypt with item_key,
  /// and either preview inline or trigger a download.
  async function fetchAndDecrypt(): Promise<Uint8Array | null> {
    if (!filePayload) return null;
    const res = await fetch(`/api/v1/share/${token}/blob`);
    if (!res.ok) {
      throw new Error(`server returned ${res.status}`);
    }
    const cipherBytes = new Uint8Array(await res.arrayBuffer());
    const itemKey = fromBase64(filePayload.item_key_b64);
    return openSeal(cipherBytes, itemKey);
  }

  async function loadPreview() {
    if (!filePayload) return;
    fileLoading = true;
    try {
      const bytes = await fetchAndDecrypt();
      if (!bytes) return;
      if (filePreviewKind === 'text') {
        filePreviewText = new TextDecoder().decode(bytes);
      } else {
        const blob = new Blob([bytes as BlobPart], { type: filePayload.mime });
        fileBlobUrl = URL.createObjectURL(blob);
      }
    } catch (err) {
      error = err instanceof Error ? err.message : 'preview failed';
    } finally {
      fileLoading = false;
    }
  }

  async function downloadFile() {
    if (!filePayload) return;
    fileLoading = true;
    try {
      const bytes = await fetchAndDecrypt();
      if (!bytes) return;
      const blob = new Blob([bytes as BlobPart], { type: filePayload.mime });
      const url = URL.createObjectURL(blob);
      const a = document.createElement('a');
      a.href = url;
      a.download = filePayload.filename;
      a.click();
      setTimeout(() => URL.revokeObjectURL(url), 5000);
    } catch (err) {
      error = err instanceof Error ? err.message : 'download failed';
    } finally {
      fileLoading = false;
    }
  }

  onDestroy(() => {
    if (fileBlobUrl) URL.revokeObjectURL(fileBlobUrl);
  });

  let rendered = $derived.by(() => {
    if (!plaintext) return '';
    return marked.parse(plaintext, { async: false }) as string;
  });

  function humanSize(bytes: number): string {
    if (!Number.isFinite(bytes) || bytes < 0) return '?';
    if (bytes < 1024) return `${bytes} B`;
    if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`;
    if (bytes < 1024 * 1024 * 1024) return `${(bytes / 1024 / 1024).toFixed(1)} MB`;
    return `${(bytes / 1024 / 1024 / 1024).toFixed(2)} GB`;
  }

  let kindLabel = $derived(view?.item_type === 'file' ? 'shared file' : 'shared note');
</script>

<div class="share-page">
  <header class="share-head">
    <span class="brand">rongnote · {kindLabel}</span>
    {#if view?.expires_at}
      <span class="muted">expires {new Date(view.expires_at).toLocaleString()}</span>
    {/if}
  </header>

  {#if loading}
    <div class="muted center">…</div>
  {:else if error}
    <div class="danger center">{error}</div>
  {:else if view?.item_type === 'note'}
    <h1 class="title">{view.item_title || '(untitled)'}</h1>
    <article class="md">{@html rendered}</article>
  {:else if view?.item_type === 'file' && filePayload}
    <h1 class="title">{filePayload.filename || view.item_title}</h1>
    <dl class="file-meta">
      <dt>type</dt>
      <dd class="muted">{filePayload.mime}</dd>
      <dt>size</dt>
      <dd class="muted">{humanSize(filePayload.size)}</dd>
    </dl>

    <div class="file-actions">
      <button type="button" disabled={fileLoading} onclick={downloadFile}>
        {fileLoading ? 'decrypting…' : 'download'}
      </button>
      {#if filePreviewKind !== 'none' && !fileBlobUrl && !filePreviewText}
        <button type="button" disabled={fileLoading} onclick={loadPreview}>
          load preview
        </button>
      {/if}
    </div>

    {#if filePreviewKind === 'image' && fileBlobUrl}
      <img src={fileBlobUrl} alt={filePayload.filename} />
    {:else if filePreviewKind === 'pdf' && fileBlobUrl}
      <iframe src={fileBlobUrl} title={filePayload.filename}></iframe>
    {:else if filePreviewKind === 'text' && filePreviewText}
      <pre>{filePreviewText}</pre>
    {/if}
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
    word-break: break-all;
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
  .file-meta {
    display: grid;
    grid-template-columns: 80px 1fr;
    gap: 4px 12px;
    margin: 0 0 16px;
  }
  .file-meta dt {
    color: var(--muted);
    font-size: 12px;
  }
  .file-meta dd {
    margin: 0;
    word-break: break-all;
  }
  .file-actions {
    display: flex;
    gap: 8px;
    margin-bottom: 16px;
  }
  img {
    max-width: 100%;
    max-height: 600px;
    object-fit: contain;
  }
  iframe {
    width: 100%;
    height: 600px;
    border: 1px solid var(--border);
  }
  pre {
    background: rgba(127, 127, 127, 0.08);
    padding: 8px;
    overflow: auto;
    max-height: 500px;
    margin: 0;
    white-space: pre-wrap;
    word-break: break-word;
  }
</style>
