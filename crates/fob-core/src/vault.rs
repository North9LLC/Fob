use serde::{Deserialize, Serialize};
use std::time::{SystemTime, UNIX_EPOCH};
use zeroize::Zeroize;

use crate::{
    aead,
    error::{Error, Result},
    format::{
        cell_offset, cell_size, VaultHeader, FORMAT_VERSION, HEADER_SIZE,
        NUM_SLOTS,
    },
    kdf,
    types::{
        FileEntry, NoteEntry, PasswordEntry, SshKeyEntry, TotpEntry,
    },
};

/// Returns current Unix timestamp in seconds.
pub fn unix_now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

/// Which vault slot an opened vault lives in.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SlotKind {
    Main = 0,
    Decoy = 1,
    Duress = 2,
    Reserved = 3,
}

impl SlotKind {
    pub fn from_index(i: usize) -> Option<Self> {
        match i {
            0 => Some(Self::Main),
            1 => Some(Self::Decoy),
            2 => Some(Self::Duress),
            3 => Some(Self::Reserved),
            _ => None,
        }
    }

    pub fn index(self) -> usize {
        self as usize
    }
}

/// The data payload stored inside a single encrypted vault slot.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct VaultBlob {
    pub version: u64,
    pub created: u64,
    pub last_modified: u64,
    pub passwords: Vec<PasswordEntry>,
    pub totps: Vec<TotpEntry>,
    pub ssh_keys: Vec<SshKeyEntry>,
    pub files: Vec<FileEntry>,
    pub notes: Vec<NoteEntry>,
}

impl VaultBlob {
    pub fn new() -> Self {
        let now = unix_now();
        Self {
            version: FORMAT_VERSION,
            created: now,
            last_modified: now,
            passwords: Vec::new(),
            totps: Vec::new(),
            ssh_keys: Vec::new(),
            files: Vec::new(),
            notes: Vec::new(),
        }
    }

    pub fn touch(&mut self) {
        self.last_modified = unix_now();
    }

    /// Total number of entries across all categories.
    pub fn entry_count(&self) -> usize {
        self.passwords.len()
            + self.totps.len()
            + self.ssh_keys.len()
            + self.files.len()
            + self.notes.len()
    }

    /// Serialize to CBOR bytes.
    pub fn to_cbor(&self) -> Result<Vec<u8>> {
        serde_cbor::to_vec(self).map_err(Error::from)
    }

    /// Deserialize from CBOR bytes.
    pub fn from_cbor(data: &[u8]) -> Result<Self> {
        serde_cbor::from_slice(data).map_err(Error::from)
    }
}


/// A vault file in memory: the raw bytes and parsed header.
pub struct VaultFile {
    pub data: Vec<u8>,
    pub header: VaultHeader,
}

impl VaultFile {
    /// Create a fresh vault file filled with random bytes.
    ///
    /// The header is populated with new random salt and nonces.
    /// All slot cells are random and indistinguishable from ciphertext.
    pub fn create_fresh(vault_size: usize) -> Result<Self> {
        if vault_size < HEADER_SIZE + NUM_SLOTS * 64 {
            return Err(Error::Format("vault size too small".into()));
        }

        let mut data = vec![0u8; vault_size];
        getrandom::getrandom(&mut data).map_err(|_| Error::Encrypt)?;

        let salt: [u8; 32] = data[0..32].try_into().unwrap();
        let mut nonces = [[0u8; 24]; NUM_SLOTS];
        for (i, nonce) in nonces.iter_mut().enumerate() {
            let off = 32 + i * 24;
            nonce.copy_from_slice(&data[off..off + 24]);
        }

        let header = VaultHeader {
            salt,
            nonces,
            format_version: 0,
        };
        let header_bytes = header.to_bytes();
        data[..HEADER_SIZE].copy_from_slice(&header_bytes);

        Ok(Self { data, header })
    }

    /// Parse a vault file from existing bytes (e.g., read from USB).
    pub fn from_bytes(data: Vec<u8>) -> Result<Self> {
        let header = VaultHeader::parse(&data)?;
        Ok(Self { data, header })
    }

    /// Write a VaultBlob into a specific slot, encrypting it with `slot_key`.
    ///
    /// Plaintext layout inside the cell (all within the encrypted region):
    ///   [8 bytes]   CBOR length as u64 LE  (only readable with the correct key)
    ///   [N bytes]   CBOR-encoded VaultBlob
    ///   [remainder] random padding to fill cell_size - TAG_LEN bytes
    ///
    /// The ciphertext is always exactly cell_size bytes, making all cells
    /// indistinguishable in size. The length field is inside the ciphertext
    /// so it leaks nothing to an adversary without the key.
    pub fn write_slot(&mut self, slot: SlotKind, slot_key: &[u8; 32], blob: &VaultBlob) -> Result<()> {
        let cbor = blob.to_cbor()?;
        let cell_sz = cell_size(self.data.len());
        let plaintext_capacity = cell_sz.saturating_sub(aead::XCHACHA_TAG_LEN);
        const LENGTH_PREFIX: usize = 8;

        if cbor.len() + LENGTH_PREFIX > plaintext_capacity {
            return Err(Error::VaultFull);
        }

        // Build padded plaintext: len_u64_le || cbor || random_padding.
        let mut padded = vec![0u8; plaintext_capacity];
        getrandom::getrandom(&mut padded).map_err(|_| Error::Encrypt)?;
        padded[..LENGTH_PREFIX].copy_from_slice(&(cbor.len() as u64).to_le_bytes());
        padded[LENGTH_PREFIX..LENGTH_PREFIX + cbor.len()].copy_from_slice(&cbor);

        let nonce = &self.header.nonces[slot.index()];
        let aad = &self.header.to_bytes();
        let ciphertext = aead::encrypt_with_nonce(slot_key, nonce, &padded, aad)?;

        // ciphertext is now exactly cell_sz bytes.
        let offset = cell_offset(self.data.len(), slot.index());
        self.data[offset..offset + cell_sz].copy_from_slice(&ciphertext);

        Ok(())
    }

    /// Attempt to decrypt a specific slot with the given key.
    ///
    /// Returns Ok(blob) on success, Err(Error::Decrypt) if wrong key or corrupted.
    pub fn read_slot(&self, slot: SlotKind, slot_key: &[u8; 32]) -> Result<VaultBlob> {
        let cell_sz = cell_size(self.data.len());
        let offset = cell_offset(self.data.len(), slot.index());
        let nonce = &self.header.nonces[slot.index()];
        let aad = &self.header.to_bytes();
        let cell = &self.data[offset..offset + cell_sz];

        let plaintext = aead::decrypt_with_nonce(slot_key, nonce, cell, aad)?;

        const LENGTH_PREFIX: usize = 8;
        if plaintext.len() < LENGTH_PREFIX {
            return Err(Error::Format("decrypted cell too small".into()));
        }
        let cbor_len = u64::from_le_bytes(plaintext[..LENGTH_PREFIX].try_into().unwrap()) as usize;
        let cbor_end = LENGTH_PREFIX + cbor_len;
        if cbor_end > plaintext.len() {
            return Err(Error::Format("CBOR length field exceeds plaintext".into()));
        }

        VaultBlob::from_cbor(&plaintext[LENGTH_PREFIX..cbor_end])
    }
}

/// Parameters for creating a new vault.
pub struct VaultInitParams {
    pub main_passphrase: Vec<u8>,
    pub decoy_passphrase: Option<Vec<u8>>,
    pub duress_passphrase: Option<Vec<u8>>,
    pub vault_size: usize,
    pub decoy_blob: Option<VaultBlob>,
}

impl Drop for VaultInitParams {
    fn drop(&mut self) {
        self.main_passphrase.zeroize();
        if let Some(ref mut p) = self.decoy_passphrase {
            p.zeroize();
        }
        if let Some(ref mut p) = self.duress_passphrase {
            p.zeroize();
        }
    }
}

/// Create a new vault file with the given passphrases and main vault content.
///
/// Returns the full encrypted vault bytes ready to write to disk.
pub fn init_vault(params: VaultInitParams) -> Result<Vec<u8>> {
    validate_passphrases(
        &params.main_passphrase,
        params.decoy_passphrase.as_deref(),
        params.duress_passphrase.as_deref(),
    )?;

    let mut vault_file = VaultFile::create_fresh(params.vault_size)?;

    // Derive main slot key and write empty main vault.
    let main_kdf = kdf::derive_master(&params.main_passphrase, &vault_file.header.salt)?;
    let main_keys = kdf::derive_all_slot_keys(main_kdf.master_secret());
    let main_blob = VaultBlob::new();
    vault_file.write_slot(SlotKind::Main, &main_keys[0], &main_blob)?;

    // Write decoy slot if passphrase provided.
    if let Some(decoy_pass) = &params.decoy_passphrase {
        let decoy_kdf = kdf::derive_master(decoy_pass, &vault_file.header.salt)?;
        let decoy_keys = kdf::derive_all_slot_keys(decoy_kdf.master_secret());
        let decoy_blob = params.decoy_blob.clone().unwrap_or_default();
        vault_file.write_slot(SlotKind::Decoy, &decoy_keys[1], &decoy_blob)?;
    }

    // Write duress slot if passphrase provided.
    if let Some(duress_pass) = &params.duress_passphrase {
        let duress_kdf = kdf::derive_master(duress_pass, &vault_file.header.salt)?;
        let duress_keys = kdf::derive_all_slot_keys(duress_kdf.master_secret());
        // Duress blob is deliberately empty — opening it triggers wipe.
        let duress_blob = VaultBlob::new();
        vault_file.write_slot(SlotKind::Duress, &duress_keys[2], &duress_blob)?;
    }

    Ok(vault_file.data)
}

/// Attempt to unlock a vault with a passphrase.
///
/// Tries Main, Decoy, and Duress slots. Returns `(SlotKind, VaultBlob)` on
/// success. If the Duress slot matches, the vault file at `duress_wipe_path`
/// (if provided) is overwritten with random bytes before returning `Err(Decrypt)`,
/// making the wipe indistinguishable from a wrong passphrase.
pub fn unlock_vault(vault_bytes: &[u8], passphrase: &[u8]) -> Result<(SlotKind, VaultBlob)> {
    unlock_vault_inner(vault_bytes, passphrase, None)
}

/// Like `unlock_vault` but with duress wipe enabled: if the duress passphrase
/// is entered, the file at `vault_path` is cryptographically wiped.
pub fn unlock_vault_with_duress_wipe(
    vault_bytes: &[u8],
    passphrase: &[u8],
    vault_path: &std::path::Path,
) -> Result<(SlotKind, VaultBlob)> {
    unlock_vault_inner(vault_bytes, passphrase, Some(vault_path))
}

fn unlock_vault_inner(
    vault_bytes: &[u8],
    passphrase: &[u8],
    duress_wipe_path: Option<&std::path::Path>,
) -> Result<(SlotKind, VaultBlob)> {
    let vault_file = VaultFile::from_bytes(vault_bytes.to_vec())?;
    let kdf_out = kdf::derive_master(passphrase, &vault_file.header.salt)?;
    let slot_keys = kdf::derive_all_slot_keys(kdf_out.master_secret());

    // Always attempt all three slots so timing does not reveal which slot matched.
    let duress_ok = vault_file.read_slot(SlotKind::Duress, &slot_keys[2]).is_ok();
    let main_result = vault_file.read_slot(SlotKind::Main, &slot_keys[0]);
    let decoy_result = vault_file.read_slot(SlotKind::Decoy, &slot_keys[1]);

    if duress_ok {
        if let Some(path) = duress_wipe_path {
            let _ = wipe_file(path, vault_bytes.len());
        }
        return Err(Error::Decrypt);
    }

    if let Ok(blob) = main_result {
        return Ok((SlotKind::Main, blob));
    }
    if let Ok(blob) = decoy_result {
        return Ok((SlotKind::Decoy, blob));
    }

    Err(Error::Decrypt)
}

/// Overwrite a file completely with random bytes, then truncate.
/// Called on duress to destroy all vault data irreversibly.
fn wipe_file(path: &std::path::Path, size: usize) -> std::io::Result<()> {
    use std::io::Write;
    let mut f = std::fs::OpenOptions::new().write(true).open(path)?;
    let chunk_size = 65536;
    let mut written = 0;
    let mut chunk = vec![0u8; chunk_size.min(size)];
    while written < size {
        getrandom::getrandom(&mut chunk)
            .map_err(|e| std::io::Error::other(e.to_string()))?;
        let to_write = chunk.len().min(size - written);
        f.write_all(&chunk[..to_write])?;
        written += to_write;
    }
    f.flush()?;
    // Sync to disk — we want this to actually hit storage.
    f.sync_all()?;
    Ok(())
}

fn validate_passphrases(
    main: &[u8],
    decoy: Option<&[u8]>,
    duress: Option<&[u8]>,
) -> Result<()> {
    use subtle::ConstantTimeEq;
    if main.is_empty() {
        return Err(Error::InvalidArgument("main passphrase cannot be empty".into()));
    }
    if let Some(d) = decoy {
        if d.ct_eq(main).into() {
            return Err(Error::InvalidArgument(
                "decoy passphrase must differ from main".into(),
            ));
        }
    }
    if let Some(d) = duress {
        if d.ct_eq(main).into() {
            return Err(Error::InvalidArgument(
                "duress passphrase must differ from main".into(),
            ));
        }
    }
    if let (Some(dec), Some(dur)) = (decoy, duress) {
        if dec.ct_eq(dur).into() {
            return Err(Error::InvalidArgument(
                "decoy and duress passphrases must differ".into(),
            ));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    // Use a smaller vault for fast tests.
    const TEST_VAULT_SIZE: usize = 512 * 1024; // 512 KiB

    fn simple_vault(main_pass: &[u8]) -> Vec<u8> {
        init_vault(VaultInitParams {
            main_passphrase: main_pass.to_vec(),
            decoy_passphrase: None,
            duress_passphrase: None,
            vault_size: TEST_VAULT_SIZE,
            decoy_blob: None,
        })
        .unwrap()
    }

    #[test]
    fn init_and_unlock_main() {
        let vault = simple_vault(b"correct-horse-battery-staple");
        let (slot, blob) = unlock_vault(&vault, b"correct-horse-battery-staple").unwrap();
        assert_eq!(slot, SlotKind::Main);
        assert_eq!(blob.version, FORMAT_VERSION);
    }

    #[test]
    fn wrong_passphrase_fails() {
        let vault = simple_vault(b"my-secret");
        assert!(unlock_vault(&vault, b"wrong-password").is_err());
    }

    #[test]
    fn decoy_slot_opens_with_decoy_passphrase() {
        let main_pass = b"main-pass";
        let decoy_pass = b"decoy-pass";

        let mut decoy_blob = VaultBlob::new();
        decoy_blob.passwords.push(PasswordEntry::new(
            "Fake Bank",
            "fake-user",
            "fake-password-123",
        ));

        let vault = init_vault(VaultInitParams {
            main_passphrase: main_pass.to_vec(),
            decoy_passphrase: Some(decoy_pass.to_vec()),
            duress_passphrase: None,
            vault_size: TEST_VAULT_SIZE,
            decoy_blob: Some(decoy_blob),
        })
        .unwrap();

        let (slot, blob) = unlock_vault(&vault, main_pass).unwrap();
        assert_eq!(slot, SlotKind::Main);
        assert_eq!(blob.passwords.len(), 0);

        let (slot, blob) = unlock_vault(&vault, decoy_pass).unwrap();
        assert_eq!(slot, SlotKind::Decoy);
        assert_eq!(blob.passwords.len(), 1);
        assert_eq!(blob.passwords[0].name, "Fake Bank");
    }

    #[test]
    fn slot_isolation_different_keys() {
        let main_pass = b"main-passphrase";
        let decoy_pass = b"decoy-passphrase";

        let vault = init_vault(VaultInitParams {
            main_passphrase: main_pass.to_vec(),
            decoy_passphrase: Some(decoy_pass.to_vec()),
            duress_passphrase: None,
            vault_size: TEST_VAULT_SIZE,
            decoy_blob: None,
        })
        .unwrap();

        // Main key should not open decoy slot.
        let vault_file = VaultFile::from_bytes(vault.clone()).unwrap();
        let main_kdf = kdf::derive_master(main_pass, &vault_file.header.salt).unwrap();
        let main_keys = kdf::derive_all_slot_keys(main_kdf.master_secret());
        assert!(vault_file.read_slot(SlotKind::Decoy, &main_keys[1]).is_err());

        // Decoy key should not open main slot.
        let decoy_kdf = kdf::derive_master(decoy_pass, &vault_file.header.salt).unwrap();
        let decoy_keys = kdf::derive_all_slot_keys(decoy_kdf.master_secret());
        assert!(vault_file.read_slot(SlotKind::Main, &decoy_keys[0]).is_err());
    }

    #[test]
    fn vault_blob_roundtrip_cbor() {
        let mut blob = VaultBlob::new();
        blob.passwords.push(PasswordEntry::new("Test", "user", "pw"));
        blob.notes.push(NoteEntry::new("Note", "body text"));

        let cbor = blob.to_cbor().unwrap();
        let parsed = VaultBlob::from_cbor(&cbor).unwrap();

        assert_eq!(parsed.passwords.len(), 1);
        assert_eq!(parsed.passwords[0].name, "Test");
        assert_eq!(parsed.notes.len(), 1);
        assert_eq!(parsed.notes[0].title, "Note");
    }

    #[test]
    fn vault_file_is_correct_size() {
        let vault = simple_vault(b"pass");
        assert_eq!(vault.len(), TEST_VAULT_SIZE);
    }

    #[test]
    fn tampered_vault_fails_decryption() {
        let mut vault = simple_vault(b"secure-pass");
        // Flip bytes in the first slot cell.
        let offset = crate::format::cell_offset(TEST_VAULT_SIZE, 0);
        vault[offset] ^= 0xFF;
        vault[offset + 1] ^= 0xFF;
        assert!(unlock_vault(&vault, b"secure-pass").is_err());
    }

    #[test]
    fn duplicate_passphrases_rejected() {
        let result = init_vault(VaultInitParams {
            main_passphrase: b"same".to_vec(),
            decoy_passphrase: Some(b"same".to_vec()),
            duress_passphrase: None,
            vault_size: TEST_VAULT_SIZE,
            decoy_blob: None,
        });
        assert!(result.is_err());
    }

    #[test]
    fn entry_persistence_through_unlock() {
        let pass = b"my-vault-pass";
        let vault_size = TEST_VAULT_SIZE;

        // Init vault.
        let vault_bytes = init_vault(VaultInitParams {
            main_passphrase: pass.to_vec(),
            decoy_passphrase: None,
            duress_passphrase: None,
            vault_size,
            decoy_blob: None,
        })
        .unwrap();

        // Unlock, add an entry, re-lock (re-write).
        let (slot, mut blob) = unlock_vault(&vault_bytes, pass).unwrap();
        blob.passwords.push(PasswordEntry::new("GitHub", "alice", "gh-secret"));
        blob.touch();

        let mut vault_file = VaultFile::from_bytes(vault_bytes).unwrap();
        let kdf_out = kdf::derive_master(pass, &vault_file.header.salt).unwrap();
        let slot_keys = kdf::derive_all_slot_keys(kdf_out.master_secret());
        vault_file.write_slot(slot, &slot_keys[slot.index()], &blob).unwrap();

        // Unlock again and verify entry is present.
        let (_, loaded) = unlock_vault(&vault_file.data, pass).unwrap();
        assert_eq!(loaded.passwords.len(), 1);
        assert_eq!(loaded.passwords[0].name, "GitHub");
    }

    #[test]
    fn duress_passphrase_returns_generic_error() {
        let vault = init_vault(VaultInitParams {
            main_passphrase: b"main-pass".to_vec(),
            decoy_passphrase: None,
            duress_passphrase: Some(b"duress-pass".to_vec()),
            vault_size: TEST_VAULT_SIZE,
            decoy_blob: None,
        })
        .unwrap();

        // Main passphrase must still open normally.
        assert!(unlock_vault(&vault, b"main-pass").is_ok());

        // Duress passphrase must return the same error as a wrong passphrase.
        let result = unlock_vault(&vault, b"duress-pass");
        assert!(
            result.is_err(),
            "duress must not open a vault — got {:?}",
            result
        );
    }
}
