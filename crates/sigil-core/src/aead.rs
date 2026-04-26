use chacha20poly1305::{
    aead::{AeadInPlace, KeyInit},
    XChaCha20Poly1305, XNonce,
};

use crate::error::{Error, Result};

pub const XCHACHA_NONCE_LEN: usize = 24;
pub const XCHACHA_TAG_LEN: usize = 16;

/// Encrypt plaintext with XChaCha20-Poly1305.
///
/// Returns `nonce || ciphertext || tag` as a single Vec.
/// The `aad` bytes are authenticated but not encrypted — use them to bind
/// the ciphertext to its context (e.g., vault header bytes).
pub fn encrypt(key: &[u8; 32], plaintext: &[u8], aad: &[u8]) -> Result<Vec<u8>> {
    let cipher = XChaCha20Poly1305::new(key.into());

    let mut nonce_bytes = [0u8; XCHACHA_NONCE_LEN];
    getrandom::getrandom(&mut nonce_bytes).map_err(|_| Error::Encrypt)?;
    let nonce = XNonce::from(nonce_bytes);

    let mut buffer = plaintext.to_vec();
    cipher
        .encrypt_in_place(&nonce, aad, &mut buffer)
        .map_err(|_| Error::Encrypt)?;

    let mut output = Vec::with_capacity(XCHACHA_NONCE_LEN + buffer.len());
    output.extend_from_slice(&nonce_bytes);
    output.extend_from_slice(&buffer);

    Ok(output)
}

/// Decrypt a blob produced by `encrypt`.
///
/// Input must be `nonce || ciphertext || tag`. The `aad` must match exactly
/// what was passed to `encrypt` or authentication will fail.
pub fn decrypt(key: &[u8; 32], data: &[u8], aad: &[u8]) -> Result<Vec<u8>> {
    if data.len() < XCHACHA_NONCE_LEN + XCHACHA_TAG_LEN {
        return Err(Error::Decrypt);
    }

    let (nonce_bytes, ciphertext) = data.split_at(XCHACHA_NONCE_LEN);
    let nonce = XNonce::from_slice(nonce_bytes);
    let cipher = XChaCha20Poly1305::new(key.into());

    let mut buffer = ciphertext.to_vec();
    cipher
        .decrypt_in_place(nonce, aad, &mut buffer)
        .map_err(|_| Error::Decrypt)?;

    Ok(buffer)
}

/// Encrypt with a fixed nonce — useful when the caller controls nonce generation
/// (e.g., slot-level encryption where the nonce is stored in the header).
pub fn encrypt_with_nonce(
    key: &[u8; 32],
    nonce: &[u8; XCHACHA_NONCE_LEN],
    plaintext: &[u8],
    aad: &[u8],
) -> Result<Vec<u8>> {
    let cipher = XChaCha20Poly1305::new(key.into());
    let nonce = XNonce::from(*nonce);

    let mut buffer = plaintext.to_vec();
    cipher
        .encrypt_in_place(&nonce, aad, &mut buffer)
        .map_err(|_| Error::Encrypt)?;

    Ok(buffer)
}

/// Decrypt with an explicit nonce (ciphertext does NOT include nonce prefix).
pub fn decrypt_with_nonce(
    key: &[u8; 32],
    nonce: &[u8; XCHACHA_NONCE_LEN],
    ciphertext: &[u8],
    aad: &[u8],
) -> Result<Vec<u8>> {
    if ciphertext.len() < XCHACHA_TAG_LEN {
        return Err(Error::Decrypt);
    }

    let nonce = XNonce::from(*nonce);
    let cipher = XChaCha20Poly1305::new(key.into());

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
        assert!(ct.len() >= XCHACHA_NONCE_LEN + XCHACHA_TAG_LEN);
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
        ct[XCHACHA_NONCE_LEN] ^= 0xFF;
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
        // Different nonces → different ciphertexts (collision probability 2^-192).
        assert_ne!(ct_a, ct_b);
    }

    #[test]
    fn fixed_nonce_roundtrip() {
        let key = test_key();
        let nonce = [0xAAu8; XCHACHA_NONCE_LEN];
        let plaintext = b"nonce-pinned secret";
        let aad = b"ctx";

        let ct = encrypt_with_nonce(&key, &nonce, plaintext, aad).unwrap();
        let pt = decrypt_with_nonce(&key, &nonce, &ct, aad).unwrap();
        assert_eq!(pt, plaintext);
    }

    #[test]
    fn decrypt_fails_on_too_short_input() {
        let key = test_key();
        let tiny = vec![0u8; XCHACHA_NONCE_LEN + 5]; // less than nonce + tag
        assert!(decrypt(&key, &tiny, b"").is_err());
    }
}
