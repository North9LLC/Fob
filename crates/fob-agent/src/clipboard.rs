use std::time::{Duration, Instant};

/// Copy text to the system clipboard and schedule it to be cleared after
/// `clear_after` seconds.
///
/// Returns the Instant at which the clipboard should be cleared, so the
/// caller can schedule a check.
pub fn copy_with_clear_timer(text: &str, clear_after: u64) -> anyhow::Result<Instant> {
    let mut ctx = arboard::Clipboard::new()?;
    ctx.set_text(text)?;
    Ok(Instant::now() + Duration::from_secs(clear_after))
}

/// Clear the system clipboard.
pub fn clear() -> anyhow::Result<()> {
    let mut ctx = arboard::Clipboard::new()?;
    ctx.set_text("")?;
    Ok(())
}
