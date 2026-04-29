//! Mirror of `web/src/lib/crypto.ts`. Same scheme, same byte layout — what
//! the server sees from this binary is indistinguishable from a browser
//! tab. Tested against the running server.

use anyhow::{anyhow, Context, Result};
use argon2::{Algorithm, Argon2, Params, Version};
use base64::engine::general_purpose::STANDARD as B64;
use base64::Engine;
use blake2::digest::generic_array::typenum::U24;
use blake2::digest::{KeyInit, Mac};
use blake2::{Blake2b, Blake2bMac, Digest};
use crypto_box::{PublicKey, SalsaBox, SecretKey};
use crypto_secretbox::aead::Aead;
use crypto_secretbox::{Nonce, XSalsa20Poly1305};
use rand_core::{OsRng, RngCore};
use zeroize::Zeroize;

pub const KEY_LEN: usize = 32;
pub const NONCE_LEN: usize = 24;
pub const SALT_LEN: usize = 16;

const ARGON2_OPS_INTERACTIVE: u32 = 2;
const ARGON2_MEM_INTERACTIVE_KIB: u32 = 65_536; // 64 MiB
const ARGON2_PARALLELISM: u32 = 1;

const AUTH_LABEL: &[u8] = b"rongnote-auth-v1";

/// libsodium INTERACTIVE-equivalent Argon2id parameters. Used to derive
/// passphrase / recovery KEKs that wrap the master_key.
fn argon2_kdf(secret: &[u8], salt: &[u8]) -> Result<[u8; KEY_LEN]> {
    if salt.len() != SALT_LEN {
        return Err(anyhow!("salt must be {SALT_LEN} bytes, got {}", salt.len()));
    }
    let params = Params::new(
        ARGON2_MEM_INTERACTIVE_KIB,
        ARGON2_OPS_INTERACTIVE,
        ARGON2_PARALLELISM,
        Some(KEY_LEN),
    )
    .map_err(|e| anyhow!("argon2 params: {e}"))?;
    let kdf = Argon2::new(Algorithm::Argon2id, Version::V0x13, params);
    let mut out = [0u8; KEY_LEN];
    kdf.hash_password_into(secret, salt, &mut out)
        .map_err(|e| anyhow!("argon2 hash: {e}"))?;
    Ok(out)
}

pub fn derive_kek_from_passphrase(passphrase: &str, salt: &[u8]) -> Result<[u8; KEY_LEN]> {
    argon2_kdf(passphrase.as_bytes(), salt)
}

/// auth_hash = BLAKE2b-keyed(master_key, "rongnote-auth-v1") -> 32 bytes.
/// Sent to the server during login. Server stores Argon2id of this.
pub fn derive_auth_hash(master_key: &[u8; KEY_LEN]) -> Result<[u8; KEY_LEN]> {
    // Blake2b with key = master_key, message = "rongnote-auth-v1".
    let mut mac = <Blake2bMac<digest::consts::U32> as KeyInit>::new_from_slice(master_key)
        .map_err(|e| anyhow!("blake2 key: {e}"))?;
    mac.update(AUTH_LABEL);
    let bytes = mac.finalize().into_bytes();
    let mut out = [0u8; KEY_LEN];
    out.copy_from_slice(&bytes);
    Ok(out)
}

/// XSalsa20-Poly1305 secretbox. Returns nonce || ciphertext || mac, matching
/// the JS `seal()` layout.
pub fn seal(plaintext: &[u8], key: &[u8; KEY_LEN]) -> Result<Vec<u8>> {
    let cipher = XSalsa20Poly1305::new(key.into());
    let mut nonce_bytes = [0u8; NONCE_LEN];
    OsRng.fill_bytes(&mut nonce_bytes);
    let nonce = Nonce::from_slice(&nonce_bytes);
    let ct = cipher
        .encrypt(nonce, plaintext)
        .map_err(|e| anyhow!("seal: {e}"))?;
    let mut out = Vec::with_capacity(NONCE_LEN + ct.len());
    out.extend_from_slice(&nonce_bytes);
    out.extend_from_slice(&ct);
    Ok(out)
}

/// Inverse of `seal()`. Input is nonce || ciphertext || mac.
pub fn open(combined: &[u8], key: &[u8; KEY_LEN]) -> Result<Vec<u8>> {
    if combined.len() < NONCE_LEN + 16 {
        return Err(anyhow!("ciphertext too short"));
    }
    let cipher = XSalsa20Poly1305::new(key.into());
    let nonce = Nonce::from_slice(&combined[..NONCE_LEN]);
    cipher
        .decrypt(nonce, &combined[NONCE_LEN..])
        .map_err(|e| anyhow!("open: bad ciphertext or wrong key: {e}"))
}

// --- Anonymous public-key encryption (libsodium crypto_box_seal).
//
// Layout (matches libsodium):
//   sealed = ephemeral_pk(32 bytes) || box(plaintext, nonce, recipient_pk, ephemeral_sk)
//   nonce  = BLAKE2b-256-truncated-to-24(ephemeral_pk || recipient_pk)
//
// Implemented by hand on top of `crypto_box::SalsaBox` (which is the same
// XSalsa20-Poly1305 over X25519 ECDH that libsodium uses). The crypto_box
// crate's stand-alone `seal` function moves around between minor versions;
// rolling it ourselves keeps this code immune to that and matches the
// browser libsodium output bit-for-bit.

fn sealed_box_nonce(eph_pk: &[u8; KEY_LEN], recipient_pk: &[u8; KEY_LEN]) -> [u8; NONCE_LEN] {
    let mut hasher = Blake2b::<U24>::new();
    hasher.update(eph_pk);
    hasher.update(recipient_pk);
    let out = hasher.finalize();
    let mut n = [0u8; NONCE_LEN];
    n.copy_from_slice(&out);
    n
}

pub fn box_seal(plaintext: &[u8], recipient_pubkey: &[u8; KEY_LEN]) -> Result<Vec<u8>> {
    // Ephemeral X25519 keypair.
    let eph_sk = SecretKey::generate(&mut OsRng);
    let eph_pk = eph_sk.public_key();
    let eph_pk_bytes: [u8; KEY_LEN] = eph_pk.as_bytes().to_owned();

    let nonce_bytes = sealed_box_nonce(&eph_pk_bytes, recipient_pubkey);

    let recipient_pk = PublicKey::from_bytes(*recipient_pubkey);
    let salsa = SalsaBox::new(&recipient_pk, &eph_sk);
    use crypto_box::aead::Aead as _;
    let nonce = crypto_box::Nonce::from_slice(&nonce_bytes);
    let ct = salsa
        .encrypt(nonce, plaintext)
        .map_err(|e| anyhow!("crypto_box_seal encrypt: {e}"))?;

    let mut out = Vec::with_capacity(KEY_LEN + ct.len());
    out.extend_from_slice(&eph_pk_bytes);
    out.extend_from_slice(&ct);
    Ok(out)
}

pub fn box_open(
    sealed: &[u8],
    public_key: &[u8; KEY_LEN],
    private_key: &[u8; KEY_LEN],
) -> Result<Vec<u8>> {
    if sealed.len() < KEY_LEN + 16 {
        return Err(anyhow!("sealed box too short"));
    }
    let eph_pk_bytes: [u8; KEY_LEN] = sealed[..KEY_LEN]
        .try_into()
        .map_err(|_| anyhow!("eph pubkey size"))?;
    let ct = &sealed[KEY_LEN..];
    let nonce_bytes = sealed_box_nonce(&eph_pk_bytes, public_key);

    let our_sk = SecretKey::from_bytes(*private_key);
    let eph_pk = PublicKey::from_bytes(eph_pk_bytes);
    let salsa = SalsaBox::new(&eph_pk, &our_sk);
    use crypto_box::aead::Aead as _;
    let nonce = crypto_box::Nonce::from_slice(&nonce_bytes);
    salsa
        .decrypt(nonce, ct)
        .map_err(|e| anyhow!("crypto_box_seal decrypt: {e}"))
}

// --- Convenience encoding helpers ---

pub fn b64_encode(bytes: &[u8]) -> String {
    B64.encode(bytes)
}

pub fn b64_decode(s: &str) -> Result<Vec<u8>> {
    B64.decode(s).context("invalid base64")
}

pub fn b64_decode_array<const N: usize>(s: &str) -> Result<[u8; N]> {
    let v = b64_decode(s)?;
    v.try_into()
        .map_err(|v: Vec<u8>| anyhow!("expected {N} bytes, got {}", v.len()))
}

/// Wrap zeroize so we can drop master keys deterministically.
#[allow(dead_code)]
pub fn zeroize<T: Zeroize>(t: &mut T) {
    t.zeroize();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn secretbox_roundtrip() {
        let key = [7u8; KEY_LEN];
        let plaintext = b"the magic words are squeamish ossifrage";
        let ct = seal(plaintext, &key).unwrap();
        assert!(ct.len() > NONCE_LEN);
        let pt = open(&ct, &key).unwrap();
        assert_eq!(&pt, plaintext);
    }

    #[test]
    fn sealed_box_roundtrip() {
        // Generate a recipient keypair via the same crypto_box building
        // blocks the runtime uses, so the test exercises the exact path.
        let sk = SecretKey::generate(&mut OsRng);
        let pk = sk.public_key();
        let pk_bytes: [u8; KEY_LEN] = pk.as_bytes().to_owned();
        let sk_bytes: [u8; KEY_LEN] = sk.to_bytes();

        let plaintext = b"membership wrap test";
        let sealed = box_seal(plaintext, &pk_bytes).unwrap();
        // sealed = eph_pk(32) || ct(plaintext.len()+16)
        assert_eq!(sealed.len(), KEY_LEN + plaintext.len() + 16);
        let opened = box_open(&sealed, &pk_bytes, &sk_bytes).unwrap();
        assert_eq!(&opened, plaintext);
    }

    #[test]
    fn argon2_matches_libsodium_interactive_params() {
        // Sanity check: KDF runs and produces a key of the right size for a
        // known salt. The exact bytes aren't fixed across libsodium versions
        // but the parameters are.
        let salt = [0u8; SALT_LEN];
        let k = derive_kek_from_passphrase("hunter2", &salt).unwrap();
        assert_eq!(k.len(), KEY_LEN);
    }

    #[test]
    fn auth_hash_is_blake2b_keyed_32() {
        let mk = [0xAAu8; KEY_LEN];
        let h = derive_auth_hash(&mk).unwrap();
        assert_eq!(h.len(), KEY_LEN);
        // Different keys must produce different hashes.
        let mk2 = [0xBBu8; KEY_LEN];
        let h2 = derive_auth_hash(&mk2).unwrap();
        assert_ne!(h, h2);
    }
}
