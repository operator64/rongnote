<script lang="ts">
  import { onMount } from 'svelte';
  import { fromBase64 } from '$lib/crypto';
  import { session } from '$lib/session.svelte';
  import { vault } from '$lib/vault.svelte';

  let password = $state('');
  let error = $state('');
  let busy = $state(false);
  let stage = $state('');
  let pwInput: HTMLInputElement | undefined = $state();

  onMount(() => pwInput?.focus());

  async function submit(e: Event) {
    e.preventDefault();
    if (!session.user) return;
    busy = true;
    error = '';
    stage = 'deriving passphrase key…';
    try {
      await vault.unlockFromPassphrase({
        passphrase: password,
        passphraseSalt: fromBase64(session.user.passphrase_salt),
        masterWrapPassphrase: fromBase64(session.user.master_wrap_passphrase),
        publicKey: fromBase64(session.user.public_key),
        encryptedPrivateKey: fromBase64(session.user.encrypted_private_key)
      });
    } catch (err) {
      error = err instanceof Error ? err.message : 'unlock failed';
    } finally {
      busy = false;
      stage = '';
    }
  }

  async function signOut() {
    vault.lock();
    await session.logout();
    location.assign('/login');
  }
</script>

<form class="center-form" onsubmit={submit}>
  <h1>vault locked</h1>
  <p class="muted">
    your session is still valid, but the master key isn't loaded in this tab.
    enter your passphrase to unlock — server can't reset it.
  </p>
  <label>
    <span>passphrase for {session.user?.email ?? ''}</span>
    <input
      type="password"
      autocomplete="current-password"
      bind:value={password}
      bind:this={pwInput}
      required
    />
  </label>
  <div class="err">{error || (busy ? stage : '')}</div>
  <div class="actions">
    <button type="button" class="link" onclick={signOut}>sign out</button>
    <button type="submit" disabled={busy}>{busy ? '…' : 'unlock'}</button>
  </div>
</form>

<style>
  .center-form p {
    color: var(--muted);
    margin: 0 0 12px;
  }
  .link {
    border: none;
    background: none;
    padding: 0;
    color: var(--muted);
  }
  .link:hover {
    color: var(--fg);
  }
</style>
