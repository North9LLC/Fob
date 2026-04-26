/// SSH agent protocol implementation stub.
///
/// Full implementation per draft-miller-ssh-agent:
/// https://datatracker.ietf.org/doc/html/draft-miller-ssh-agent
///
/// Exposes a Unix domain socket at $XDG_RUNTIME_DIR/sigil-agent.sock
/// and responds to SSH_AGENTC_REQUEST_IDENTITIES, SSH_AGENTC_SIGN_REQUEST,
/// etc. from SSH clients that have SSH_AUTH_SOCK set.

use std::path::PathBuf;
use sigil_core::types::SshKeyEntry;

pub struct SshAgent {
    socket_path: PathBuf,
    keys: Vec<SshKeyEntry>,
}

impl SshAgent {
    pub fn new(socket_path: PathBuf) -> Self {
        Self { socket_path, keys: Vec::new() }
    }

    pub fn add_keys(&mut self, keys: Vec<SshKeyEntry>) {
        self.keys = keys;
    }

    pub fn socket_path(&self) -> &PathBuf {
        &self.socket_path
    }

    /// Start listening on the socket. Returns when the agent is stopped.
    pub async fn run(self) -> anyhow::Result<()> {
        // TODO: implement SSH agent protocol over Unix socket
        // Message types to handle:
        //   SSH2_AGENTC_REQUEST_IDENTITIES (11) → SSH2_AGENT_IDENTITIES_ANSWER
        //   SSH2_AGENTC_SIGN_REQUEST (13)       → SSH2_AGENT_SIGN_RESPONSE
        //   SSH_AGENTC_REMOVE_ALL_IDENTITIES(19) → SSH_AGENT_SUCCESS
        Ok(())
    }
}

impl Drop for SshAgent {
    fn drop(&mut self) {
        // Remove socket file on drop so SSH clients don't see a stale socket.
        let _ = std::fs::remove_file(&self.socket_path);
    }
}
