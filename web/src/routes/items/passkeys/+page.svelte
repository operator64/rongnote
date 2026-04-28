<script lang="ts">
  import { onMount } from 'svelte';
  import { goto } from '$app/navigation';
  import { api, ApiError, type PasskeyListItem } from '$lib/api';
  import { vault } from '$lib/vault.svelte';
  import { isPasskeySupported, registerPasskey, PasskeyError } from '$lib/webauthn';

  let passkeys = $state<PasskeyListItem[]>([]);
  let loading = $state(true);
  let error = $state('');
  let busy = $state(false);
  const supported = isPasskeySupported();

  onMount(load);

  async function load() {
    loading = true;
    error = '';
    try {
      passkeys = await api.listPasskeys();
    } catch (err) {
      error = err instanceof ApiError ? err.message : 'load failed';
    } finally {
      loading = false;
    }
  }

  async function add() {
    if (!vault.masterKey) {
      error = 'vault locked — unlock first';
      return;
    }
    busy = true;
    error = '';
    try {
      const name =
        prompt('passkey name (e.g. "iPhone", "Yubikey 5C")', '') ?? undefined;
      await registerPasskey({
        masterKey: vault.masterKey,
        name: name?.trim() || undefined
      });
      await load();
    } catch (err) {
      error = err instanceof PasskeyError ? err.message : err instanceof Error ? err.message : 'register failed';
    } finally {
      busy = false;
    }
  }

  async function remove(p: PasskeyListItem) {
    if (passkeys.length === 1) {
      if (
        !confirm(
          `delete your only passkey "${p.name}"? you'll have to fall back to passphrase login.`
        )
      )
        return;
    } else {
      if (!confirm(`delete passkey "${p.name}"?`)) return;
    }
    busy = true;
    error = '';
    try {
      await api.deletePasskey(p.id);
      await load();
    } catch (err) {
      error = err instanceof ApiError ? err.message : 'delete failed';
    } finally {
      busy = false;
    }
  }

  function fmtTime(s: string): string {
    return new Date(s).toLocaleString();
  }
</script>

<div class="page">
  <div class="head row">
    <button type="button" onclick={() => goto('/items')}>← back</button>
    <span class="grow"></span>
    {#if supported}
      <button type="button" disabled={busy} onclick={add}>+ add passkey</button>
    {/if}
  </div>

  {#if !supported}
    <div class="muted" style="padding: 16px;">
      this browser does not expose the WebAuthn API.
    </div>
  {:else if loading}
    <div class="muted" style="padding: 16px;">…</div>
  {:else if error}
    <div class="danger" style="padding: 16px;">{error}</div>
  {:else if passkeys.length === 0}
    <div class="empty">
      <p class="muted">no passkeys registered yet.</p>
      <p class="muted">
        click <strong>+ add passkey</strong> to bind your authenticator.
        the first one needs PRF support — the only authenticators known to
        ship it: Yubikey 5+, iOS 17+, Android Chrome ≥ 115, 1Password and
        Bitwarden extensions.
      </p>
    </div>
  {:else}
    <table class="passkeys">
      <thead>
        <tr>
          <th>name</th>
          <th class="when">added</th>
          <th class="when">last used</th>
          <th></th>
        </tr>
      </thead>
      <tbody>
        {#each passkeys as p (p.id)}
          <tr>
            <td>{p.name || '(unnamed)'}</td>
            <td class="when muted">{fmtTime(p.created_at)}</td>
            <td class="when muted">{p.last_used_at ? fmtTime(p.last_used_at) : 'never'}</td>
            <td class="action">
              <button type="button" class="danger" disabled={busy} onclick={() => remove(p)}>
                delete
              </button>
            </td>
          </tr>
        {/each}
      </tbody>
    </table>
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
  .empty {
    padding: 16px;
    max-width: 560px;
  }
  .empty p {
    margin: 0 0 12px;
  }
  .passkeys {
    border-collapse: collapse;
    width: 100%;
    overflow: auto;
    flex: 1;
    display: block;
  }
  .passkeys thead {
    position: sticky;
    top: 0;
    background: var(--bg);
    border-bottom: 1px solid var(--border);
  }
  .passkeys th {
    text-align: left;
    padding: 6px 12px;
    font-weight: normal;
    color: var(--muted);
    font-size: 11px;
    text-transform: uppercase;
    letter-spacing: 0.05em;
  }
  .passkeys td {
    padding: 6px 12px;
    border-bottom: 1px solid var(--border);
  }
  .when {
    width: 200px;
    white-space: nowrap;
    font-size: 12px;
  }
  .action {
    width: 100px;
    text-align: right;
  }
</style>
