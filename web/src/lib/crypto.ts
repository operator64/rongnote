// All client-side crypto. Server never sees plaintext bodies, master key,
// passphrase, recovery code, or X25519 private key.
//
// Scheme (v0.3):
//   master_key       = random 32 bytes (generated once on register)
//   passphrase_kek   = Argon2id(passphrase,    passphrase_salt) -> 32 bytes
//   recovery_kek     = Argon2id(recovery_code, recovery_salt)   -> 32 bytes
//   master_wrap_*    = nonce(24) || secretbox(master_key, nonce, kek)
//   keypair          = X25519 keypair, generated once on register
//   encrypted_private_key = nonce(24) || secretbox(privkey, nonce, master_key)
//   auth_hash        = BLAKE2b-keyed(master_key, "rongnote-auth-v1") -> 32 bytes
//   per-item:
//     item_key       = random 32 bytes
//     wrapped_item_key = nonce(24) || secretbox(item_key, nonce, master_key)
//     encrypted_body = nonce(24) || secretbox(utf8(body), nonce, item_key)
//
// Recovery code is shown ONCE on register. It lets the user reset their
// passphrase by re-wrapping master_key with a new passphrase_kek.
//
// secretbox = XSalsa20-Poly1305 (libsodium crypto_secretbox_easy).

import sodium from 'libsodium-wrappers-sumo';

let readyPromise: Promise<void> | null = null;

export function ensureReady(): Promise<void> {
  if (!readyPromise) readyPromise = sodium.ready;
  return readyPromise;
}

const SALT_LEN = 16;
const NONCE_LEN = 24;
const KEY_LEN = 32;
const MAC_LEN = 16;
const RECOVERY_BYTES = 15; // 120 bits → 24 base32 chars

const AUTH_LABEL = 'rongnote-auth-v1';

// --- Random helpers ---

export function generateSalt(): Uint8Array {
  return sodium.randombytes_buf(SALT_LEN);
}

export function generateMasterKey(): Uint8Array {
  return sodium.randombytes_buf(KEY_LEN);
}

export function generateItemKey(): Uint8Array {
  return sodium.randombytes_buf(KEY_LEN);
}

export function generateKeyPair(): { publicKey: Uint8Array; privateKey: Uint8Array } {
  const kp = sodium.crypto_box_keypair();
  return { publicKey: kp.publicKey, privateKey: kp.privateKey };
}

// --- KDF ---

async function deriveKek(secret: string, salt: Uint8Array): Promise<Uint8Array> {
  if (salt.length !== SALT_LEN) {
    throw new Error(`salt must be ${SALT_LEN} bytes, got ${salt.length}`);
  }
  return sodium.crypto_pwhash(
    KEY_LEN,
    secret,
    salt,
    sodium.crypto_pwhash_OPSLIMIT_INTERACTIVE,
    sodium.crypto_pwhash_MEMLIMIT_INTERACTIVE,
    sodium.crypto_pwhash_ALG_ARGON2ID13
  );
}

export function deriveKekFromPassphrase(
  passphrase: string,
  salt: Uint8Array
): Promise<Uint8Array> {
  return deriveKek(passphrase, salt);
}

export function deriveKekFromRecoveryCode(
  code: string,
  salt: Uint8Array
): Promise<Uint8Array> {
  // Canonicalize before deriving so the user can type with/without dashes
  // and in any case.
  return deriveKek(canonicalizeRecoveryCode(code), salt);
}

export function deriveAuthHash(masterKey: Uint8Array): Uint8Array {
  return sodium.crypto_generichash(KEY_LEN, AUTH_LABEL, masterKey);
}

// --- Symmetric crypto ---

/// Encrypt with XSalsa20-Poly1305. Returns nonce || ciphertext.
export function seal(plaintext: Uint8Array, key: Uint8Array): Uint8Array {
  if (key.length !== KEY_LEN) {
    throw new Error(`key must be ${KEY_LEN} bytes, got ${key.length}`);
  }
  const nonce = sodium.randombytes_buf(NONCE_LEN);
  const ct = sodium.crypto_secretbox_easy(plaintext, nonce, key);
  const out = new Uint8Array(nonce.length + ct.length);
  out.set(nonce, 0);
  out.set(ct, nonce.length);
  return out;
}

/// Decrypt the nonce || ciphertext blob produced by seal().
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

// --- Recovery code: 15 random bytes -> 24 base32 chars, dashed every 4 ---

const BASE32_ALPHABET = 'ABCDEFGHIJKLMNOPQRSTUVWXYZ234567';

function base32Encode(bytes: Uint8Array): string {
  let bits = 0;
  let value = 0;
  let out = '';
  for (const b of bytes) {
    value = (value << 8) | b;
    bits += 8;
    while (bits >= 5) {
      out += BASE32_ALPHABET[(value >>> (bits - 5)) & 0x1f];
      bits -= 5;
    }
  }
  if (bits > 0) out += BASE32_ALPHABET[(value << (5 - bits)) & 0x1f];
  return out;
}

export function generateRecoveryCode(): string {
  const bytes = sodium.randombytes_buf(RECOVERY_BYTES);
  const raw = base32Encode(bytes);
  return raw.match(/.{1,4}/g)!.join('-');
}

/// Strip whitespace + dashes, uppercase. Lets users type
/// "abcd efgh-ijkl mnop qrst uvwx" or all variations.
export function canonicalizeRecoveryCode(input: string): string {
  return input.replace(/[\s-]+/g, '').toUpperCase();
}

export function isValidRecoveryCode(input: string): boolean {
  const c = canonicalizeRecoveryCode(input);
  if (c.length !== 24) return false;
  for (const ch of c) {
    if (!BASE32_ALPHABET.includes(ch)) return false;
  }
  return true;
}

// --- Encoding helpers ---

export function toBase64(bytes: Uint8Array): string {
  return sodium.to_base64(bytes, sodium.base64_variants.ORIGINAL);
}

export function fromBase64(s: string): Uint8Array {
  return sodium.from_base64(s, sodium.base64_variants.ORIGINAL);
}

const utf8Enc = new TextEncoder();
const utf8Dec = new TextDecoder();

export function utf8Encode(s: string): Uint8Array {
  return utf8Enc.encode(s);
}

export function utf8Decode(b: Uint8Array): string {
  return utf8Dec.decode(b);
}

export const KEY_BYTES = KEY_LEN;
export const NONCE_BYTES = NONCE_LEN;
export const SALT_BYTES = SALT_LEN;
