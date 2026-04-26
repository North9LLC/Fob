use anyhow::Result;
use clap::{Parser, Subcommand};
use std::path::PathBuf;

use crate::tui;

pub const WEB_INDEX_HTML: &str = include_str!("../../../web/index.html");

const RELEASES_API: &str =
    "https://api.github.com/repos/North9LLC/NorthUSB/releases/latest";
const INSTALL_URL: &str =
    "https://raw.githubusercontent.com/North9LLC/NorthUSB/main/install/install.sh";

#[derive(Parser)]
#[command(
    name = "sigil",
    about = "NorthUSB — encrypted vault on a USB drive",
    version = env!("CARGO_PKG_VERSION"),
    long_about = None,
)]
pub struct Cli {
    /// Path to a specific USB device or vault file.
    #[arg(short, long, global = true)]
    pub device: Option<PathBuf>,

    #[command(subcommand)]
    pub command: Option<Commands>,
}

#[derive(Subcommand)]
pub enum Commands {
    /// Interactive setup: format USB, create vault, write web UI.
    Setup,

    /// Open the TUI for an existing vault.
    Unlock {
        device: Option<PathBuf>,
    },

    /// Show detected USB drives.
    Devices,

    /// Check for and install updates.
    Update {
        /// Only print whether an update is available; don't install.
        #[arg(long)]
        check: bool,
    },
}

impl Cli {
    pub fn run(self) -> Result<()> {
        match self.command {
            None | Some(Commands::Setup) => tui::run_tui(self.device),
            Some(Commands::Unlock { device }) => tui::run_tui(device.or(self.device)),
            Some(Commands::Devices) => cmd_devices(),
            Some(Commands::Update { check }) => cmd_update(check),
        }
    }
}

/// List detected removable USB devices (does not require unlocking).
fn cmd_devices() -> Result<()> {
    let devices = crate::device::enumerate_usb_devices();
    if devices.is_empty() {
        println!("No USB drives detected. Insert a USB drive and retry.");
        return Ok(());
    }
    println!("Detected USB drives:");
    for (i, d) in devices.iter().enumerate() {
        let vault = if d.has_sigil_vault { " [vault present]" } else { "" };
        println!("  [{}]  {}  {}  {}{}", i+1, d.name, d.size_display(), d.path.display(), vault);
    }
    Ok(())
}

/// Write the embedded web UI to the USB device path.
pub fn write_web_ui(device_path: &std::path::Path) -> Result<()> {
    let dest = device_path.join("index.html");
    std::fs::write(&dest, WEB_INDEX_HTML)?;
    #[cfg(target_os = "macos")]
    { let _ = std::process::Command::new("chflags").args(["uchg", &dest.to_string_lossy()]).status(); }
    Ok(())
}

/// Check GitHub for a newer release and optionally install it.
fn cmd_update(check_only: bool) -> Result<()> {
    use std::io::Write as _;

    let current = concat!("v", env!("CARGO_PKG_VERSION"));
    println!("Current version: {current}");
    print!("Checking for updates…  ");
    std::io::stdout().flush()?;

    // Fetch latest release tag via GitHub API using the system curl.
    let out = std::process::Command::new("curl")
        .args([
            "-fsSL", "--max-time", "10",
            "-H", "Accept: application/vnd.github+json",
            "-H", "X-GitHub-Api-Version: 2022-11-28",
            RELEASES_API,
        ])
        .output();

    let latest = match out {
        Ok(o) if o.status.success() => {
            let body = String::from_utf8_lossy(&o.stdout);
            // Extract "tag_name" without pulling in a JSON dep.
            body.lines()
                .find(|l| l.contains("\"tag_name\""))
                .and_then(|l| l.split('"').nth(3))
                .map(str::to_owned)
                .unwrap_or_else(|| "unknown".into())
        }
        _ => {
            println!("could not reach GitHub. Check your connection.");
            return Ok(());
        }
    };

    if latest == "unknown" {
        println!("could not parse release info.");
        return Ok(());
    }

    if latest == current {
        println!("up to date ✓");
        return Ok(());
    }

    println!("update available → {latest}");

    if check_only {
        println!(
            "\nRun `sigil update` or re-run the install script to upgrade:\n  curl -fsSL {INSTALL_URL} | sh"
        );
        return Ok(());
    }

    print!("\nInstall {latest} now? [y/N] ");
    std::io::stdout().flush()?;
    let mut input = String::new();
    std::io::stdin().read_line(&mut input)?;

    if input.trim().eq_ignore_ascii_case("y") {
        println!("Running install script…");
        let status = std::process::Command::new("sh")
            .arg("-c")
            .arg(format!("curl -fsSL {INSTALL_URL} | sh -s -- --no-path"))
            .status()?;
        if status.success() {
            println!("\n✓ Updated. Restart sigil to use {latest}.");
        } else {
            println!("\nInstall script failed. Try manually:\n  curl -fsSL {INSTALL_URL} | sh");
        }
    } else {
        println!("\nTo update manually:\n  curl -fsSL {INSTALL_URL} | sh");
    }

    Ok(())
}
