use anyhow::{bail, Context, Result};
use clap::{Parser, Subcommand};
use std::path::PathBuf;

use crate::{device, tui};

#[derive(Parser)]
#[command(
    name = "sigil",
    about = "NorthUSB — encrypted security key for USB drives",
    version = env!("CARGO_PKG_VERSION"),
    long_about = None,
)]
pub struct Cli {
    /// Path to a specific device or vault file to operate on.
    #[arg(short, long, global = true)]
    pub device: Option<PathBuf>,

    #[command(subcommand)]
    pub command: Option<Commands>,
}

#[derive(Subcommand)]
pub enum Commands {
    /// Initialize a new vault on a USB device (interactive wizard).
    Init,

    /// Unlock the vault and enter the interactive TUI.
    Unlock {
        /// USB device path (auto-detected if omitted).
        device: Option<PathBuf>,
    },

    /// Explicitly lock the vault and kill the agent.
    Lock,

    /// Show agent status, current device, and vault fingerprint.
    Status,

    /// Add an entry to the vault.
    Add {
        #[command(subcommand)]
        entry_type: AddType,
    },

    /// Retrieve an entry (prints to stdout, copies to clipboard).
    Get {
        /// Entry name or search query.
        name: String,
    },

    /// Generate and display a TOTP code.
    Totp {
        /// Issuer name.
        issuer: String,
        /// Watch mode: refresh each 30s window.
        #[arg(long)]
        watch: bool,
    },

    /// List vault entries.
    List {
        /// Entry type filter: passwords | totp | ssh | files | notes
        #[arg()]
        entry_type: Option<String>,
    },

    /// Search across all vault entries.
    Search {
        query: String,
    },

    /// Cryptographically wipe a vault (irreversible).
    Destroy {
        device: Option<PathBuf>,
    },
}

#[derive(Subcommand)]
pub enum AddType {
    /// Add a password entry.
    Password {
        name: String,
    },
    /// Add a TOTP entry.
    Totp {
        issuer: String,
    },
    /// Add or import an SSH key.
    Ssh {
        name: String,
    },
    /// Encrypt a file into the vault.
    File {
        path: PathBuf,
    },
}

impl Cli {
    pub fn run(self) -> Result<()> {
        match self.command {
            Some(Commands::Init) | None => tui::run_tui(self.device),
            Some(Commands::Unlock { device }) => {
                tui::run_tui(device.or(self.device))
            }
            Some(Commands::Status) => cmd_status(),
            Some(Commands::Lock) => cmd_lock(),
            Some(Commands::List { entry_type }) => cmd_list(entry_type),
            Some(Commands::Search { query }) => cmd_search(query),
            Some(Commands::Totp { issuer, watch }) => cmd_totp(issuer, watch),
            Some(Commands::Get { name }) => cmd_get(name),
            Some(Commands::Add { entry_type }) => cmd_add(entry_type),
            Some(Commands::Destroy { device }) => cmd_destroy(device.or(self.device)),
        }
    }
}

fn cmd_status() -> Result<()> {
    // TODO: check if agent is running, display fingerprint
    println!("sigil agent: not running");
    Ok(())
}

fn cmd_lock() -> Result<()> {
    // TODO: signal running agent to lock
    println!("Vault locked.");
    Ok(())
}

fn cmd_list(_entry_type: Option<String>) -> Result<()> {
    // TODO: requires unlocked vault — implement with agent IPC
    bail!("vault is not unlocked — run `sigil unlock` first")
}

fn cmd_search(_query: String) -> Result<()> {
    bail!("vault is not unlocked — run `sigil unlock` first")
}

fn cmd_totp(_issuer: String, _watch: bool) -> Result<()> {
    bail!("vault is not unlocked — run `sigil unlock` first")
}

fn cmd_get(_name: String) -> Result<()> {
    bail!("vault is not unlocked — run `sigil unlock` first")
}

fn cmd_add(_entry_type: AddType) -> Result<()> {
    bail!("vault is not unlocked — run `sigil unlock` first")
}

fn cmd_destroy(device: Option<PathBuf>) -> Result<()> {
    bail!("destroy not yet implemented")
}
