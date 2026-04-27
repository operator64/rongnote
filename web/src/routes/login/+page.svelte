<script lang="ts">
  import { goto } from '$app/navigation';
  import { api, ApiError } from '$lib/api';
  import {
    deriveAuthHash,
    deriveKekFromPassphrase,
    ensureReady,
    fromBase64,
    open,
    toBase64
  } from '$lib/crypto';
  import { session } from '$lib/session.svelte';
  import { vault } from '$lib/vault.svelte';
  import { isPasskeySupported, loginWithPasskey, PasskeyError } from '$lib/webauthn';

  let email = $state('');
  let password = $state('');
  let error = $state('');
  let busy = $state(false);
  let stage = $state('');
  const passkeysSupported = isPasskeySupported();

  async function submit(e: Event) {
    e.preventDefault();
    busy = true;
    error = '';
    try {
      await ensureReady();

      stage = 'fetching salt…';
      const pre = await api.precheck(email);

      stage = 'deriving passphrase key (Argon2id)…';
      const passphraseSalt = fromBase64(pre.passphrase_salt);
      const passphraseKek = await deriveKekFromPassphrase(password, passphraseSalt);

      stage = 'unwrapping master key…';
      let masterKey: Uint8Array;
      try {
        masterKey = open(fromBase64(pre.master_wrap_passphrase), passphraseKek);
      } catch {
        // Wrong passphrase — drop the same hint as the server's 401 path.
        throw new ApiError(401, 'unauthorized', 'invalid credentials');
      }

      stage = 'authenticating…';
      const authHash = deriveAuthHash(masterKey);
      const user = await api.login(email, toBase64(authHash));

      stage = 'opening vault…';
      await vault.installAndUnwrap({
        masterKey,
        publicKey: fromBase64(user.public_key),
        encryptedPrivateKey: fromBase64(user.encrypted_private_key)
      });
      session.setUser(user);
      await goto('/items', { replaceState: true });
    } catch (err) {
      if (err instanceof ApiError) {
        error =
          err.status === 401 || err.status === 404 ? 'invalid credentials' : err.message;
      } else {
        error = err instanceof Error ? err.message : 'login failed';
      }
    } finally {
      busy = false;
      stage = '';
    }
  }

  async function passkeyLogin() {
    busy = true;
    error = '';
    stage = 'waiting for passkey…';
    try {
      const { user, masterKey } = await loginWithPasskey();
      stage = 'opening vault…';
      await vault.installAndUnwrap({
        masterKey,
        publicKey: fromBase64(user.public_key),
        encryptedPrivateKey: fromBase64(user.encrypted_private_key)
      });
      session.setUser(user);
      await goto('/items', { replaceState: true });
    } catch (err) {
      if (err instanceof PasskeyError) {
        error = err.message;
      } else if (err instanceof ApiError) {
        error = err.status === 401 ? 'passkey not recognised' : err.message;
      } else {
        error = err instanceof Error ? err.message : 'passkey login failed';
      }
    } finally {
      busy = false;
      stage = '';
    }
  }
</script>

<form class="center-form" onsubmit={submit}>
  <h1>rongnote — sign in</h1>
  <label>
    <span>email</span>
    <input type="email" autocomplete="email" bind:value={email} required />
  </label>
  <label>
    <span>password</span>
    <input type="password" autocomplete="current-password" bind:value={password} required />
  </label>
  <div class="err">{error || (busy ? stage : '')}</div>
  <div class="actions">
    <a href="/register">register</a>
    <span class="grow"></span>
    <a href="/recovery">forgot?</a>
    <button type="submit" disabled={busy} style="margin-left: 12px;">
      {busy ? '…' : 'sign in'}
    </button>
  </div>
  {#if passkeysSupported}
    <div class="passkey-row">
      <hr style="border: none; border-top: 1px solid var(--border); margin: 16px 0 12px;" />
      <button type="button" class="passkey-btn" disabled={busy} onclick={passkeyLogin}>
        sign in with passkey
      </button>
    </div>
  {/if}
</form>

<style>
  .passkey-btn {
    width: 100%;
  }
</style>
