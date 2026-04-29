// Single helper used by every editor (and dev-seed, files.ts) so that
// personal-space vs team-space wrapping logic lives in one place.
//
// Personal-space items wrap their per-item key with the user's master_key
// (XSalsa20-Poly1305) and rotate the key on every save. Team-space items
// wrap the per-item key separately for each current member using libsodium
// crypto_box_seal against that member's X25519 public key, and *reuse* the
// same item_key across saves so version snapshots remain decryptable
// (item_member_keys only stores the current wrap, not per-version).

import { api, type Item, type MemberKeyInput, type Member } from './api';
import {
  boxOpen,
  boxSeal,
  fromBase64,
  generateItemKey,
  open as openSecretbox,
  seal as sealSecretbox,
  toBase64
} from './crypto';
import { spaces } from './spaces.svelte';

export interface WrappedKey {
  wrapped_item_key?: string;
  member_keys?: MemberKeyInput[];
}

/// Encrypt `body` for create or update. The shape returned plugs straight
/// into CreateItemInput / UpdateItemInput.
///
/// - personal space: rotates the item_key, returns wrapped_item_key.
/// - team space, no `item`: generates a fresh item_key and wraps it for
///   every current member. Returns member_keys.
/// - team space, with existing `item`: unwraps the *current* item_key from
///   `item.wrapped_item_key` (which the server populated with the caller's
///   sealed wrap) and re-encrypts the body with that same key. Returns
///   nothing for the wrap — server keeps the existing item_member_keys
///   rows untouched.
export async function encryptBodyForSpace(opts: {
  body: string | Uint8Array;
  spaceId: string;
  masterKey: Uint8Array;
  publicKey: Uint8Array;
  privateKey: Uint8Array;
  item?: Item;
}): Promise<{ encrypted_body: string } & WrappedKey> {
  const utf8 = typeof opts.body === 'string' ? new TextEncoder().encode(opts.body) : opts.body;
  const space = spaces.list.find((s) => s.id === opts.spaceId);
  const isTeam = space?.kind === 'team';

  if (isTeam && opts.item && opts.item.wrapped_item_key) {
    // Reuse the existing item_key — decrypt the caller's current sealed wrap.
    const existingKey = unwrapItemKey(opts.item, opts.masterKey, opts.publicKey, opts.privateKey);
    return {
      encrypted_body: toBase64(sealSecretbox(utf8, existingKey))
    };
  }

  if (isTeam) {
    // First-time team-space encryption (create, or update of an item with
    // no body yet).
    const itemKey = generateItemKey();
    const members = await api.listMembers(opts.spaceId);
    return {
      encrypted_body: toBase64(sealSecretbox(utf8, itemKey)),
      member_keys: sealForMembers(itemKey, members)
    };
  }

  // Personal space — rotate freely.
  const itemKey = generateItemKey();
  return {
    encrypted_body: toBase64(sealSecretbox(utf8, itemKey)),
    wrapped_item_key: toBase64(sealSecretbox(itemKey, opts.masterKey))
  };
}

/// Helper used by Phase C (member invite) and the team-space create path.
export function sealForMembers(itemKey: Uint8Array, members: Member[]): MemberKeyInput[] {
  return members.map((m) => ({
    user_id: m.user_id,
    sealed_item_key: toBase64(boxSeal(itemKey, fromBase64(m.public_key)))
  }));
}

/// Wrap an externally-generated `itemKey` for a target space. Used by file
/// uploads where the same itemKey encrypts both the blob bytes and the
/// metadata payload.
export async function wrapItemKey(
  itemKey: Uint8Array,
  spaceId: string,
  masterKey: Uint8Array
): Promise<WrappedKey> {
  const space = spaces.list.find((s) => s.id === spaceId);
  if (space?.kind === 'team') {
    const members = await api.listMembers(spaceId);
    return { member_keys: sealForMembers(itemKey, members) };
  }
  return { wrapped_item_key: toBase64(sealSecretbox(itemKey, masterKey)) };
}

/// Decrypt the `wrapped_item_key` blob the server returned for `item`. Picks
/// secretbox vs sealed-box based on `item.key_wrap`. Throws if no wrap.
export function unwrapItemKey(
  item: Item,
  masterKey: Uint8Array,
  publicKey: Uint8Array,
  privateKey: Uint8Array
): Uint8Array {
  if (!item.wrapped_item_key) {
    throw new Error('item has no wrap');
  }
  const blob = fromBase64(item.wrapped_item_key);
  // Default to 'master' for items predating the discriminator — the field
  // is missing from cached responses on personal-only databases.
  const kind = item.key_wrap ?? 'master';
  if (kind === 'sealed') {
    return boxOpen(blob, publicKey, privateKey);
  }
  return openSecretbox(blob, masterKey);
}

/// Convenience: unwrap and decrypt the item body in one shot.
export function decryptItemBody(
  item: Item,
  masterKey: Uint8Array,
  publicKey: Uint8Array,
  privateKey: Uint8Array
): string {
  if (!item.encrypted_body) return '';
  const itemKey = unwrapItemKey(item, masterKey, publicKey, privateKey);
  const plain = openSecretbox(fromBase64(item.encrypted_body), itemKey);
  return new TextDecoder().decode(plain);
}
