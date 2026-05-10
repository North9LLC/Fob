use std::path::Path;

const PNG_IEND: &[u8] = &[0x00, 0x00, 0x00, 0x00, b'I', b'E', b'N', b'D', 0xAE, 0x42, 0x60, 0x82];
const FOB_MAGIC: &[u8] = b"FOBV1\0";

/// Embed `vault_bytes` into a PNG file by appending them after the IEND chunk.
///
/// The PNG remains a valid image. Tools that don't know about the appendix
/// ignore it; Fob reads it by scanning from the end.
pub fn write_vault_to_png(png_path: &Path, vault_bytes: &[u8]) -> anyhow::Result<()> {
    let mut png_data = std::fs::read(png_path)?;

    // Verify it ends with a valid IEND chunk.
    if !png_data.ends_with(PNG_IEND) {
        anyhow::bail!("file does not appear to be a valid PNG (missing IEND chunk)");
    }

    // Append: FOB_MAGIC || vault_length_u64_le || vault_bytes
    let vault_len = vault_bytes.len() as u64;
    png_data.extend_from_slice(FOB_MAGIC);
    png_data.extend_from_slice(&vault_len.to_le_bytes());
    png_data.extend_from_slice(vault_bytes);

    std::fs::write(png_path, &png_data)?;
    Ok(())
}

/// Extract vault bytes from a PNG with a Fob appendix.
pub fn read_vault_from_png(png_path: &Path) -> anyhow::Result<Vec<u8>> {
    let data = std::fs::read(png_path)?;
    extract_from_bytes(&data)
}

pub fn extract_from_bytes(data: &[u8]) -> anyhow::Result<Vec<u8>> {
    // Scan for FOB_MAGIC from the end (efficient for large images).
    let magic_len = FOB_MAGIC.len();
    let min_len = magic_len + 8; // magic + u64 length

    if data.len() < min_len {
        anyhow::bail!("no Fob appendix found");
    }

    // Search for FOB_MAGIC.
    let _search_start = data.len().saturating_sub(data.len()); // search whole file
    let pos = data
        .windows(magic_len)
        .rposition(|w| w == FOB_MAGIC)
        .ok_or_else(|| anyhow::anyhow!("no Fob appendix found in file"))?;

    let after_magic = pos + magic_len;
    if after_magic + 8 > data.len() {
        anyhow::bail!("truncated Fob appendix");
    }

    let vault_len = u64::from_le_bytes(
        data[after_magic..after_magic + 8].try_into().unwrap(),
    ) as usize;

    let vault_start = after_magic + 8;
    if vault_start + vault_len > data.len() {
        anyhow::bail!("Fob appendix length exceeds file size");
    }

    Ok(data[vault_start..vault_start + vault_len].to_vec())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fake_png() -> Vec<u8> {
        let mut png = Vec::new();
        // Minimal 1×1 PNG header bytes.
        png.extend_from_slice(&[137, 80, 78, 71, 13, 10, 26, 10]); // PNG sig
        // Minimal IHDR (simplified, not valid dimensions but structurally present).
        png.extend_from_slice(&[0, 0, 0, 13]); // chunk length
        png.extend_from_slice(b"IHDR");
        png.extend_from_slice(&[0u8; 17]); // data + crc
        // IEND chunk.
        png.extend_from_slice(PNG_IEND);
        png
    }

    #[test]
    fn appendix_roundtrip() {
        let mut data = fake_png();
        let vault_bytes = b"test vault payload";

        // Simulate write.
        data.extend_from_slice(FOB_MAGIC);
        data.extend_from_slice(&(vault_bytes.len() as u64).to_le_bytes());
        data.extend_from_slice(vault_bytes);

        let extracted = extract_from_bytes(&data).unwrap();
        assert_eq!(extracted, vault_bytes);
    }

    #[test]
    fn no_appendix_returns_error() {
        let data = fake_png();
        assert!(extract_from_bytes(&data).is_err());
    }

    #[test]
    fn truncated_appendix_returns_error() {
        let mut data = fake_png();
        data.extend_from_slice(FOB_MAGIC);
        // Only 4 bytes of the 8-byte length — truncated.
        data.extend_from_slice(&[0, 0, 0, 0]);
        assert!(extract_from_bytes(&data).is_err());
    }
}
