mod cli;
mod device;
mod tui;

use anyhow::Result;
use cli::Cli;
use clap::Parser;

fn main() -> Result<()> {
    // Disable core dumps on Linux before doing anything else.
    #[cfg(target_os = "linux")]
    unsafe {
        libc::prctl(libc::PR_SET_DUMPABLE, 0, 0, 0, 0);
    }

    let cli = Cli::parse();
    cli.run()
}
