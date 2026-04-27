<script lang="ts">
  import { goto } from '$app/navigation';
  import { api, ApiError } from '$lib/api';
  import {
    canonicalizeRecoveryCode,
    deriveAuthHash,
    deriveKekFromPassphrase,
    deriveKekFromRecoveryCode,
    ensureReady,
    fromBase64,
    generateSalt,
    isValidRecoveryCode,
    open,
    seal,
    toBase64
  } from '$lib/crypto';

  let email = $state('');
  let code = $state('');
  let password = $state('');
  let confirm = $state('');
  let error = $state('');
  let success = $state(false);
  let busy = $state(false);
  let stage = $state('');

  async function submit(e: Event) {
    e.preventDefault();
    error = '';
    if (!isValidRecoveryCode(code)) {
      error = 'recovery code looks malformed (24 base32 characters)';
      return;
    }
    if (password !== confirm) {
      error = 'new passwords do not match';
      return;
    }
    if (password.length < 12) {
      error = 'new password must be at least 12 characters';
      return;
    }
    busy = true;
    try {
      await ensureReady();

      stage = 'fetching recovery wrap…';
      const init = await api.recoveryInit(email);

      stage = 'deriving recovery key (Argon2id)…';
      const recoverySalt = fromBase64(init.recovery_salt);
      const recoveryKek = await deriveKekFromRecoveryCode(code, recoverySalt);

      stage = 'unwrapping master key…';
      let masterKey: Uint8Array;
      try {
        masterKey = open(fromBase64(init.master_wrap_recovery), recoveryKek);
      } catch {
        throw new Error('recovery code does not match');
      }

      stage = 'deriving new passphrase key (Argon2id)…';
      const newSalt = generateSalt();
      const newKek = await deriveKekFromPassphrase(password, newSalt);

      stage = 'rewrapping…';
      const newWrap = seal(masterKey, newKek);
      const authHash = deriveAuthHash(masterKey);

      stage = 'updating server…';
      await api.resetPassphrase({
        email,
        auth_hash: toBase64(authHash),
        new_passphrase_salt: toBase64(newSalt),
        new_master_wrap_passphrase: toBase64(newWrap)
      });

      success = true;
    } catch (err) {
      if (err instanceof ApiError) {
        error =
          err.status === 401 || err.status === 404
            ? 'email or recovery code does not match'
            : err.message;
      } else {
        error = err instanceof Error ? err.message : 'recovery failed';
      }
    } finally {
      busy = false;
      stage = '';
    }
  }

  let canonical = $derived(canonicalizeRecoveryCode(code));
</script>

{#if success}
  <div class="center-form">
    <h1>passphrase reset</h1>
    <p class="muted">your new passphrase is active. existing sessions on other tabs were signed out.</p>
    <div class="actions">
      <span class="grow"></span>
      <button type="button" onclick={() => goto('/login', { replaceState: true })}>
        sign in
      </button>
    </div>
  </div>
{:else}
  <form class="center-form" onsubmit={submit}>
    <h1>reset passphrase via recovery code</h1>
    <p class="muted">
      enter the 24-character recovery code you saved when you registered, plus
      the new passphrase you want to use. notes are not lost.
    </p>
    <label>
      <span>email</span>
      <input type="email" autocomplete="email" bind:value={email} required />
    </label>
    <label>
      <span>recovery code (dashes optional)</span>
      <input
        type="text"
        autocomplete="off"
        spellcheck="false"
        autocapitalize="characters"
        bind:value={code}
        required
      />
      {#if code && canonical.length > 0}
        <span class="muted hint">{canonical.length}/24 chars</span>
      {/if}
    </label>
    <label>
      <span>new password (min 12 chars)</span>
      <input
        type="password"
        autocomplete="new-password"
        bind:value={password}
        required
      />
    </label>
    <label>
      <span>confirm</span>
      <input
        type="password"
        autocomplete="new-password"
        bind:value={confirm}
        required
      />
    </label>
    <div class="err">{error || (busy ? stage : '')}</div>
    <div class="actions">
      <a href="/login">back to sign in</a>
      <button type="submit" disabled={busy}>{busy ? '…' : 'reset'}</button>
    </div>
  </form>
{/if}

<style>
  .center-form p {
    color: var(--muted);
    margin: 0 0 12px;
  }
  .hint {
    display: block;
    font-size: 12px;
    margin-top: 2px;
  }
</style>
