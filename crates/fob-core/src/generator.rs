use crate::error::{Error, Result};

/// Charset for `generate_password` — kept in sync with `web/index.html`'s
/// `generatePassword()` so both interfaces produce passwords of equal
/// quality from the same pool of characters.
const CHARSET: &[u8] =
    b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789!@#$%^&*-_=+";

/// Generate a random password of `length` characters from a fixed charset.
///
/// Uses rejection sampling against the charset size to avoid modulo bias:
/// naively mapping a random byte via `byte % CHARSET.len()` would make the
/// first `256 % CHARSET.len()` characters of the charset slightly more
/// likely than the rest, since 256 isn't an exact multiple of the charset
/// size.
pub fn generate_password(length: usize) -> Result<String> {
    let charset_len = CHARSET.len();
    let max = 256 - (256 % charset_len);
    let mut out = String::with_capacity(length);
    let mut byte = [0u8; 1];
    while out.len() < length {
        getrandom::getrandom(&mut byte).map_err(|_| Error::Encrypt)?;
        let b = byte[0] as usize;
        if b < max {
            out.push(CHARSET[b % charset_len] as char);
        }
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generates_requested_length() {
        let pw = generate_password(20).unwrap();
        assert_eq!(pw.chars().count(), 20);
    }

    #[test]
    fn only_uses_charset_characters() {
        let pw = generate_password(200).unwrap();
        assert!(pw.chars().all(|c| CHARSET.contains(&(c as u8))));
    }

    #[test]
    fn is_not_deterministic() {
        let a = generate_password(32).unwrap();
        let b = generate_password(32).unwrap();
        assert_ne!(a, b, "two generated passwords collided — RNG is broken");
    }

    #[test]
    fn zero_length_returns_empty_string() {
        assert_eq!(generate_password(0).unwrap(), "");
    }
}
