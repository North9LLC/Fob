/// Steganographic cover I/O for vault concealment.
///
/// Three modes (Phase 7 of the implementation plan):
///
/// - PNG appendix: vault bytes appended after the PNG IEND chunk.
///   PNG viewers render the image normally; Fob reads the appendix.
///
/// - LSB embedding: vault bytes encoded in the least-significant bits
///   of a large cover image. Requires cover ≥ 8× the vault size.
///
/// - Filesystem dispersion: vault split across multiple plausible files
///   (PDFs, MP3s) with key-derived offsets.
pub mod png_appendix;

pub use png_appendix::{read_vault_from_png, write_vault_to_png};
