#![no_main]

use fob_core::vault::VaultBlob;
use libfuzzer_sys::fuzz_target;

// Fuzzes VaultBlob::from_json, which deserializes the decrypted plaintext of
// a vault slot as JSON. Reached only after AES-GCM authentication succeeds,
// but a corrupted/adversarial vault could still produce authenticated bytes
// that decode as some other decryptable slot's ciphertext (e.g. via nonce
// reuse or a crafted file), so this parser still processes attacker-influenced
// bytes and must not panic.
fuzz_target!(|data: &[u8]| {
    let _ = VaultBlob::from_json(data);
});
