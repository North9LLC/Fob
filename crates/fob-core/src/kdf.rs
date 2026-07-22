use hkdf::Hkdf;
use pbkdf2::pbkdf2_hmac;
use sha2::Sha256;

use crate::mem::LockedSecret;

/// PBKDF2-HMAC-SHA256 master secret, derived from a passphrase and salt.
///
/// PBKDF2 is used (rather than a memory-hard KDF like Argon2id) because the
/// browser vault must derive the exact same key using only native WebCrypto
/// — WebCrypto has no Argon2id primitive. This keeps the CLI and browser
/// vault on one interoperable format instead of two.
///
/// Backed by `LockedSecret`: mlocked (best-effort, non-fatal if the OS
/// denies it) and excluded from core dumps for as long as it's alive, and
/// zeroized + munlocked on drop — not just zeroized.
pub struct KdfOutput(LockedSecret<32>);

impl KdfOutput {
    pub fn master_secret(&self) -> &[u8; 32] {
        self.0.bytes()
    }
}

/// Derive the 32-byte master secret from a passphrase, a 32-byte salt, and
/// an iteration count.
///
/// The salt must be random (from CSPRNG) and stored in the vault header.
pub fn derive_master(passphrase: &[u8], salt: &[u8; 32], iterations: u32) -> KdfOutput {
    let mut output = [0u8; 32];
    pbkdf2_hmac::<Sha256>(passphrase, salt, iterations, &mut output);
    KdfOutput(LockedSecret::new(output))
}

/// HKDF-SHA256 key derivation for individual vault slot keys.
///
/// `master_secret` is the PBKDF2 output above.
/// `info` is a domain-separation string like `"fob/v1/main"`.
pub fn derive_slot_key(master_secret: &[u8; 32], info: &[u8]) -> [u8; 32] {
    let hk = Hkdf::<Sha256>::new(None, master_secret);
    let mut key = [0u8; 32];
    hk.expand(info, &mut key)
        .expect("HKDF expand output length is always valid for 32 bytes");
    key
}

/// Domain-separated labels for each vault slot.
pub const SLOT_LABELS: [&[u8]; 4] = [
    b"fob/v1/main",
    b"fob/v1/decoy",
    b"fob/v1/duress",
    b"fob/v1/reserved",
];

/// Derive all four slot keys from a master secret.
///
/// Each key is individually mlocked and zeroized on drop via `LockedSecret`
/// — these are real AES-256-GCM keys capable of decrypting the whole vault,
/// not incidental scratch data.
pub fn derive_all_slot_keys(master_secret: &[u8; 32]) -> [LockedSecret<32>; 4] {
    [
        LockedSecret::new(derive_slot_key(master_secret, SLOT_LABELS[0])),
        LockedSecret::new(derive_slot_key(master_secret, SLOT_LABELS[1])),
        LockedSecret::new(derive_slot_key(master_secret, SLOT_LABELS[2])),
        LockedSecret::new(derive_slot_key(master_secret, SLOT_LABELS[3])),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    const TEST_ITERATIONS: u32 = 1000; // small, fast iteration count for tests

    fn test_salt() -> [u8; 32] {
        [0x42u8; 32]
    }

    #[test]
    fn derive_master_produces_32_bytes() {
        let out = derive_master(b"test-passphrase", &test_salt(), TEST_ITERATIONS);
        assert_ne!(out.master_secret(), &[0u8; 32]);
    }

    #[test]
    fn derive_master_is_deterministic() {
        let a = derive_master(b"hunter2", &test_salt(), TEST_ITERATIONS);
        let b = derive_master(b"hunter2", &test_salt(), TEST_ITERATIONS);
        assert_eq!(a.master_secret(), b.master_secret());
    }

    #[test]
    fn derive_master_different_passphrases() {
        let a = derive_master(b"passphrase-a", &test_salt(), TEST_ITERATIONS);
        let b = derive_master(b"passphrase-b", &test_salt(), TEST_ITERATIONS);
        assert_ne!(a.master_secret(), b.master_secret());
    }

    #[test]
    fn derive_master_different_salts() {
        let salt_a = [0x11u8; 32];
        let salt_b = [0x22u8; 32];
        let a = derive_master(b"same-passphrase", &salt_a, TEST_ITERATIONS);
        let b = derive_master(b"same-passphrase", &salt_b, TEST_ITERATIONS);
        assert_ne!(a.master_secret(), b.master_secret());
    }

    #[test]
    fn derive_master_different_iterations() {
        let a = derive_master(b"same-passphrase", &test_salt(), 1000);
        let b = derive_master(b"same-passphrase", &test_salt(), 2000);
        assert_ne!(a.master_secret(), b.master_secret());
    }

    #[test]
    fn slot_keys_are_distinct() {
        let master = [0xABu8; 32];
        let keys = derive_all_slot_keys(&master);
        assert_ne!(keys[0].bytes(), keys[1].bytes());
        assert_ne!(keys[0].bytes(), keys[2].bytes());
        assert_ne!(keys[1].bytes(), keys[2].bytes());
        assert_ne!(keys[0].bytes(), keys[3].bytes());
    }

    #[test]
    fn slot_keys_are_deterministic() {
        let master = [0xABu8; 32];
        let a = derive_all_slot_keys(&master);
        let b = derive_all_slot_keys(&master);
        for i in 0..4 {
            assert_eq!(a[i].bytes(), b[i].bytes());
        }
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
