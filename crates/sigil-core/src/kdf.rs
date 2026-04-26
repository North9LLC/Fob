use argon2::{Algorithm, Argon2, Params, Version};
use hkdf::Hkdf;
use sha2::Sha256;
use zeroize::{Zeroize, ZeroizeOnDrop};

use crate::error::{Error, Result};

/// Argon2id parameters for interactive use.
/// 256 MiB memory, 4 iterations, 4 lanes.
const ARGON2_MEMORY_KIB: u32 = 256 * 1024;
const ARGON2_ITERATIONS: u32 = 4;
const ARGON2_PARALLELISM: u32 = 4;
const ARGON2_OUTPUT_LEN: usize = 64;

/// Full 64-byte output of Argon2id key derivation.
/// First 32 bytes → master_secret; last 32 bytes → nonce_seed.
#[derive(Zeroize, ZeroizeOnDrop)]
pub struct KdfOutput([u8; ARGON2_OUTPUT_LEN]);

impl KdfOutput {
    pub fn master_secret(&self) -> &[u8; 32] {
        self.0[..32].try_into().unwrap()
    }

    pub fn nonce_seed(&self) -> &[u8; 32] {
        self.0[32..].try_into().unwrap()
    }
}

/// Derive the 64-byte KDF output from a passphrase and a 32-byte salt.
///
/// The salt must be random (from CSPRNG) and stored in the vault header.
/// Returns an error if Argon2id cannot allocate the required 256 MiB of memory.
pub fn derive_master(passphrase: &[u8], salt: &[u8; 32]) -> Result<KdfOutput> {
    let params = Params::new(
        ARGON2_MEMORY_KIB,
        ARGON2_ITERATIONS,
        ARGON2_PARALLELISM,
        Some(ARGON2_OUTPUT_LEN),
    )
    .map_err(|e| Error::Kdf(e.to_string()))?;

    let argon2 = Argon2::new(Algorithm::Argon2id, Version::V0x13, params);

    let mut output = KdfOutput([0u8; ARGON2_OUTPUT_LEN]);
    argon2
        .hash_password_into(passphrase, salt, &mut output.0)
        .map_err(|e| Error::Kdf(e.to_string()))?;

    Ok(output)
}

/// HKDF-SHA256 key derivation for individual vault slot keys.
///
/// `master_secret` is the first 32 bytes of KdfOutput.
/// `info` is a domain-separation string like `"sigil/v1/main"`.
pub fn derive_slot_key(master_secret: &[u8; 32], info: &[u8]) -> [u8; 32] {
    let hk = Hkdf::<Sha256>::new(None, master_secret);
    let mut key = [0u8; 32];
    hk.expand(info, &mut key)
        .expect("HKDF expand output length is always valid for 32 bytes");
    key
}

/// Domain-separated labels for each vault slot.
pub const SLOT_LABELS: [&[u8]; 4] = [
    b"sigil/v1/main",
    b"sigil/v1/decoy",
    b"sigil/v1/duress",
    b"sigil/v1/reserved",
];

/// Derive all four slot keys from a master secret.
pub fn derive_all_slot_keys(master_secret: &[u8; 32]) -> [[u8; 32]; 4] {
    [
        derive_slot_key(master_secret, SLOT_LABELS[0]),
        derive_slot_key(master_secret, SLOT_LABELS[1]),
        derive_slot_key(master_secret, SLOT_LABELS[2]),
        derive_slot_key(master_secret, SLOT_LABELS[3]),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_salt() -> [u8; 32] {
        [0x42u8; 32]
    }

    #[test]
    fn derive_master_produces_64_bytes() {
        let out = derive_master(b"test-passphrase", &test_salt()).unwrap();
        assert_ne!(out.master_secret(), &[0u8; 32]);
        assert_ne!(out.nonce_seed(), &[0u8; 32]);
        assert_ne!(out.master_secret(), out.nonce_seed());
    }

    #[test]
    fn derive_master_is_deterministic() {
        let a = derive_master(b"hunter2", &test_salt()).unwrap();
        let b = derive_master(b"hunter2", &test_salt()).unwrap();
        assert_eq!(a.master_secret(), b.master_secret());
        assert_eq!(a.nonce_seed(), b.nonce_seed());
    }

    #[test]
    fn derive_master_different_passphrases() {
        let a = derive_master(b"passphrase-a", &test_salt()).unwrap();
        let b = derive_master(b"passphrase-b", &test_salt()).unwrap();
        assert_ne!(a.master_secret(), b.master_secret());
    }

    #[test]
    fn derive_master_different_salts() {
        let salt_a = [0x11u8; 32];
        let salt_b = [0x22u8; 32];
        let a = derive_master(b"same-passphrase", &salt_a).unwrap();
        let b = derive_master(b"same-passphrase", &salt_b).unwrap();
        assert_ne!(a.master_secret(), b.master_secret());
    }

    #[test]
    fn slot_keys_are_distinct() {
        let master = [0xABu8; 32];
        let keys = derive_all_slot_keys(&master);
        assert_ne!(keys[0], keys[1]);
        assert_ne!(keys[0], keys[2]);
        assert_ne!(keys[1], keys[2]);
        assert_ne!(keys[0], keys[3]);
    }

    #[test]
    fn slot_keys_are_deterministic() {
        let master = [0xABu8; 32];
        let a = derive_all_slot_keys(&master);
        let b = derive_all_slot_keys(&master);
        assert_eq!(a, b);
    }

    #[test]
    fn slot_key_changes_with_master() {
        let a = [0x01u8; 32];
        let b = [0x02u8; 32];
        assert_ne!(
            derive_slot_key(&a, SLOT_LABELS[0]),
            derive_slot_key(&b, SLOT_LABELS[0])
        );
    }
}
