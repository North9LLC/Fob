/// SSH public key parsing helpers — algorithm detection and the standard
/// OpenSSH `SHA256:...` fingerprint. No signing here: importing a key you
/// already have doesn't need it, and the SSH agent protocol (which does)
/// pulls in real signing crates separately.
use base64::{engine::general_purpose::STANDARD, engine::general_purpose::STANDARD_NO_PAD, Engine};
use sha2::{Digest, Sha256};

use crate::{
    error::{Error, Result},
    types::SshAlgorithm,
};

/// Compute the OpenSSH `SHA256:...` fingerprint of a public key line
/// (`ssh-ed25519 AAAA... comment`), matching `ssh-keygen -lf`.
pub fn fingerprint(public_key_line: &str) -> Result<String> {
    let blob_b64 = public_key_line
        .split_whitespace()
        .nth(1)
        .ok_or_else(|| Error::InvalidArgument("malformed SSH public key".into()))?;

    let blob = STANDARD
        .decode(blob_b64)
        .map_err(|_| Error::InvalidArgument("invalid base64 in SSH public key".into()))?;

    let hash = Sha256::digest(&blob);
    Ok(format!("SHA256:{}", STANDARD_NO_PAD.encode(hash)))
}

/// Detect the key algorithm from a public key line's type prefix.
pub fn algorithm(public_key_line: &str) -> SshAlgorithm {
    match public_key_line.split_whitespace().next() {
        Some("ssh-rsa") => SshAlgorithm::Rsa,
        Some(t) if t.starts_with("ecdsa-sha2-") => SshAlgorithm::Ecdsa,
        _ => SshAlgorithm::Ed25519,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // A real ed25519 test public key (not a secret — throwaway test vector).
    const TEST_PUBKEY: &str = "ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAIBaLexhIIfz1MorwSoTHf07P8SEwaxjc9V2t8GLzuFgz test@example";

    #[test]
    fn fingerprint_matches_known_openssh_output() {
        // Cross-checked against `ssh-keygen -lf` for this exact key:
        // `256 SHA256:ZE2JLEe57KcPkMk5xzA0EwxIrjNLP1W6WvaL+N87Ggg test@example (ED25519)`
        assert_eq!(
            fingerprint(TEST_PUBKEY).unwrap(),
            "SHA256:ZE2JLEe57KcPkMk5xzA0EwxIrjNLP1W6WvaL+N87Ggg"
        );
    }

    #[test]
    fn fingerprint_rejects_malformed_key() {
        assert!(fingerprint("not-a-valid-key").is_err());
    }

    #[test]
    fn fingerprint_rejects_bad_base64() {
        assert!(fingerprint("ssh-ed25519 not-base64!!! comment").is_err());
    }

    #[test]
    fn algorithm_detects_ed25519() {
        assert_eq!(algorithm(TEST_PUBKEY), SshAlgorithm::Ed25519);
    }

    #[test]
    fn algorithm_detects_rsa() {
        assert_eq!(
            algorithm("ssh-rsa AAAAB3NzaC1yc2EAAAADAQABAAAB comment"),
            SshAlgorithm::Rsa
        );
    }

    #[test]
    fn algorithm_detects_ecdsa() {
        assert_eq!(
            algorithm("ecdsa-sha2-nistp256 AAAAE2VjZHNhLXNoYTItbmlzdHAyNTY comment"),
            SshAlgorithm::Ecdsa
        );
    }

    #[test]
    fn algorithm_defaults_to_ed25519_for_unknown() {
        assert_eq!(
            algorithm("something-weird AAAA comment"),
            SshAlgorithm::Ed25519
        );
    }
}
