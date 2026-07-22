/// On-disk vault file format constants and header layout.
///
/// The vault file is a fixed-size blob split into `NUM_SLOTS` equal-size
/// cells. Each cell is either:
/// - Populated: AES-256-GCM(nonce || len_u64_le || JSON blob || random_padding || tag)
/// - Unpopulated: random bytes
///
/// Layout:
/// ```text
/// Offset  Length  Field
/// ------  ------  -----
/// 0       4       magic "FOB2"
/// 4       4       format_version — u32 LE
/// 8       4       kdf_iterations — u32 LE (PBKDF2-HMAC-SHA256 iteration count)
/// 12      32      salt — PBKDF2 salt, shared across all slot derivations
/// 44      56      reserved — zeroed
/// 100     ...     Encrypted slot region
/// ```
///
/// The header intentionally carries a magic + version + iteration count in
/// the clear (unlike a random-looking blob) so both the CLI and the
/// zero-dependency browser vault can parse it without guessing. The vault's
/// deniability guarantees (decoy/duress) protect *which passphrase unlocks
/// which content*, not whether the file is a Fob vault at all — the
/// filename `vault.fob` already reveals that.
///
/// Each slot cell carries its own fresh, randomly-generated AES-GCM nonce as
/// the first 12 bytes of the cell (see `aead::encrypt`/`decrypt`), generated
/// anew on every write. Format v2 stored one nonce per slot *in the header*,
/// reused unchanged across every subsequent save of that slot for the
/// vault's entire lifetime — an unconditional AES-GCM nonce reuse, which
/// breaks confidentiality (XORing two on-disk snapshots of the same slot
/// leaks the plaintext XOR) and, via the GCM "forbidden attack", lets an
/// attacker holding two same-key/nonce ciphertexts forge undetectable
/// replacement content. v3 fixes this structurally: nonces never repeat
/// because they're never reused, and the header holds no nonce at all.
///
/// Cell size = (FILE_SIZE - HEADER_SIZE) / NUM_SLOTS
use crate::error::Error;

pub const MAGIC: &[u8; 4] = b"FOB2";
pub const MAGIC_OFFSET: usize = 0;
pub const MAGIC_LEN: usize = 4;

pub const HEADER_VERSION_OFFSET: usize = 4;
pub const HEADER_VERSION_LEN: usize = 4;

pub const HEADER_ITERATIONS_OFFSET: usize = 8;
pub const HEADER_ITERATIONS_LEN: usize = 4;

pub const HEADER_SALT_OFFSET: usize = 12;
pub const HEADER_SALT_LEN: usize = 32;

pub const NUM_SLOTS: usize = 4;

pub const HEADER_RESERVED_OFFSET: usize = 44;
pub const HEADER_RESERVED_LEN: usize = 56;

pub const HEADER_SIZE: usize = 100;

/// Default vault file size: 16 MiB.
pub const DEFAULT_VAULT_SIZE: usize = 16 * 1024 * 1024;

/// Maximum supported vault file size: 1 GiB.
pub const MAX_VAULT_SIZE: usize = 1024 * 1024 * 1024;

/// Minimum vault file size — enough header + a floor per-slot cell size
/// (64 bytes) for all `NUM_SLOTS` slots. Below this, a cell wouldn't even
/// have room for the nonce + length-prefix + GCM tag, so the file can't be
/// a genuine Fob vault. Enforced both when creating a fresh vault
/// (`VaultFile::create_fresh`) and when parsing an existing one
/// (`VaultFile::from_bytes`) — the latter is the one that reads untrusted
/// bytes off a USB drive, where a truncated/corrupted file would otherwise
/// parse "successfully" into a file with degenerate (even zero-byte) slot
/// cells, later surfacing only as an opaque "wrong passphrase or corrupted
/// data" on every unlock attempt instead of a clear diagnostic here.
pub const MIN_VAULT_SIZE: usize = HEADER_SIZE + NUM_SLOTS * 64;

/// Current format version.
///
/// Bumped 2 -> 3 to fix a critical AES-GCM nonce-reuse vulnerability: v2
/// stored a fixed nonce per slot in the header, reused for every save. v3
/// stores a fresh random nonce inline in each cell on every write instead.
/// v2 vaults are rejected outright (see `VaultHeader::parse`) rather than
/// silently misread, since the on-disk cell layout is incompatible.
pub const FORMAT_VERSION: u32 = 3;

/// Default PBKDF2-HMAC-SHA256 iteration count.
///
/// Chosen to match what the browser vault (WebCrypto, no native Argon2id)
/// can run interactively — this value is also the only KDF used by the
/// native CLI, so a vault created on one is always openable by the other.
pub const DEFAULT_KDF_ITERATIONS: u32 = 310_000;

/// Highest PBKDF2 iteration count accepted from an on-disk header.
///
/// `kdf_iterations` lives in the cleartext, unauthenticated part of the
/// header — it's read and used before any slot's AEAD tag is checked, so a
/// corrupted or maliciously-modified vault file could otherwise force an
/// arbitrarily expensive PBKDF2 pass (e.g. `u32::MAX`, ~4.29 billion
/// iterations) on every unlock attempt before authentication ever runs, a
/// denial-of-service reachable without knowing any passphrase. This cap is
/// deliberately generous (normal use is ~310k) but still bounded.
pub const MAX_KDF_ITERATIONS: u32 = 10_000_000;

/// Parsed vault file header.
#[derive(Debug, Clone)]
pub struct VaultHeader {
    pub format_version: u32,
    pub kdf_iterations: u32,
    pub salt: [u8; HEADER_SALT_LEN],
}

impl VaultHeader {
    /// Parse a header from the first `HEADER_SIZE` bytes of the vault file.
    pub fn parse(data: &[u8]) -> Result<Self, Error> {
        if data.len() < HEADER_SIZE {
            return Err(Error::Format(format!(
                "vault too small: {} < {}",
                data.len(),
                HEADER_SIZE
            )));
        }

        if &data[MAGIC_OFFSET..MAGIC_OFFSET + MAGIC_LEN] != MAGIC {
            return Err(Error::Format("not a Fob vault".into()));
        }

        let format_version = u32::from_le_bytes(
            data[HEADER_VERSION_OFFSET..HEADER_VERSION_OFFSET + HEADER_VERSION_LEN]
                .try_into()
                .unwrap(),
        );
        if format_version != FORMAT_VERSION {
            return Err(Error::Format(format!(
                "unsupported vault format version {format_version} (expected {FORMAT_VERSION})"
            )));
        }

        let kdf_iterations = u32::from_le_bytes(
            data[HEADER_ITERATIONS_OFFSET..HEADER_ITERATIONS_OFFSET + HEADER_ITERATIONS_LEN]
                .try_into()
                .unwrap(),
        );
        if kdf_iterations > MAX_KDF_ITERATIONS {
            return Err(Error::Format(format!(
                "kdf_iterations {kdf_iterations} exceeds maximum of {MAX_KDF_ITERATIONS}"
            )));
        }

        let salt: [u8; HEADER_SALT_LEN] = data
            [HEADER_SALT_OFFSET..HEADER_SALT_OFFSET + HEADER_SALT_LEN]
            .try_into()
            .unwrap();

        Ok(Self {
            format_version,
            kdf_iterations,
            salt,
        })
    }

    /// Serialize the header to `HEADER_SIZE` bytes.
    pub fn to_bytes(&self) -> [u8; HEADER_SIZE] {
        let mut out = [0u8; HEADER_SIZE];
        out[MAGIC_OFFSET..MAGIC_OFFSET + MAGIC_LEN].copy_from_slice(MAGIC);
        out[HEADER_VERSION_OFFSET..HEADER_VERSION_OFFSET + HEADER_VERSION_LEN]
            .copy_from_slice(&self.format_version.to_le_bytes());
        out[HEADER_ITERATIONS_OFFSET..HEADER_ITERATIONS_OFFSET + HEADER_ITERATIONS_LEN]
            .copy_from_slice(&self.kdf_iterations.to_le_bytes());
        out[HEADER_SALT_OFFSET..HEADER_SALT_OFFSET + HEADER_SALT_LEN].copy_from_slice(&self.salt);
        out
    }
}

/// Size of each slot cell given a vault file size.
pub fn cell_size(vault_size: usize) -> usize {
    (vault_size - HEADER_SIZE) / NUM_SLOTS
}

/// Byte offset of cell `i` within the vault file.
pub fn cell_offset(vault_size: usize, slot: usize) -> usize {
    HEADER_SIZE + slot * cell_size(vault_size)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_header() -> VaultHeader {
        VaultHeader {
            format_version: FORMAT_VERSION,
            kdf_iterations: DEFAULT_KDF_ITERATIONS,
            salt: [0x11u8; 32],
        }
    }

    #[test]
    fn header_roundtrip() {
        let header = test_header();
        let bytes = header.to_bytes();
        let parsed = VaultHeader::parse(&bytes).unwrap();
        assert_eq!(parsed.salt, header.salt);
        assert_eq!(parsed.format_version, FORMAT_VERSION);
        assert_eq!(parsed.kdf_iterations, DEFAULT_KDF_ITERATIONS);
    }

    #[test]
    fn header_too_small_fails() {
        let tiny = vec![0u8; 10];
        assert!(VaultHeader::parse(&tiny).is_err());
    }

    #[test]
    fn header_wrong_magic_fails() {
        let mut bytes = test_header().to_bytes();
        bytes[0] = b'X';
        assert!(VaultHeader::parse(&bytes).is_err());
    }

    #[test]
    fn header_wrong_version_rejected() {
        let mut bytes = test_header().to_bytes();
        bytes[HEADER_VERSION_OFFSET..HEADER_VERSION_OFFSET + HEADER_VERSION_LEN]
            .copy_from_slice(&2u32.to_le_bytes());
        let err = VaultHeader::parse(&bytes).unwrap_err();
        assert!(format!("{err}").contains("unsupported vault format version"));
    }

    #[test]
    fn header_excessive_kdf_iterations_rejected() {
        let mut bytes = test_header().to_bytes();
        bytes[HEADER_ITERATIONS_OFFSET..HEADER_ITERATIONS_OFFSET + HEADER_ITERATIONS_LEN]
            .copy_from_slice(&u32::MAX.to_le_bytes());
        let err = VaultHeader::parse(&bytes).unwrap_err();
        assert!(format!("{err}").contains("exceeds maximum"));
    }

    #[test]
    fn cell_sizes_are_equal() {
        let vault_size = DEFAULT_VAULT_SIZE;
        let c0 = cell_size(vault_size);
        for i in 0..NUM_SLOTS {
            assert_eq!(
                cell_offset(vault_size, i + 1) - cell_offset(vault_size, i),
                c0
            );
        }
    }

    #[test]
    fn cell_offset_does_not_exceed_file() {
        let vault_size = DEFAULT_VAULT_SIZE;
        let last_cell_end = cell_offset(vault_size, NUM_SLOTS - 1) + cell_size(vault_size);
        assert_eq!(last_cell_end, vault_size);
    }
}
