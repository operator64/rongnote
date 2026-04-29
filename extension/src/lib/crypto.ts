// Mirror of web/src/lib/crypto.ts — same scheme, same byte layout. Only
// the subset the extension needs (KEK derivation + secretbox open/seal +
// box_open for sealed wraps + auth-hash + base64). Keep this in sync with
// the canonical web client.

// libsodium-wrappers-sumo ships no types; we lean on the same runtime API
// the SPA uses and treat the module as `any`.
// @ts-expect-error untyped CJS-via-ESM module
import sodium from 'libsodium-wrappers-sumo';

let readyPromise: Promise<void> | null = null;

export function ensureReady(): Promise<void> {
  if (!readyPromise) readyPromise = sodium.ready;
  return readyPromise!;
}

const SALT_LEN = 16;
const NONCE_LEN = 24;
const KEY_LEN = 32;
const MAC_LEN = 16;

const AUTH_LABEL = 'rongnote-auth-v1';

export async function deriveKekFromPassphrase(
  passphrase: string,
  salt: Uint8Array
): Promise<Uint8Array> {
  if (salt.length !== SALT_LEN) {
    throw new Error(`salt must be ${SALT_LEN} bytes, got ${salt.length}`);
  }
  return sodium.crypto_pwhash(
    KEY_LEN,
    passphrase,
    salt,
    sodium.crypto_pwhash_OPSLIMIT_INTERACTIVE,
    sodium.crypto_pwhash_MEMLIMIT_INTERACTIVE,
    sodium.crypto_pwhash_ALG_ARGON2ID13
  );
}

export function deriveAuthHash(masterKey: Uint8Array): Uint8Array {
  return sodium.crypto_generichash(KEY_LEN, AUTH_LABEL, masterKey);
}

/// Open a `nonce(24) || ciphertext` blob with a 32-byte secretbox key.
export function open(combined: Uint8Array, key: Uint8Array): Uint8Array {
  if (key.length !== KEY_LEN) {
    throw new Error(`key must be ${KEY_LEN} bytes, got ${key.length}`);
  }
  if (combined.length < NONCE_LEN + MAC_LEN) {
    throw new Error('ciphertext too short');
  }
  const nonce = combined.subarray(0, NONCE_LEN);
  const ct = combined.subarray(NONCE_LEN);
  return sodium.crypto_secretbox_open_easy(ct, nonce, key);
}

/// Anonymous-box decrypt with our X25519 keypair (for team-space items).
export function boxOpen(
  sealed: Uint8Array,
  publicKey: Uint8Array,
  privateKey: Uint8Array
): Uint8Array {
  return sodium.crypto_box_seal_open(sealed, publicKey, privateKey);
}

export function fromBase64(s: string): Uint8Array {
  return sodium.from_base64(s, sodium.base64_variants.ORIGINAL);
}

export function toBase64(bytes: Uint8Array): string {
  return sodium.to_base64(bytes, sodium.base64_variants.ORIGINAL);
}

export function utf8Decode(b: Uint8Array): string {
  return new TextDecoder().decode(b);
}
