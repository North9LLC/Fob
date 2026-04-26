use anyhow::Result;
use clap::{Parser, Subcommand};
use std::path::PathBuf;

use crate::tui;

/// The web UI is compiled into the binary so `sigil setup` can write it to USB
/// without needing any external files.
pub const WEB_INDEX_HTML: &str = include_str!("../../../web/index.html");

#[derive(Parser)]
#[command(
    name = "sigil",
    about = "NorthUSB — set up an encrypted vault on a USB drive",
    version = env!("CARGO_PKG_VERSION"),
    long_about = "Run `sigil` with no arguments to launch the interactive setup wizard.\n\
                  After setup, open index.html from your USB drive in a browser.",
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
    /// Interactive setup: format USB, create encrypted vault, write web UI.
    Setup,

    /// Re-open the TUI (advanced: add entries from the terminal).
    Unlock {
        device: Option<PathBuf>,
    },

    /// Show detected USB devices.
    Devices,
}

impl Cli {
    pub fn run(self) -> Result<()> {
        match self.command {
            None | Some(Commands::Setup) => tui::run_tui(self.device),
            Some(Commands::Unlock { device }) => tui::run_tui(device.or(self.device)),
            Some(Commands::Devices) => cmd_devices(),
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
    // Mark read-only on macOS so it can't be accidentally overwritten.
    #[cfg(target_os = "macos")]
    { let _ = std::process::Command::new("chflags").args(["uchg", &dest.to_string_lossy()]).status(); }
    Ok(())
}
