/// Browser-compatible vault format: PBKDF2-SHA256 + AES-256-GCM.
///
/// Designed so the browser can read and write this file using only
/// native WebCrypto APIs (no server, no WASM required).
///
/// Binary layout:
///   [0..4]   magic "SIGL"
///   [4..8]   version: u32 LE (1)
///   [8..12]  kdf_iterations: u32 LE
///   [12..44] salt: 32 random bytes (PBKDF2 salt)
///   [44..56] iv: 12 random bytes (AES-256-GCM nonce)
///   [56..]   ciphertext: AES-256-GCM encrypted JSON + 16-byte auth tag
///
/// The JSON payload is a VaultJson struct (serde).
use aes_gcm::{
    aead::{Aead, KeyInit},
    Aes256Gcm, Nonce,
};
use pbkdf2::pbkdf2_hmac;
use sha2::Sha256;
use zeroize::Zeroize;

use crate::error::{Error, Result};

pub const MAGIC: &[u8; 4] = b"SIGL";
pub const VERSION: u32 = 1;
pub const KDF_ITERATIONS: u32 = 310_000;

const MAGIC_LEN: usize = 4;
const VERSION_LEN: usize = 4;
const ITER_LEN: usize = 4;
const SALT_LEN: usize = 32;
const IV_LEN: usize = 12;
const HEADER_LEN: usize = MAGIC_LEN + VERSION_LEN + ITER_LEN + SALT_LEN + IV_LEN; // 56

/// Create an encrypted vault file from a passphrase and JSON payload.
///
/// Returns the raw bytes to write to `vault.sigil` on the USB.
pub fn create(passphrase: &[u8], json_payload: &str) -> Result<Vec<u8>> {
    let mut salt = [0u8; SALT_LEN];
    let mut iv = [0u8; IV_LEN];
    getrandom::getrandom(&mut salt).map_err(|_| Error::Encrypt)?;
    getrandom::getrandom(&mut iv).map_err(|_| Error::Encrypt)?;

    let ciphertext = encrypt(passphrase, &salt, &iv, KDF_ITERATIONS, json_payload.as_bytes())?;

    let mut out = Vec::with_capacity(HEADER_LEN + ciphertext.len());
    out.extend_from_slice(MAGIC);
    out.extend_from_slice(&VERSION.to_le_bytes());
    out.extend_from_slice(&KDF_ITERATIONS.to_le_bytes());
    out.extend_from_slice(&salt);
    out.extend_from_slice(&iv);
    out.extend_from_slice(&ciphertext);
    Ok(out)
}

/// Decrypt and return the JSON payload from a vault file.
///
/// Returns `Err(Error::Decrypt)` if the passphrase is wrong.
pub fn open(passphrase: &[u8], data: &[u8]) -> Result<String> {
    if data.len() < HEADER_LEN + 16 {
        return Err(Error::Format("vault file too small".into()));
    }
    if &data[0..4] != MAGIC {
        return Err(Error::Format("not a Sigil vault".into()));
    }

    let iterations = u32::from_le_bytes(data[8..12].try_into().unwrap());
    let salt = &data[12..44];
    let iv   = &data[44..56];
    let ct   = &data[56..];

    let plaintext = decrypt(passphrase, salt, iv, iterations, ct)?;
    String::from_utf8(plaintext).map_err(|_| Error::Format("vault JSON is not valid UTF-8".into()))
}

/// Re-encrypt an already-open vault with the same passphrase (to save changes).
///
/// Generates a fresh IV each time — essential for AES-GCM security.
pub fn save(passphrase: &[u8], data: &[u8], new_json: &str) -> Result<Vec<u8>> {
    if data.len() < HEADER_LEN {
        return Err(Error::Format("vault file too small".into()));
    }
    let iterations = u32::from_le_bytes(data[8..12].try_into().unwrap());
    let salt = &data[12..44];

    // Fresh IV every save — AES-GCM nonce reuse would be catastrophic.
    let mut iv = [0u8; IV_LEN];
    getrandom::getrandom(&mut iv).map_err(|_| Error::Encrypt)?;

    let ciphertext = encrypt(passphrase, salt, &iv, iterations, new_json.as_bytes())?;

    let mut out = Vec::with_capacity(HEADER_LEN + ciphertext.len());
    out.extend_from_slice(&data[0..8]);         // magic + version
    out.extend_from_slice(&data[8..12]);         // iterations (unchanged)
    out.extend_from_slice(salt);                  // salt (unchanged)
    out.extend_from_slice(&iv);                   // fresh IV
    out.extend_from_slice(&ciphertext);
    Ok(out)
}

// ---------------------------------------------------------------------------
// Internal helpers
// ---------------------------------------------------------------------------

fn derive_key(passphrase: &[u8], salt: &[u8], iterations: u32) -> [u8; 32] {
    let mut key = [0u8; 32];
    pbkdf2_hmac::<Sha256>(passphrase, salt, iterations, &mut key);
    key
}

fn encrypt(passphrase: &[u8], salt: &[u8], iv: &[u8; IV_LEN], iterations: u32, plaintext: &[u8]) -> Result<Vec<u8>> {
    let mut key = derive_key(passphrase, salt, iterations);
    let cipher = Aes256Gcm::new_from_slice(&key).map_err(|_| Error::Encrypt)?;
    let nonce = Nonce::from_slice(iv);
    let ct = cipher.encrypt(nonce, plaintext).map_err(|_| Error::Encrypt)?;
    key.zeroize();
    Ok(ct)
}

fn decrypt(passphrase: &[u8], salt: &[u8], iv: &[u8], iterations: u32, ciphertext: &[u8]) -> Result<Vec<u8>> {
    let mut key = derive_key(passphrase, salt, iterations);
    let cipher = Aes256Gcm::new_from_slice(&key).map_err(|_| Error::Decrypt)?;
    let nonce = Nonce::from_slice(iv);
    let pt = cipher.decrypt(nonce, ciphertext).map_err(|_| Error::Decrypt)?;
    key.zeroize();
    Ok(pt)
}
