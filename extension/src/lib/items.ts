// Decrypt secrets the same way the SPA does. Personal-space items use
// secretbox with master_key; team-space items use crypto_box_seal_open
// with the user's keypair.

import { boxOpen, fromBase64, open as openSecretbox, utf8Decode } from './crypto';
import type { Item } from './api';
import type { Vault } from './store';

export interface SecretPayload {
  username: string;
  password: string;
  url: string;
  totp_seed: string;
  notes: string;
}

function unwrapItemKey(item: Item, vault: Vault): Uint8Array {
  if (!item.wrapped_item_key) throw new Error('item has no key wrap');
  const blob = fromBase64(item.wrapped_item_key);
  const kind = item.key_wrap ?? 'master';
  if (kind === 'sealed') {
    return boxOpen(
      blob,
      fromBase64(vault.public_key_b64),
      fromBase64(vault.private_key_b64)
    );
  }
  return openSecretbox(blob, fromBase64(vault.master_key_b64));
}

export function decryptItemBody(item: Item, vault: Vault): string {
  if (!item.encrypted_body) return '';
  const itemKey = unwrapItemKey(item, vault);
  const plain = openSecretbox(fromBase64(item.encrypted_body), itemKey);
  return utf8Decode(plain);
}

export function decryptSecret(item: Item, vault: Vault): SecretPayload | null {
  if (item.type !== 'secret') return null;
  try {
    const text = decryptItemBody(item, vault);
    if (!text) return null;
    const parsed = JSON.parse(text) as Partial<SecretPayload>;
    return {
      username: parsed.username ?? '',
      password: parsed.password ?? '',
      url: parsed.url ?? '',
      totp_seed: parsed.totp_seed ?? '',
      notes: parsed.notes ?? ''
    };
  } catch {
    return null;
  }
}

/// Match a saved secret's URL against the current tab URL. Compares hosts
/// after normalising — example.com matches m.example.com and
/// foo.example.com (suffix match), and exact-host always matches.
export function urlMatches(savedUrl: string, currentHost: string): boolean {
  if (!savedUrl) return false;
  let savedHost: string;
  try {
    savedHost = new URL(savedUrl).hostname.toLowerCase();
  } catch {
    return false;
  }
  if (!savedHost) return false;
  const cur = currentHost.toLowerCase();
  if (savedHost === cur) return true;
  // Suffix match on label boundary: "github.com" matches "api.github.com"
  // but not "githubsupport.com".
  return cur.endsWith('.' + savedHost) || savedHost.endsWith('.' + cur);
}
