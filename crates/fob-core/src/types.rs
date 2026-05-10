use serde::{Deserialize, Serialize};
use std::fmt;
use uuid::Uuid;
use zeroize::{Zeroize, ZeroizeOnDrop};

/// Opaque wrapper for secret byte slices that zeroizes on drop and never
/// serializes its contents in a human-readable form.
#[derive(Clone, Zeroize, ZeroizeOnDrop, Serialize, Deserialize)]
pub struct SecretBytes(pub Vec<u8>);

impl fmt::Debug for SecretBytes {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("SecretBytes([REDACTED])")
    }
}

/// Opaque wrapper for secret strings.
#[derive(Clone, Zeroize, ZeroizeOnDrop, Serialize, Deserialize)]
pub struct SecretString(pub String);

impl SecretString {
    pub fn new(s: impl Into<String>) -> Self {
        Self(s.into())
    }

    pub fn expose(&self) -> &str {
        &self.0
    }
}

impl fmt::Debug for SecretString {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("SecretString([REDACTED])")
    }
}

impl From<String> for SecretString {
    fn from(s: String) -> Self {
        Self(s)
    }
}

impl From<&str> for SecretString {
    fn from(s: &str) -> Self {
        Self(s.to_string())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum TotpAlgorithm {
    #[default]
    Sha1,
    Sha256,
    Sha512,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum SshAlgorithm {
    #[default]
    Ed25519,
    Rsa,
    Ecdsa,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PasswordHistoryItem {
    pub password: SecretString,
    pub replaced_at: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PasswordEntry {
    pub id: Uuid,
    pub name: String,
    pub username: String,
    pub password: SecretString,
    pub url: Option<String>,
    pub notes: Option<String>,
    pub tags: Vec<String>,
    pub created: u64,
    pub modified: u64,
    pub history: Vec<PasswordHistoryItem>,
}

impl PasswordEntry {
    pub fn new(
        name: impl Into<String>,
        username: impl Into<String>,
        password: impl Into<String>,
    ) -> Self {
        let now = crate::vault::unix_now();
        Self {
            id: Uuid::new_v4(),
            name: name.into(),
            username: username.into(),
            password: SecretString::new(password),
            url: None,
            notes: None,
            tags: Vec::new(),
            created: now,
            modified: now,
            history: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TotpEntry {
    pub id: Uuid,
    pub issuer: String,
    pub account: String,
    pub secret: SecretBytes,
    pub algorithm: TotpAlgorithm,
    pub digits: u8,
    pub period: u32,
    pub created: u64,
}

impl TotpEntry {
    pub fn new(
        issuer: impl Into<String>,
        account: impl Into<String>,
        secret_bytes: Vec<u8>,
    ) -> Self {
        Self {
            id: Uuid::new_v4(),
            issuer: issuer.into(),
            account: account.into(),
            secret: SecretBytes(secret_bytes),
            algorithm: TotpAlgorithm::Sha1,
            digits: 6,
            period: 30,
            created: crate::vault::unix_now(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SshKeyEntry {
    pub id: Uuid,
    pub name: String,
    pub algorithm: SshAlgorithm,
    pub public_key: String,
    pub private_key: SecretString,
    pub fingerprint: String,
    pub created: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileEntry {
    pub id: Uuid,
    pub name: String,
    pub size: u64,
    pub mime: Option<String>,
    pub blake3: [u8; 32],
    pub content: SecretBytes,
    pub added: u64,
}

impl FileEntry {
    pub fn new(name: impl Into<String>, content: Vec<u8>, mime: Option<String>) -> Self {
        let hash = *blake3::hash(&content).as_bytes();
        let size = content.len() as u64;
        Self {
            id: Uuid::new_v4(),
            name: name.into(),
            size,
            mime,
            blake3: hash,
            content: SecretBytes(content),
            added: crate::vault::unix_now(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NoteEntry {
    pub id: Uuid,
    pub title: String,
    pub body: SecretString,
    pub created: u64,
    pub modified: u64,
}

impl NoteEntry {
    pub fn new(title: impl Into<String>, body: impl Into<String>) -> Self {
        let now = crate::vault::unix_now();
        Self {
            id: Uuid::new_v4(),
            title: title.into(),
            body: SecretString::new(body),
            created: now,
            modified: now,
        }
    }
}
