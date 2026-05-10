/// On-disk vault file format constants and header layout.
///
/// The vault file is a fixed-size blob of bytes that is indistinguishable
/// from random data in the default (no-magic) mode. No magic bytes appear
/// in the file unless `--magic` was passed at init time.
///
/// Layout:
/// ```text
/// Offset  Length  Field
/// ------  ------  -----
/// 0       32      slot_salt          — 32 random bytes, used for Argon2id salt
/// 32      24      slot_nonce[0]      — XChaCha nonce for the main slot
/// 56      24      slot_nonce[1]      — XChaCha nonce for the decoy slot
/// 80      24      slot_nonce[2]      — XChaCha nonce for the duress slot
/// 104     24      slot_nonce[3]      — XChaCha nonce for the reserved slot
/// 128     8       format_version     — u64 little-endian (0 for new vaults)
/// 136     8       reserved           — zeroed
/// 144     ...     Encrypted slot region
/// ```
///
/// The slot region is divided into 4 equal-size cells. Each cell is either:
/// - Populated: nonce || XChaCha20-Poly1305(vault_blob) || random_padding
/// - Unpopulated: random bytes
///
/// Cell size = (FILE_SIZE - HEADER_SIZE) / NUM_SLOTS
pub const HEADER_SALT_OFFSET: usize = 0;
pub const HEADER_SALT_LEN: usize = 32;

pub const HEADER_NONCE_OFFSET: usize = 32;
pub const NONCE_LEN: usize = 24;
pub const NUM_SLOTS: usize = 4;

pub const HEADER_VERSION_OFFSET: usize = 128;
pub const HEADER_VERSION_LEN: usize = 8;

pub const HEADER_RESERVED_OFFSET: usize = 136;
pub const HEADER_RESERVED_LEN: usize = 8;

pub const HEADER_SIZE: usize = 144;

/// Default vault file size: 16 MiB.
pub const DEFAULT_VAULT_SIZE: usize = 16 * 1024 * 1024;

/// Maximum supported vault file size: 1 GiB.
pub const MAX_VAULT_SIZE: usize = 1024 * 1024 * 1024;

/// Optional magic marker for tooling-friendly mode (not used by default).
pub const MAGIC: &[u8; 4] = b"FOB1";

/// Current format version.
pub const FORMAT_VERSION: u64 = 1;

/// Parsed vault file header.
#[derive(Debug, Clone)]
pub struct VaultHeader {
    pub salt: [u8; HEADER_SALT_LEN],
    pub nonces: [[u8; NONCE_LEN]; NUM_SLOTS],
    pub format_version: u64,
}

impl VaultHeader {
    /// Parse a header from the first `HEADER_SIZE` bytes of the vault file.
    pub fn parse(data: &[u8]) -> Result<Self, crate::error::Error> {
        if data.len() < HEADER_SIZE {
            return Err(crate::error::Error::Format(format!(
                "vault too small: {} < {}",
                data.len(),
                HEADER_SIZE
            )));
        }

        let salt: [u8; HEADER_SALT_LEN] = data[0..32].try_into().unwrap();

        let mut nonces = [[0u8; NONCE_LEN]; NUM_SLOTS];
        for (i, nonce) in nonces.iter_mut().enumerate() {
            let offset = HEADER_NONCE_OFFSET + i * NONCE_LEN;
            nonce.copy_from_slice(&data[offset..offset + NONCE_LEN]);
        }

        let version_bytes: [u8; 8] = data[HEADER_VERSION_OFFSET..HEADER_VERSION_OFFSET + 8]
            .try_into()
            .unwrap();
        let format_version = u64::from_le_bytes(version_bytes);

        Ok(Self {
            salt,
            nonces,
            format_version,
        })
    }

    /// Serialize the header to `HEADER_SIZE` bytes.
    pub fn to_bytes(&self) -> [u8; HEADER_SIZE] {
        let mut out = [0u8; HEADER_SIZE];
        out[0..32].copy_from_slice(&self.salt);
        for (i, nonce) in self.nonces.iter().enumerate() {
            let offset = HEADER_NONCE_OFFSET + i * NONCE_LEN;
            out[offset..offset + NONCE_LEN].copy_from_slice(nonce);
        }
        out[HEADER_VERSION_OFFSET..HEADER_VERSION_OFFSET + 8]
            .copy_from_slice(&self.format_version.to_le_bytes());
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

    #[test]
    fn header_roundtrip() {
        let header = VaultHeader {
            salt: [0x11u8; 32],
            nonces: [
                [0xAAu8; 24],
                [0xBBu8; 24],
                [0xCCu8; 24],
                [0xDDu8; 24],
            ],
            format_version: FORMAT_VERSION,
        };
        let bytes = header.to_bytes();
        let parsed = VaultHeader::parse(&bytes).unwrap();
        assert_eq!(parsed.salt, header.salt);
        assert_eq!(parsed.nonces, header.nonces);
        assert_eq!(parsed.format_version, FORMAT_VERSION);
    }

    #[test]
    fn header_too_small_fails() {
        let tiny = vec![0u8; 10];
        assert!(VaultHeader::parse(&tiny).is_err());
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
        let last_cell_end =
            cell_offset(vault_size, NUM_SLOTS - 1) + cell_size(vault_size);
        assert_eq!(last_cell_end, vault_size);
    }
}
