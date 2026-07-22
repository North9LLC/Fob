use aes_gcm::{
    aead::{AeadInOut, KeyInit},
    Aes256Gcm, Nonce,
};

use crate::error::{Error, Result};

pub const GCM_NONCE_LEN: usize = 12;
pub const GCM_TAG_LEN: usize = 16;

/// Encrypt plaintext with AES-256-GCM.
///
/// Returns `nonce || ciphertext || tag` as a single Vec.
/// The `aad` bytes are authenticated but not encrypted — use them to bind
/// the ciphertext to its context (e.g., vault header bytes).
pub fn encrypt(key: &[u8; 32], plaintext: &[u8], aad: &[u8]) -> Result<Vec<u8>> {
    let cipher = Aes256Gcm::new(key.into());

    let mut nonce_bytes = [0u8; GCM_NONCE_LEN];
    getrandom::getrandom(&mut nonce_bytes).map_err(|_| Error::Encrypt)?;
    let nonce = Nonce::from(nonce_bytes);

    let mut buffer = plaintext.to_vec();
    cipher
        .encrypt_in_place(&nonce, aad, &mut buffer)
        .map_err(|_| Error::Encrypt)?;

    let mut output = Vec::with_capacity(GCM_NONCE_LEN + buffer.len());
    output.extend_from_slice(&nonce_bytes);
    output.extend_from_slice(&buffer);

    Ok(output)
}

/// Decrypt a blob produced by `encrypt`.
///
/// Input must be `nonce || ciphertext || tag`. The `aad` must match exactly
/// what was passed to `encrypt` or authentication will fail.
pub fn decrypt(key: &[u8; 32], data: &[u8], aad: &[u8]) -> Result<Vec<u8>> {
    if data.len() < GCM_NONCE_LEN + GCM_TAG_LEN {
        return Err(Error::Decrypt);
    }

    let (nonce_bytes, ciphertext) = data.split_at(GCM_NONCE_LEN);
    let nonce = Nonce::try_from(nonce_bytes).map_err(|_| Error::Decrypt)?;
    let cipher = Aes256Gcm::new(key.into());

    let mut buffer = ciphertext.to_vec();
    cipher
        .decrypt_in_place(&nonce, aad, &mut buffer)
        .map_err(|_| Error::Decrypt)?;

    Ok(buffer)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_key() -> [u8; 32] {
        [0x5Au8; 32]
    }

    #[test]
    fn encrypt_decrypt_roundtrip() {
        let key = test_key();
        let plaintext = b"hello, world -- secret message";
        let aad = b"additional-data";

        let ciphertext = encrypt(&key, plaintext, aad).unwrap();
        let decrypted = decrypt(&key, &ciphertext, aad).unwrap();

        assert_eq!(decrypted, plaintext);
    }

    #[test]
    fn encrypt_produces_nonce_prefix() {
        let key = test_key();
        let ct = encrypt(&key, b"data", b"").unwrap();
        assert!(ct.len() >= GCM_NONCE_LEN + GCM_TAG_LEN);
    }

    #[test]
    fn decrypt_fails_on_wrong_key() {
        let key_a = [0x01u8; 32];
        let key_b = [0x02u8; 32];
        let ct = encrypt(&key_a, b"secret", b"").unwrap();
        assert!(decrypt(&key_b, &ct, b"").is_err());
    }

    #[test]
    fn decrypt_fails_on_tampered_ciphertext() {
        let key = test_key();
        let mut ct = encrypt(&key, b"secret", b"").unwrap();
        // Flip a bit in the ciphertext body (after the nonce).
        ct[GCM_NONCE_LEN] ^= 0xFF;
        assert!(decrypt(&key, &ct, b"").is_err());
    }

    #[test]
    fn decrypt_fails_on_wrong_aad() {
        let key = test_key();
        let ct = encrypt(&key, b"secret", b"correct-aad").unwrap();
        assert!(decrypt(&key, &ct, b"wrong-aad").is_err());
    }

    #[test]
    fn encrypt_is_nondeterministic() {
        let key = test_key();
        let ct_a = encrypt(&key, b"same plaintext", b"").unwrap();
        let ct_b = encrypt(&key, b"same plaintext", b"").unwrap();
        // Different nonces → different ciphertexts.
        assert_ne!(ct_a, ct_b);
    }

    #[test]
    fn decrypt_fails_on_too_short_input() {
        let key = test_key();
        let tiny = vec![0u8; GCM_NONCE_LEN + 5]; // less than nonce + tag
        assert!(decrypt(&key, &tiny, b"").is_err());
    }
}
