<script lang="ts">
  import { onMount } from 'svelte';
  import { fromBase64 } from '$lib/crypto';
  import { session } from '$lib/session.svelte';
  import { vault } from '$lib/vault.svelte';
  import { isPasskeySupported, loginWithPasskey, PasskeyError } from '$lib/webauthn';

  let password = $state('');
  let error = $state('');
  let busy = $state(false);
  let stage = $state('');
  let pwInput: HTMLInputElement | undefined = $state();
  const passkeysSupported = isPasskeySupported();

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

  async function passkeyUnlock() {
    busy = true;
    error = '';
    stage = 'waiting for passkey…';
    try {
      const { user, masterKey } = await loginWithPasskey();
      await vault.installAndUnwrap({
        masterKey,
        publicKey: fromBase64(user.public_key),
        encryptedPrivateKey: fromBase64(user.encrypted_private_key)
      });
      // The discoverable-auth flow may have selected a different account's
      // passkey. Sync the session view to whoever was actually authenticated.
      session.setUser(user);
    } catch (err) {
      if (err instanceof PasskeyError) {
        error = err.message;
      } else {
        error = err instanceof Error ? err.message : 'passkey unlock failed';
      }
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
  {#if passkeysSupported}
    <hr class="separator" />
    <button
      type="button"
      class="passkey-btn"
      disabled={busy}
      onclick={passkeyUnlock}
    >
      unlock with passkey
    </button>
  {/if}
</form>

<style>
  .separator {
    border: none;
    border-top: 1px solid var(--border);
    margin: 16px 0 12px;
  }
  .passkey-btn {
    width: 100%;
  }
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
