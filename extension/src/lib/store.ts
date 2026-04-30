// Wrappers around browser.storage. session = ephemeral (cleared when the
// browser closes), local = persisted (survives restart).
//
// Settings live in `local`: server URL, last email used.
// Vault keys live in `session`: master_key, public_key, private_key, base64.
// → A browser restart re-locks the extension. Same security posture as the
// SPA's sessionStorage.

const KEY_SETTINGS = 'rn.settings';
const KEY_VAULT = 'rn.vault';
const KEY_CACHE = 'rn.cache';

export interface Settings {
  server: string;
  email: string;
}

export interface Vault {
  master_key_b64: string;
  public_key_b64: string;
  private_key_b64: string;
  /// epoch ms — used to drive the auto-lock check
  unlocked_at: number;
}

export async function loadSettings(): Promise<Settings> {
  const out = await browser.storage.local.get(KEY_SETTINGS);
  const s = out[KEY_SETTINGS] as Partial<Settings> | undefined;
  return {
    server: s?.server ?? '',
    email: s?.email ?? ''
  };
}

export async function saveSettings(s: Settings): Promise<void> {
  await browser.storage.local.set({ [KEY_SETTINGS]: s });
}

export async function loadVault(): Promise<Vault | null> {
  const out = await browser.storage.session.get(KEY_VAULT);
  return (out[KEY_VAULT] as Vault | undefined) ?? null;
}

export async function saveVault(v: Vault): Promise<void> {
  await browser.storage.session.set({ [KEY_VAULT]: v });
}

export async function clearVault(): Promise<void> {
  await browser.storage.session.remove([KEY_VAULT, KEY_CACHE]);
}

/// Decrypted secret payloads keyed by item id. Lives in session storage so
/// it dies with the browser. Lets the popup show matches in <100ms once
/// the first decrypt-everything pass has run.
export interface CachedSecret {
  id: string;
  updated_at: string;
  title: string;
  username: string;
  password: string;
  url: string;
  totp_seed: string;
  notes: string;
}

export async function loadSecretCache(): Promise<Record<string, CachedSecret>> {
  const out = await browser.storage.session.get(KEY_CACHE);
  return (out[KEY_CACHE] as Record<string, CachedSecret> | undefined) ?? {};
}

export async function saveSecretCache(
  cache: Record<string, CachedSecret>
): Promise<void> {
  await browser.storage.session.set({ [KEY_CACHE]: cache });
}
