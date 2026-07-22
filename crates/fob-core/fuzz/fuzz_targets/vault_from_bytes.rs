#![no_main]

use fob_core::vault::VaultFile;
use libfuzzer_sys::fuzz_target;

// Fuzzes VaultFile::from_bytes, which validates overall vault file size and
// then delegates to VaultHeader::parse. Called on the full contents of
// whatever file lives at the expected path on a mounted USB drive — arbitrary
// length, arbitrary bytes.
fuzz_target!(|data: &[u8]| {
    let _ = VaultFile::from_bytes(data.to_vec());
});
