use thiserror::Error;

pub type Result<T> = std::result::Result<T, Error>;

#[derive(Debug, Error)]
pub enum Error {
    #[error("key derivation failed: {0}")]
    Kdf(String),

    #[error("encryption failed")]
    Encrypt,

    #[error("decryption failed — wrong passphrase or corrupted data")]
    Decrypt,

    #[error("vault format error: {0}")]
    Format(String),

    #[error("vault is full — no free entry slots")]
    VaultFull,

    #[error("entry not found: {0}")]
    NotFound(String),

    #[error("serialization error: {0}")]
    Serialize(String),

    #[error("memory allocation failed (mlock): {0}")]
    MemLock(String),

    #[error("invalid TOTP configuration: {0}")]
    InvalidTotp(String),

    #[error("invalid argument: {0}")]
    InvalidArgument(String),

    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
}

impl From<serde_json::Error> for Error {
    fn from(e: serde_json::Error) -> Self {
        Error::Serialize(e.to_string())
    }
}
