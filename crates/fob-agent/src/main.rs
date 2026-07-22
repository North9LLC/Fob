/// `fob-agent` — SSH agent daemon, spawned by the `fob` CLI on unlock.
///
/// Not meant to be run standalone: it reads its identity set (JSON-encoded
/// `Vec<SshKeyEntry>`) from stdin once at startup and never re-reads it —
/// the caller (fob-cli) restarts the process whenever the vault's SSH keys
/// change. This keeps the vault-unlock/passphrase logic entirely in fob-cli;
/// fob-agent only ever sees already-decrypted key material.
use std::io::Read;
use std::path::PathBuf;

use fob_core::types::SshKeyEntry;

fn main() -> anyhow::Result<()> {
    let socket_path = parse_socket_arg()?;

    let mut input = String::new();
    std::io::stdin().read_to_string(&mut input)?;
    let entries: Vec<SshKeyEntry> = serde_json::from_str(&input)
        .map_err(|e| anyhow::anyhow!("failed to parse SSH keys from stdin: {e}"))?;

    let mut agent = fob_agent::SshAgent::new(socket_path);
    agent.add_keys(entries);

    let rt = tokio::runtime::Runtime::new()?;
    rt.block_on(agent.run())
}

fn parse_socket_arg() -> anyhow::Result<PathBuf> {
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        if arg == "--socket" {
            return args
                .next()
                .map(PathBuf::from)
                .ok_or_else(|| anyhow::anyhow!("--socket requires a path argument"));
        }
    }
    Err(anyhow::anyhow!("usage: fob-agent --socket <path>"))
}
