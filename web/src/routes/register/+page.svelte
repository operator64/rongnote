<script lang="ts">
  import { goto } from '$app/navigation';
  import { api, ApiError } from '$lib/api';
  import {
    deriveAuthHash,
    deriveKekFromPassphrase,
    deriveKekFromRecoveryCode,
    ensureReady,
    generateKeyPair,
    generateMasterKey,
    generateRecoveryCode,
    generateSalt,
    seal,
    toBase64
  } from '$lib/crypto';
  import { session } from '$lib/session.svelte';
  import { vault } from '$lib/vault.svelte';

  let email = $state('');
  let password = $state('');
  let confirm = $state('');
  let error = $state('');
  let busy = $state(false);
  let stage = $state('');
  let recoveryCode = $state<string | null>(null);
  let pendingUser = $state<typeof session.user>(null);
  let saved = $state(false);
  let copyLabel = $state('copy');

  async function submit(e: Event) {
    e.preventDefault();
    error = '';
    if (password !== confirm) {
      error = 'passwords do not match';
      return;
    }
    if (password.length < 12) {
      error = 'password must be at least 12 characters';
      return;
    }
    busy = true;
    try {
      await ensureReady();

      stage = 'preparing keys…';
      const masterKey = generateMasterKey();
      const kp = generateKeyPair();
      const passphraseSalt = generateSalt();
      const recoverySalt = generateSalt();
      const code = generateRecoveryCode();

      stage = 'deriving passphrase key (Argon2id)…';
      const passphraseKek = await deriveKekFromPassphrase(password, passphraseSalt);

      stage = 'deriving recovery key (Argon2id)…';
      const recoveryKek = await deriveKekFromRecoveryCode(code, recoverySalt);

      stage = 'wrapping…';
      const masterWrapPassphrase = seal(masterKey, passphraseKek);
      const masterWrapRecovery = seal(masterKey, recoveryKek);
      const wrappedPriv = seal(kp.privateKey, masterKey);
      const authHash = deriveAuthHash(masterKey);

      stage = 'creating account…';
      const user = await api.register({
        email,
        passphrase_salt: toBase64(passphraseSalt),
        recovery_salt: toBase64(recoverySalt),
        master_wrap_passphrase: toBase64(masterWrapPassphrase),
        master_wrap_recovery: toBase64(masterWrapRecovery),
        auth_hash: toBase64(authHash),
        public_key: toBase64(kp.publicKey),
        encrypted_private_key: toBase64(wrappedPriv)
      });

      vault.installFresh({
        masterKey,
        publicKey: kp.publicKey,
        privateKey: kp.privateKey
      });

      // Hold session.setUser() until the user confirms they saved the code —
      // otherwise the layout's auth-redirect would whisk them off to /items
      // before the recovery screen renders.
      pendingUser = user;
      recoveryCode = code;
    } catch (err) {
      error =
        err instanceof ApiError
          ? err.message
          : err instanceof Error
            ? err.message
            : 'register failed';
    } finally {
      busy = false;
      stage = '';
    }
  }

  async function copyCode() {
    if (!recoveryCode) return;
    try {
      await navigator.clipboard.writeText(recoveryCode);
      copyLabel = 'copied';
      setTimeout(() => (copyLabel = 'copy'), 1500);
    } catch {
      copyLabel = 'select+copy manually';
    }
  }

  async function continueToApp() {
    if (pendingUser) session.setUser(pendingUser);
    await goto('/items', { replaceState: true });
  }
</script>

{#if !recoveryCode}
  <form class="center-form" onsubmit={submit}>
    <h1>rongnote — register</h1>
    <label>
      <span>email</span>
      <input type="email" autocomplete="email" bind:value={email} required />
    </label>
    <label>
      <span>password (min 12 chars)</span>
      <input type="password" autocomplete="new-password" bind:value={password} required />
    </label>
    <label>
      <span>confirm</span>
      <input type="password" autocomplete="new-password" bind:value={confirm} required />
    </label>
    <div class="err">{error || (busy ? stage : '')}</div>
    <div class="actions">
      <a href="/login">sign in</a>
      <button type="submit" disabled={busy}>{busy ? '…' : 'create account'}</button>
    </div>
  </form>
{:else}
  <div class="center-form recovery">
    <h1>save your recovery code</h1>
    <p class="muted">
      write this down. it's the only way to recover your notes if you forget your
      passphrase. it's shown <strong>once</strong> — server can't reset it.
    </p>
    <div class="code">{recoveryCode}</div>
    <div class="row" style="justify-content: flex-end; margin: 8px 0 16px;">
      <button type="button" onclick={copyCode}>{copyLabel}</button>
    </div>
    <label class="checkbox">
      <input type="checkbox" bind:checked={saved} />
      <span>i've stored this somewhere safe</span>
    </label>
    <div class="actions">
      <span class="muted">{pendingUser?.email}</span>
      <button type="button" disabled={!saved} onclick={continueToApp}>continue</button>
    </div>
  </div>
{/if}

<style>
  .recovery .code {
    font-family: var(--font-mono);
    font-size: 18px;
    letter-spacing: 0.04em;
    text-align: center;
    padding: 16px 8px;
    border: 1px solid var(--border);
    user-select: all;
    word-spacing: 0.2em;
  }
  .recovery .checkbox {
    display: flex;
    align-items: center;
    gap: 8px;
    margin-bottom: 16px;
  }
  .recovery .checkbox input {
    width: auto;
  }
  .recovery p {
    color: var(--muted);
    margin: 0 0 16px;
  }
</style>
