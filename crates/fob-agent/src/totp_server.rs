/// TOTP IPC server stub.
///
/// Provides TOTP codes over a local IPC channel (Unix socket or named pipe)
/// to the `fob totp` subcommand without requiring the vault to be re-unlocked.
use fob_core::types::TotpEntry;

pub struct TotpServer {
    entries: Vec<TotpEntry>,
}

impl TotpServer {
    pub fn new(entries: Vec<TotpEntry>) -> Self {
        Self { entries }
    }

    /// Get the current TOTP code for `issuer`.
    pub fn get_code(&self, issuer: &str) -> Option<String> {
        self.entries
            .iter()
            .find(|e| e.issuer.eq_ignore_ascii_case(issuer))
            .and_then(|e| fob_core::totp::generate_now(e).ok())
    }
}
