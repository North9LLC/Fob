mod app;
mod screens;
mod state;
mod widgets;

use anyhow::Result;
use std::path::PathBuf;

pub use app::App;
pub use state::{Screen, VaultView, WizardStep};

/// Entry point: launch the full TUI.
pub fn run_tui(device: Option<PathBuf>) -> Result<()> {
    app::run(device)
}
