#![no_main]

use fob_core::format::VaultHeader;
use libfuzzer_sys::fuzz_target;

// Fuzzes VaultHeader::parse, which reads the first HEADER_SIZE bytes of a
// vault file (magic, version, kdf iterations, salt, per-slot nonces). This
// runs on bytes read straight off a physical USB drive before any crypto
// verification happens, so it must never panic regardless of input.
fuzz_target!(|data: &[u8]| {
    let _ = VaultHeader::parse(data);
});
