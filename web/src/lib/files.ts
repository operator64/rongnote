// Client-side file encryption + upload helpers.

import { api, type Item } from './api';
import {
  fromBase64,
  generateItemKey,
  open as openSeal,
  seal,
  toBase64,
  utf8Decode,
  utf8Encode
} from './crypto';
import { unwrapItemKey, wrapItemKey } from './itemCrypto';
import { items } from './items.svelte';
import { spaces } from './spaces.svelte';

export interface FileMeta {
  filename: string;
  mime: string;
  /// plaintext byte length, before encryption.
  size: number;
}

/// Encrypt a File picked from the browser, upload the ciphertext blob, then
/// create the item with encrypted metadata + a reference to the blob. The
/// item is created in the active space (personal vs team handled inside
/// encryptBodyForSpace).
export async function uploadFile(input: {
  file: File;
  masterKey: Uint8Array;
  path?: string;
  tags?: string[];
}): Promise<Item> {
  const bytes = new Uint8Array(await input.file.arrayBuffer());

  const itemKey = generateItemKey();
  const encryptedBytes = seal(bytes, itemKey);

  const upload = await api.uploadBlob(encryptedBytes);

  const meta: FileMeta = {
    filename: input.file.name,
    mime: input.file.type || 'application/octet-stream',
    size: bytes.length
  };

  const spaceId = spaces.activeId ?? spaces.personal()?.id ?? '';
  const encryptedBody = seal(utf8Encode(JSON.stringify(meta)), itemKey);
  const wrap = await wrapItemKey(itemKey, spaceId, input.masterKey);

  const item = await api.createItem({
    type: 'file',
    title: input.file.name,
    encrypted_body: toBase64(encryptedBody),
    ...wrap,
    blob_sha256: upload.sha256,
    tags: input.tags ?? [],
    path: input.path ?? '/',
    space_id: spaceId || undefined
  });
  items.upsert(item);
  return item;
}

/// Decrypt the metadata JSON for an existing file item. Falls back to a
/// title-only stub when the vault can't unwrap (locked or wrong keys).
export function decryptFileMeta(
  item: Item,
  masterKey: Uint8Array,
  publicKey: Uint8Array,
  privateKey: Uint8Array
): FileMeta {
  if (!item.encrypted_body || !item.wrapped_item_key) {
    return { filename: item.title || 'file', mime: 'application/octet-stream', size: 0 };
  }
  const itemKey = unwrapItemKey(item, masterKey, publicKey, privateKey);
  const bytes = openSeal(fromBase64(item.encrypted_body), itemKey);
  return JSON.parse(utf8Decode(bytes)) as FileMeta;
}

/// Fetch + decrypt the file content. Returns the plaintext bytes.
export async function downloadFileBytes(
  item: Item,
  masterKey: Uint8Array,
  publicKey: Uint8Array,
  privateKey: Uint8Array
): Promise<Uint8Array> {
  if (!item.blob_sha256 || !item.wrapped_item_key) {
    throw new Error('item has no blob');
  }
  const itemKey = unwrapItemKey(item, masterKey, publicKey, privateKey);
  const cipher = await api.fetchBlob(item.blob_sha256);
  return openSeal(cipher, itemKey);
}

export function humanSize(bytes: number): string {
  if (!Number.isFinite(bytes) || bytes < 0) return '?';
  if (bytes < 1024) return `${bytes} B`;
  if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`;
  if (bytes < 1024 * 1024 * 1024) return `${(bytes / 1024 / 1024).toFixed(1)} MB`;
  return `${(bytes / 1024 / 1024 / 1024).toFixed(2)} GB`;
}

// utf8Encode kept here so callers don't need to import from crypto. Unused
// by current callers but exported for parity with previous module surface.
export { utf8Encode };
