<script lang="ts">
  import { onDestroy } from 'svelte';
  import { goto } from '$app/navigation';
  import PasswordGenerator from '$lib/PasswordGenerator.svelte';
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
  import { generateCode, parseTotpInput, TotpError, type TotpConfig } from '$lib/totp';
  import { HibpError, pwnedCount } from '$lib/hibp';
  import { vault } from '$lib/vault.svelte';

  type SecretPayload = {
    username: string;
    password: string;
    url: string;
    totp_seed: string;
    notes: string;
  };

  function emptyPayload(): SecretPayload {
    return { username: '', password: '', url: '', totp_seed: '', notes: '' };
  }

  type Props = { item: Item };
  let { item: initial }: Props = $props();

  // svelte-ignore state_referenced_locally
  let item = $state<Item>(initial);
  // svelte-ignore state_referenced_locally
  let title = $state(initial.title);
  // svelte-ignore state_referenced_locally
  let payload = $state<SecretPayload>(decryptPayload(initial));
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

  let revealed = $state(false);
  let revealTimer: ReturnType<typeof setTimeout> | null = null;
  const REVEAL_MS = 10_000;
  const CLIPBOARD_CLEAR_MS = 30_000;

  let showGenerator = $state(false);

  let breachState = $state<
    | { kind: 'idle' }
    | { kind: 'checking' }
    | { kind: 'safe' }
    | { kind: 'pwned'; count: number }
    | { kind: 'error'; message: string }
  >({ kind: 'idle' });

  async function checkBreach() {
    if (!payload.password) {
      breachState = { kind: 'error', message: 'no password to check' };
      return;
    }
    breachState = { kind: 'checking' };
    try {
      const count = await pwnedCount(payload.password);
      breachState = count > 0 ? { kind: 'pwned', count } : { kind: 'safe' };
    } catch (err) {
      breachState = {
        kind: 'error',
        message: err instanceof HibpError ? err.message : 'check failed'
      };
    }
  }
  $effect(() => {
    // Reset breach state when the password changes — old result no longer
    // applies.
    void payload.password;
    breachState = { kind: 'idle' };
  });

  // Live TOTP code state.
  let totpConfig = $state<TotpConfig | null>(null);
  let totpError = $state('');
  let totpCode = $state('');
  let totpSeconds = $state(0);
  let totpTicker: ReturnType<typeof setInterval> | null = null;

  let saveTimer: ReturnType<typeof setTimeout> | null = null;
  const SAVE_DEBOUNCE_MS = 600;

  $effect(() => {
    if (initial.id !== item.id) {
      item = initial;
      title = initial.title;
      payload = decryptPayload(initial);
      tagsInput = formatTagInput(initial.tags);
      pathInput = initial.path;
      dirty = false;
      lastSavedAt = new Date(initial.updated_at);
      error = '';
      revealed = false;
    }
  });

  $effect(() => {
    const seed = payload.totp_seed.trim();
    if (!seed) {
      totpConfig = null;
      totpError = '';
      return;
    }
    try {
      totpConfig = parseTotpInput(seed);
      totpError = '';
    } catch (err) {
      totpConfig = null;
      totpError = err instanceof TotpError ? err.message : 'invalid totp';
    }
  });

  $effect(() => {
    if (totpTicker) {
      clearInterval(totpTicker);
      totpTicker = null;
    }
    if (!totpConfig) {
      totpCode = '';
      totpSeconds = 0;
      return;
    }
    const cfg = totpConfig;
    const tick = async () => {
      try {
        const r = await generateCode(cfg);
        totpCode = r.code;
        totpSeconds = r.secondsRemaining;
      } catch {
        totpCode = '';
      }
    };
    tick();
    totpTicker = setInterval(tick, 1000);
  });

  onDestroy(() => {
    if (totpTicker) clearInterval(totpTicker);
    if (revealTimer) clearTimeout(revealTimer);
    if (saveTimer) clearTimeout(saveTimer);
  });

  function decryptPayload(n: Item): SecretPayload {
    if (!n.encrypted_body || !n.wrapped_item_key) return emptyPayload();
    if (!vault.masterKey) return emptyPayload();
    try {
      const itemKey = openSeal(fromBase64(n.wrapped_item_key), vault.masterKey);
      const bytes = openSeal(fromBase64(n.encrypted_body), itemKey);
      const json = utf8Decode(bytes);
      const parsed = JSON.parse(json) as Partial<SecretPayload>;
      return { ...emptyPayload(), ...parsed };
    } catch (err) {
      console.error('secret decrypt failed', err);
      return emptyPayload();
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
    const payloadSnap = JSON.stringify(payload);
    const tagsSnap = parseTagInput(tagsInput);
    const pathSnap = normalizePath(pathInput);
    dirty = false;
    try {
      const itemKey = generateItemKey();
      const encryptedBody = seal(utf8Encode(payloadSnap), itemKey);
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
    if (!confirm('move this secret to trash?')) return;
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
    if (!confirm('permanently delete this secret? this cannot be undone.')) return;
    try {
      await api.deleteItem(item.id, { hard: true });
      items.remove(item.id);
      goto('/items', { replaceState: true });
    } catch (err) {
      error = err instanceof Error ? err.message : 'delete failed';
    }
  }

  async function copyAndClear(text: string, label: string) {
    if (!text) return;
    try {
      await navigator.clipboard.writeText(text);
      flashStatus(`${label} copied — clears in 30s`);
      setTimeout(async () => {
        try {
          const current = await navigator.clipboard.readText();
          if (current === text) await navigator.clipboard.writeText('');
        } catch {
          // No clipboard read permission — best-effort overwrite anyway.
          try {
            await navigator.clipboard.writeText('');
          } catch {
            /* ignore */
          }
        }
      }, CLIPBOARD_CLEAR_MS);
    } catch {
      flashStatus(`${label}: clipboard blocked`);
    }
  }

  let statusFlash = $state('');
  function flashStatus(msg: string) {
    statusFlash = msg;
    setTimeout(() => {
      if (statusFlash === msg) statusFlash = '';
    }, 3000);
  }

  function toggleReveal() {
    if (revealTimer) {
      clearTimeout(revealTimer);
      revealTimer = null;
    }
    revealed = !revealed;
    if (revealed) {
      revealTimer = setTimeout(() => {
        revealed = false;
      }, REVEAL_MS);
    }
  }

  function onPickGenerated(pw: string) {
    payload.password = pw;
    showGenerator = false;
    scheduleSave();
  }

  function onField(_: Event) {
    scheduleSave();
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

  let savedLabel = $derived.by(() => {
    if (saving) return 'saving…';
    if (dirty) return 'unsaved';
    if (!lastSavedAt) return '';
    const diff = (Date.now() - lastSavedAt.getTime()) / 1000;
    if (diff < 5) return 'saved';
    if (diff < 60) return `saved ${Math.floor(diff)}s ago`;
    return `saved ${lastSavedAt.toLocaleTimeString()}`;
  });
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
  {#if !trashed}
    <button class="danger" onclick={del}>delete</button>
  {/if}
</div>
<div class="meta-row row">
  <input
    class="meta-input"
    type="text"
    placeholder="tags (comma-separated, optional #)"
    value={tagsInput}
    oninput={onTagsInput}
    autocomplete="off"
    spellcheck="false"
  />
  <input
    class="meta-input path"
    type="text"
    placeholder="/path"
    value={pathInput}
    oninput={onPathInput}
    autocomplete="off"
    spellcheck="false"
  />
</div>

<div class="body">
  <div class="form">
    <label class="field">
      <span class="lbl">username</span>
      <div class="row">
        <input
          type="text"
          autocomplete="off"
          spellcheck="false"
          bind:value={payload.username}
          oninput={onField}
        />
        <button type="button" onclick={() => copyAndClear(payload.username, 'username')}>
          copy
        </button>
      </div>
    </label>

    <label class="field">
      <span class="lbl">password</span>
      <div class="row">
        {#if revealed}
          <input
            type="text"
            autocomplete="off"
            spellcheck="false"
            bind:value={payload.password}
            oninput={onField}
          />
        {:else}
          <input
            type="password"
            autocomplete="new-password"
            bind:value={payload.password}
            oninput={onField}
          />
        {/if}
        <button type="button" onclick={toggleReveal}>{revealed ? 'hide' : 'reveal'}</button>
        <button type="button" onclick={() => (showGenerator = true)}>generate</button>
        <button type="button" onclick={() => copyAndClear(payload.password, 'password')}>
          copy
        </button>
      </div>
      {#if showGenerator}
        <PasswordGenerator
          onPick={onPickGenerated}
          onClose={() => (showGenerator = false)}
        />
      {/if}
      <div class="row breach-row">
        <button
          type="button"
          class="link"
          onclick={checkBreach}
          disabled={breachState.kind === 'checking'}
        >
          {breachState.kind === 'checking' ? 'checking…' : 'check breach (HIBP)'}
        </button>
        {#if breachState.kind === 'safe'}
          <span class="muted small">not found in any known breach</span>
        {:else if breachState.kind === 'pwned'}
          <span class="danger small">
            seen {breachState.count.toLocaleString()} times in known breaches —
            don't use
          </span>
        {:else if breachState.kind === 'error'}
          <span class="muted small">{breachState.message}</span>
        {/if}
      </div>
    </label>

    <label class="field">
      <span class="lbl">url</span>
      <div class="row">
        <input
          type="url"
          autocomplete="off"
          spellcheck="false"
          bind:value={payload.url}
          oninput={onField}
        />
        {#if payload.url}
          <a class="open-link" href={payload.url} target="_blank" rel="noopener noreferrer">
            open ↗
          </a>
        {/if}
      </div>
    </label>

    <label class="field">
      <span class="lbl">totp seed (base32 or otpauth://)</span>
      <input
        type="text"
        autocomplete="off"
        spellcheck="false"
        bind:value={payload.totp_seed}
        oninput={onField}
      />
      {#if totpError}
        <span class="danger small">{totpError}</span>
      {:else if totpCode}
        <div class="row totp-line">
          <code class="totp-code">{totpCode.slice(0, 3)} {totpCode.slice(3)}</code>
          <span class="muted small">{totpSeconds}s</span>
          <button type="button" onclick={() => copyAndClear(totpCode, 'code')}>
            copy
          </button>
        </div>
      {/if}
    </label>

    <label class="field">
      <span class="lbl">notes</span>
      <textarea
        rows="6"
        spellcheck="false"
        bind:value={payload.notes}
        oninput={onField}
      ></textarea>
    </label>
  </div>
</div>

<div class="meta row">
  {#if error}
    <span class="danger">{error}</span>
  {:else if statusFlash}
    <span class="muted">{statusFlash}</span>
  {:else}
    <span class="muted">{savedLabel}</span>
  {/if}
  <span class="grow"></span>
  <span class="muted" title="encrypted on this device">e2e · secret · {item.id.slice(0, 8)}</span>
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
  }
  .form {
    flex: 1;
    padding: 12px 16px;
    display: flex;
    flex-direction: column;
    gap: 12px;
    max-width: 640px;
  }
  .field {
    display: flex;
    flex-direction: column;
    gap: 4px;
  }
  .field input,
  .field textarea {
    width: 100%;
  }
  .field .row input {
    flex: 1;
  }
  .lbl {
    font-size: 11px;
    color: var(--muted);
  }
  .open-link {
    align-self: center;
    font-size: 11px;
  }
  .small {
    font-size: 11px;
  }
  .totp-line {
    margin-top: 4px;
    align-items: center;
  }
  .totp-code {
    font-family: var(--font-mono);
    font-size: 18px;
    letter-spacing: 0.08em;
    border: 1px solid var(--border);
    padding: 2px 6px;
    user-select: all;
  }
  .breach-row {
    margin-top: 4px;
    gap: 8px;
  }
  .link {
    border: none;
    background: none;
    padding: 0;
    color: var(--muted);
    cursor: pointer;
    font-size: 11px;
    text-decoration: underline;
  }
  .link:hover {
    color: var(--fg);
  }
  textarea {
    font: inherit;
    color: inherit;
    background: var(--bg);
    border: 1px solid var(--border);
    padding: 4px 8px;
    border-radius: 0;
    resize: vertical;
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
</style>
