/// SSH agent protocol implementation, per draft-miller-ssh-agent (the de
/// facto standard OpenSSH implements).
///
/// Exposes a Unix domain socket that responds to SSH_AGENTC_REQUEST_IDENTITIES
/// and SSH_AGENTC_SIGN_REQUEST from any SSH client with SSH_AUTH_SOCK pointed
/// at it. The identity set comes from the vault (via `add_keys`) and is not
/// mutable over the wire — SSH_AGENTC_REMOVE_ALL_IDENTITIES is accepted as a
/// no-op rather than actually clearing anything.
use std::path::PathBuf;
use std::sync::Arc;

use signature::Signer;
use tokio::net::{UnixListener, UnixStream};

use crate::proto::{self, Reader};
use fob_core::types::SshKeyEntry;

struct LoadedKey {
    /// Raw SSH wire-format public key blob — what clients present in
    /// SSH_AGENTC_SIGN_REQUEST to identify which key to sign with.
    blob: Vec<u8>,
    comment: String,
    private_key: ssh_key::PrivateKey,
}

pub struct SshAgent {
    socket_path: PathBuf,
    keys: Arc<Vec<LoadedKey>>,
}

impl SshAgent {
    pub fn new(socket_path: PathBuf) -> Self {
        Self {
            socket_path,
            keys: Arc::new(Vec::new()),
        }
    }

    /// Parse vault SSH key entries into signing-ready keys.
    ///
    /// Entries that fail to parse, or that are still passphrase-encrypted,
    /// are skipped (logged to stderr) rather than treated as fatal — Fob's
    /// vault passphrase is already the protection layer, so imported keys
    /// are expected to have their own SSH-level passphrase stripped first
    /// (`ssh-keygen -p -N ""`).
    pub fn add_keys(&mut self, entries: Vec<SshKeyEntry>) {
        let mut loaded = Vec::with_capacity(entries.len());
        for entry in entries {
            match ssh_key::PrivateKey::from_openssh(entry.private_key.expose()) {
                Ok(key) if key.is_encrypted() => {
                    eprintln!(
                        "fob-agent: skipping '{}' — passphrase-protected SSH keys aren't \
                         supported; strip the key's own passphrase before importing it",
                        entry.name
                    );
                }
                Ok(key) => match key.public_key().to_bytes() {
                    Ok(blob) => loaded.push(LoadedKey {
                        blob,
                        comment: entry.name,
                        private_key: key,
                    }),
                    Err(e) => {
                        eprintln!(
                            "fob-agent: skipping '{}' — could not encode public key: {e}",
                            entry.name
                        );
                    }
                },
                Err(e) => {
                    eprintln!(
                        "fob-agent: skipping '{}' — invalid SSH private key: {e}",
                        entry.name
                    );
                }
            }
        }
        self.keys = Arc::new(loaded);
    }

    pub fn socket_path(&self) -> &PathBuf {
        &self.socket_path
    }

    /// Start listening on the socket. Runs until the listener errors out
    /// (e.g. the socket file is removed from under it) or the process
    /// receives SIGTERM/Ctrl-C — either way, returning drops `self`, which
    /// removes the socket file. A raw `kill`/SIGKILL bypasses this like any
    /// other destructor, which is why fob-cli also unlinks the socket path
    /// itself after asking the child to exit.
    pub async fn run(self) -> anyhow::Result<()> {
        if let Some(parent) = self.socket_path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let _ = std::fs::remove_file(&self.socket_path);
        let listener = UnixListener::bind(&self.socket_path)?;

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&self.socket_path, std::fs::Permissions::from_mode(0o600))?;
        }

        tokio::select! {
            result = Self::accept_loop(&listener, &self.keys) => result,
            _ = shutdown_signal() => Ok(()),
        }
    }

    /// Max simultaneously in-flight connections. Each accepted stream is
    /// otherwise `tokio::spawn`ed with no limit at all, so a burst of
    /// requests (or a hostile/buggy local client opening many connections)
    /// could exhaust the process's fd table — which is exactly the
    /// condition that used to make `accept()` itself start failing and
    /// take the whole daemon down (see below). This bounds that.
    const MAX_CONCURRENT_CONNECTIONS: usize = 64;

    async fn accept_loop(
        listener: &UnixListener,
        keys: &Arc<Vec<LoadedKey>>,
    ) -> anyhow::Result<()> {
        let permits = Arc::new(tokio::sync::Semaphore::new(
            Self::MAX_CONCURRENT_CONNECTIONS,
        ));
        loop {
            let (stream, _) = match listener.accept().await {
                Ok(conn) => conn,
                Err(e) => {
                    // A single transient accept() failure (e.g. temporary fd
                    // exhaustion) must not take down the whole agent —
                    // propagating it here used to exit the entire process
                    // (see `run`'s `tokio::select!`), silently (stdio is
                    // piped to /dev/null by the spawning side) leaving every
                    // already-unlocked SSH client without signing service
                    // until the next unlock/key-add respawns it. Log and
                    // back off briefly instead, so a persistent failure
                    // doesn't spin the CPU at 100%.
                    eprintln!("fob-agent: accept() failed: {e}");
                    tokio::time::sleep(std::time::Duration::from_millis(100)).await;
                    continue;
                }
            };
            let keys = Arc::clone(keys);
            let permits = Arc::clone(&permits);
            tokio::spawn(async move {
                // Semaphore is only ever closed by being dropped, which
                // doesn't happen here — acquire_owned() failing would mean
                // that, so this is unreachable in practice.
                let Ok(_permit) = permits.acquire_owned().await else {
                    return;
                };
                if let Err(e) = handle_connection(stream, keys).await {
                    if e.kind() != std::io::ErrorKind::UnexpectedEof {
                        eprintln!("fob-agent: connection error: {e}");
                    }
                }
            });
        }
    }
}

/// Resolves once on Ctrl-C or (on Unix) SIGTERM — whichever comes first.
async fn shutdown_signal() {
    #[cfg(unix)]
    {
        let mut term = tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
            .expect("failed to install SIGTERM handler");
        tokio::select! {
            _ = tokio::signal::ctrl_c() => {},
            _ = term.recv() => {},
        }
    }
    #[cfg(not(unix))]
    {
        let _ = tokio::signal::ctrl_c().await;
    }
}

impl Drop for SshAgent {
    fn drop(&mut self) {
        // Remove socket file on drop so SSH clients don't see a stale socket.
        let _ = std::fs::remove_file(&self.socket_path);
    }
}

async fn handle_connection(
    mut stream: UnixStream,
    keys: Arc<Vec<LoadedKey>>,
) -> std::io::Result<()> {
    loop {
        let body = proto::read_message(&mut stream).await?;
        let response = handle_request(&body, &keys);
        proto::write_message(&mut stream, &response).await?;
    }
}

fn handle_request(body: &[u8], keys: &[LoadedKey]) -> Vec<u8> {
    let mut r = Reader::new(body);
    let Some(msg_type) = r.read_u8() else {
        return vec![proto::SSH_AGENT_FAILURE];
    };

    match msg_type {
        proto::SSH_AGENTC_REQUEST_IDENTITIES => {
            let mut out = vec![proto::SSH_AGENT_IDENTITIES_ANSWER];
            proto::write_u32(&mut out, keys.len() as u32);
            for k in keys {
                proto::write_string(&mut out, &k.blob);
                proto::write_string(&mut out, k.comment.as_bytes());
            }
            out
        }

        proto::SSH_AGENTC_SIGN_REQUEST => {
            let (Some(key_blob), Some(data)) = (r.read_string(), r.read_string()) else {
                return vec![proto::SSH_AGENT_FAILURE];
            };
            // A uint32 `flags` field follows (SSH_AGENT_RSA_SHA2_256/512),
            // which we deliberately don't read: ssh-key's RSA signer always
            // uses rsa-sha2-512, already the modern default OpenSSH prefers.

            match keys.iter().find(|k| k.blob == key_blob) {
                Some(k) => match k.private_key.try_sign(data) {
                    Ok(sig) => match Vec::<u8>::try_from(sig) {
                        Ok(sig_bytes) => {
                            let mut out = vec![proto::SSH_AGENT_SIGN_RESPONSE];
                            proto::write_string(&mut out, &sig_bytes);
                            out
                        }
                        Err(_) => vec![proto::SSH_AGENT_FAILURE],
                    },
                    Err(_) => vec![proto::SSH_AGENT_FAILURE],
                },
                None => vec![proto::SSH_AGENT_FAILURE],
            }
        }

        proto::SSH_AGENTC_REMOVE_ALL_IDENTITIES => {
            // No-op: the identity set is derived from the vault, not mutable
            // over the wire — but report success so well-behaved clients
            // (e.g. `ssh-add -D`) don't treat this as an error.
            vec![proto::SSH_AGENT_SUCCESS]
        }

        _ => vec![proto::SSH_AGENT_FAILURE],
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    // A real disposable ed25519 test keypair generated with `ssh-keygen -t
    // ed25519 -N ""` (not used anywhere else, not a secret).
    const TEST_PUBKEY: &str = "ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAIEJm7X5tIxbUkIb6VLD91P65Cr0iqKyTKTDd0cYpQHtv test@example";
    const TEST_PRIVKEY: &str = "-----BEGIN OPENSSH PRIVATE KEY-----\nb3BlbnNzaC1rZXktdjEAAAAABG5vbmUAAAAEbm9uZQAAAAAAAAABAAAAMwAAAAtzc2gtZW\nQyNTUxOQAAACBCZu1+bSMW1JCG+lSw/dT+uQq9Iqiskykw3dHGKUB7bwAAAJDW2GvD1thr\nwwAAAAtzc2gtZWQyNTUxOQAAACBCZu1+bSMW1JCG+lSw/dT+uQq9Iqiskykw3dHGKUB7bw\nAAAEA2NzJWP1E87pnjoqaDyFo1ZbFu/Uu+ne8zT6+RRUBKPkJm7X5tIxbUkIb6VLD91P65\nCr0iqKyTKTDd0cYpQHtvAAAADHRlc3RAZXhhbXBsZQE=\n-----END OPENSSH PRIVATE KEY-----\n";

    fn test_entry() -> SshKeyEntry {
        SshKeyEntry::new("laptop", TEST_PUBKEY, TEST_PRIVKEY).unwrap()
    }

    #[test]
    fn add_keys_loads_a_valid_entry() {
        let mut agent = SshAgent::new(PathBuf::from("/tmp/does-not-matter.sock"));
        agent.add_keys(vec![test_entry()]);
        assert_eq!(agent.keys.len(), 1);
        assert_eq!(agent.keys[0].comment, "laptop");
    }

    #[test]
    fn add_keys_skips_garbage_private_key() {
        let mut entry = test_entry();
        entry.private_key = fob_core::types::SecretString::new("not a key");
        let mut agent = SshAgent::new(PathBuf::from("/tmp/does-not-matter.sock"));
        agent.add_keys(vec![entry]);
        assert_eq!(agent.keys.len(), 0);
    }

    #[test]
    fn request_identities_lists_loaded_keys() {
        let mut agent = SshAgent::new(PathBuf::from("/tmp/does-not-matter.sock"));
        agent.add_keys(vec![test_entry()]);

        let response = handle_request(&[proto::SSH_AGENTC_REQUEST_IDENTITIES], &agent.keys);
        let mut r = Reader::new(&response);
        assert_eq!(r.read_u8(), Some(proto::SSH_AGENT_IDENTITIES_ANSWER));
        assert_eq!(r.read_u32(), Some(1));
        let blob = r.read_string().unwrap();
        assert_eq!(blob, agent.keys[0].blob.as_slice());
        assert_eq!(r.read_string(), Some(&b"laptop"[..]));
    }

    #[test]
    fn sign_request_produces_verifiable_signature() {
        // `PublicKey` has both an inherent `verify()` (SSHSIG-namespace,
        // 3-arg) and the `signature::Verifier<Signature>` trait's 2-arg
        // `verify()` for raw signatures — method-call syntax always picks
        // the inherent one, so the trait method needs UFCS here.
        use signature::Verifier;

        let mut agent = SshAgent::new(PathBuf::from("/tmp/does-not-matter.sock"));
        agent.add_keys(vec![test_entry()]);
        let blob = agent.keys[0].blob.clone();

        let mut req = vec![proto::SSH_AGENTC_SIGN_REQUEST];
        proto::write_string(&mut req, &blob);
        proto::write_string(&mut req, b"data to sign");
        proto::write_u32(&mut req, 0);

        let response = handle_request(&req, &agent.keys);
        let mut r = Reader::new(&response);
        assert_eq!(r.read_u8(), Some(proto::SSH_AGENT_SIGN_RESPONSE));
        let sig_bytes = r.read_string().unwrap();

        let sig = ssh_key::Signature::try_from(sig_bytes).unwrap();
        let public_key = ssh_key::PublicKey::from_openssh(TEST_PUBKEY).unwrap();
        Verifier::verify(&public_key, b"data to sign", &sig).unwrap();
    }

    #[test]
    fn sign_request_fails_for_unknown_key() {
        let agent = SshAgent::new(PathBuf::from("/tmp/does-not-matter.sock"));
        let mut req = vec![proto::SSH_AGENTC_SIGN_REQUEST];
        proto::write_string(&mut req, b"not-a-real-blob");
        proto::write_string(&mut req, b"data");
        proto::write_u32(&mut req, 0);

        let response = handle_request(&req, &agent.keys);
        assert_eq!(response, vec![proto::SSH_AGENT_FAILURE]);
    }

    #[test]
    fn remove_all_identities_is_accepted_as_a_noop() {
        let agent = SshAgent::new(PathBuf::from("/tmp/does-not-matter.sock"));
        let response = handle_request(&[proto::SSH_AGENTC_REMOVE_ALL_IDENTITIES], &agent.keys);
        assert_eq!(response, vec![proto::SSH_AGENT_SUCCESS]);
    }

    #[test]
    fn unknown_message_type_fails() {
        let agent = SshAgent::new(PathBuf::from("/tmp/does-not-matter.sock"));
        let response = handle_request(&[200u8], &agent.keys);
        assert_eq!(response, vec![proto::SSH_AGENT_FAILURE]);
    }

    #[tokio::test]
    async fn full_socket_round_trip_request_identities() {
        let dir = std::env::temp_dir().join(format!("fob-agent-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let socket_path = dir.join("agent.sock");

        let mut agent = SshAgent::new(socket_path.clone());
        agent.add_keys(vec![test_entry()]);

        let server = tokio::spawn(agent.run());
        // Give the listener a moment to bind.
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;

        let mut client = UnixStream::connect(&socket_path).await.unwrap();
        let req = vec![proto::SSH_AGENTC_REQUEST_IDENTITIES];
        client
            .write_all(&(req.len() as u32).to_be_bytes())
            .await
            .unwrap();
        client.write_all(&req).await.unwrap();

        let mut len_buf = [0u8; 4];
        client.read_exact(&mut len_buf).await.unwrap();
        let len = u32::from_be_bytes(len_buf) as usize;
        let mut body = vec![0u8; len];
        client.read_exact(&mut body).await.unwrap();

        assert_eq!(body[0], proto::SSH_AGENT_IDENTITIES_ANSWER);

        server.abort();
        std::fs::remove_dir_all(&dir).ok();
    }
}
