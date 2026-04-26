use std::path::PathBuf;
use std::time::{Duration, Instant};

use anyhow::Result;
use crossterm::{
    event::{self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode, KeyModifiers},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{backend::CrosstermBackend, Terminal};

use super::{
    screens,
    state::{AppState, Screen},
};
use crate::device;

const TICK_RATE: Duration = Duration::from_millis(60);
const BOOT_TICKS: u8 = 20;

pub struct App {
    pub state: AppState,
}

impl App {
    pub fn new(device: Option<PathBuf>) -> Self {
        let devices = device::enumerate_usb_devices();
        let mut state = AppState::new(devices);

        // If a device path was specified, try to preselect it.
        if let Some(dev_path) = device {
            for (i, d) in state.devices.iter().enumerate() {
                if d.path == dev_path {
                    state.selected_device = i;
                    break;
                }
            }
        }

        Self { state }
    }

    /// Main event loop — runs until the user quits.
    pub fn run_loop(&mut self, terminal: &mut Terminal<CrosstermBackend<std::io::Stdout>>) -> Result<()> {
        let mut last_tick = Instant::now();

        loop {
            // Draw the current screen.
            terminal.draw(|frame| screens::render(frame, &self.state))?;

            // Poll for input or tick.
            let timeout = TICK_RATE
                .checked_sub(last_tick.elapsed())
                .unwrap_or(Duration::ZERO);

            if event::poll(timeout)? {
                if let Event::Key(key) = event::read()? {
                    self.state.touch_activity();
                    if self.handle_key(key.code, key.modifiers)? {
                        return Ok(());
                    }
                }
            }

            // Tick for animations.
            if last_tick.elapsed() >= TICK_RATE {
                self.tick();
                last_tick = Instant::now();
            }

            // Auto-lock check.
            if self.state.is_auto_lock_due() {
                self.do_lock();
            }

            // Clipboard auto-clear check.
            if let Some(clear_at) = self.state.clipboard_clear_at {
                if Instant::now() >= clear_at {
                    let _ = clear_clipboard();
                    self.state.clipboard_clear_at = None;
                }
            }
        }
    }

    fn tick(&mut self) {
        match &self.state.screen {
            Screen::Boot => {
                self.state.boot_tick = self.state.boot_tick.saturating_add(1);
                if self.state.boot_tick >= BOOT_TICKS {
                    let next = if self.state.devices.is_empty() {
                        Screen::Error(
                            "No USB devices detected.\n\
                             Insert a USB drive and restart sigil.".into(),
                        )
                    } else {
                        Screen::DevicePicker
                    };
                    self.state.transition(next);
                }
            }
            _ => {}
        }
    }

    /// Returns true if the application should exit.
    fn handle_key(&mut self, key: KeyCode, mods: KeyModifiers) -> Result<bool> {
        // Ctrl-C always exits.
        if key == KeyCode::Char('c') && mods.contains(KeyModifiers::CONTROL) {
            self.do_lock();
            return Ok(true);
        }

        let screen = self.state.screen.clone();
        match screen {
            Screen::Boot => {
                // Any key skips the boot animation.
                self.state.boot_tick = BOOT_TICKS;
            }
            Screen::DevicePicker => self.handle_device_picker(key)?,
            Screen::Unlock => {
                if self.handle_unlock(key)? {
                    return Ok(true);
                }
            }
            Screen::SetupWizard(ref step) => {
                let step = step.clone();
                if self.handle_wizard(key, step)? {
                    return Ok(true);
                }
            }
            Screen::Vault(_) => {
                if self.handle_vault(key)? {
                    return Ok(true);
                }
            }
            Screen::Locked => {
                match key {
                    KeyCode::Char('q') => return Ok(true),
                    KeyCode::Enter | KeyCode::Char(' ') => {
                        self.state.screen = Screen::Unlock;
                    }
                    _ => {}
                }
            }
            Screen::Error(_) => {
                if key == KeyCode::Char('q') || key == KeyCode::Esc {
                    return Ok(true);
                }
            }
        }

        Ok(false)
    }

    fn handle_device_picker(&mut self, key: KeyCode) -> Result<()> {
        let n = self.state.devices.len();
        match key {
            KeyCode::Char('q') | KeyCode::Esc => {
                self.state.transition(Screen::Error("Aborted.".into()));
            }
            KeyCode::Up | KeyCode::Char('k') => {
                if self.state.selected_device > 0 {
                    self.state.selected_device -= 1;
                }
            }
            KeyCode::Down | KeyCode::Char('j') => {
                if self.state.selected_device + 1 < n {
                    self.state.selected_device += 1;
                }
            }
            KeyCode::Enter | KeyCode::Char(' ') => {
                if let Some(dev) = self.state.devices.get(self.state.selected_device) {
                    if dev.has_sigil_vault {
                        self.state.transition(Screen::Unlock);
                    } else {
                        self.state
                            .transition(Screen::SetupWizard(super::state::WizardStep::Master));
                    }
                }
            }
            KeyCode::Char(c) if c.is_ascii_digit() => {
                let idx = (c as usize) - ('1' as usize);
                if idx < n {
                    self.state.selected_device = idx;
                }
            }
            _ => {}
        }
        Ok(())
    }

    fn handle_unlock(&mut self, key: KeyCode) -> Result<bool> {
        match key {
            KeyCode::Esc => {
                use zeroize::Zeroize;
                self.state.passphrase_input.zeroize();
                self.state.screen = Screen::DevicePicker;
            }
            KeyCode::Char(c) => {
                self.state.passphrase_input.push(c);
            }
            KeyCode::Backspace => {
                self.state.passphrase_input.pop();
            }
            KeyCode::Enter => {
                let result = self.try_unlock();
                match result {
                    Ok(()) => {}
                    Err(e) => {
                        use zeroize::Zeroize;
                        self.state.passphrase_input.zeroize();
                        // Brief error — stay on unlock screen, will flash.
                        // For now, transition to error if max retries exceeded (TODO).
                    }
                }
            }
            _ => {}
        }
        Ok(false)
    }

    fn try_unlock(&mut self) -> Result<()> {
        use sigil_core::{kdf, vault};
        use zeroize::Zeroize;

        let dev = self
            .state
            .devices
            .get(self.state.selected_device)
            .ok_or_else(|| anyhow::anyhow!("no device selected"))?;

        // Find vault file on device.
        let vault_path = dev.path.join("vault.sigil");
        if !vault_path.exists() {
            anyhow::bail!("no vault file found at {:?}", vault_path);
        }

        let vault_bytes = std::fs::read(&vault_path)?;
        let pass_bytes = self.state.passphrase_input.as_bytes().to_vec();

        // Use the duress-wipe variant so entering the duress passphrase
        // silently destroys the vault file and returns a generic error.
        let result =
            vault::unlock_vault_with_duress_wipe(&vault_bytes, &pass_bytes, &vault_path);

        // Zeroize passphrase immediately after use.
        self.state.passphrase_input.zeroize();

        let (slot, blob) = result?;

        // Derive slot keys for re-encryption on save.
        let header = sigil_core::format::VaultHeader::parse(&vault_bytes)?;
        let kdf_out = kdf::derive_master(&pass_bytes, &header.salt)?;
        let slot_keys = kdf::derive_all_slot_keys(kdf_out.master_secret());

        // Compute vault fingerprint.
        let fp_bytes = blake3::hash(&header.salt);
        let fp_hex = hex::encode(&fp_bytes.as_bytes()[..8]);
        let fingerprint = format!(
            "{}-{}-{}-{}",
            &fp_hex[0..4].to_uppercase(),
            &fp_hex[4..8].to_uppercase(),
            &fp_hex[8..12].to_uppercase(),
            &fp_hex[12..16].to_uppercase()
        );

        self.state.vault = Some(super::state::OpenVault {
            slot,
            blob,
            vault_path,
            fingerprint,
            slot_keys,
            last_activity: std::time::Instant::now(),
        });

        self.state.screen = Screen::Vault(super::state::VaultView::Passwords);
        Ok(())
    }

    fn handle_wizard(&mut self, key: KeyCode, step: super::state::WizardStep) -> Result<bool> {
        use super::state::WizardStep;
        match step {
            WizardStep::Master => {
                match self.state.wizard.field {
                    0 => handle_text_input(&mut self.state.wizard.main_pass, key),
                    1 => handle_text_input(&mut self.state.wizard.main_pass_confirm, key),
                    _ => {}
                }
                match key {
                    KeyCode::Tab => {
                        self.state.wizard.field = (self.state.wizard.field + 1) % 2;
                    }
                    KeyCode::Enter if self.state.wizard.field == 1 => {
                        if self.state.wizard.main_pass == self.state.wizard.main_pass_confirm
                            && !self.state.wizard.main_pass.is_empty()
                        {
                            self.state.transition(Screen::SetupWizard(WizardStep::Decoy));
                            self.state.wizard.field = 0;
                        }
                    }
                    KeyCode::Esc => {
                        self.state.transition(Screen::DevicePicker);
                    }
                    _ => {}
                }
            }
            WizardStep::Decoy => {
                match key {
                    KeyCode::Char('y') | KeyCode::Char('Y') => {
                        self.state.wizard.decoy_enabled = true;
                        self.state.transition(Screen::SetupWizard(WizardStep::DecoyPassphrase));
                    }
                    KeyCode::Char('n') | KeyCode::Char('N') => {
                        self.state.wizard.decoy_enabled = false;
                        self.state.transition(Screen::SetupWizard(WizardStep::Duress));
                    }
                    KeyCode::Esc => {
                        self.state.transition(Screen::SetupWizard(WizardStep::Master));
                    }
                    _ => {}
                }
            }
            WizardStep::DecoyPassphrase => {
                handle_text_input(&mut self.state.wizard.decoy_pass, key);
                match key {
                    KeyCode::Enter if !self.state.wizard.decoy_pass.is_empty() => {
                        self.state.transition(Screen::SetupWizard(WizardStep::Duress));
                    }
                    KeyCode::Esc => {
                        use zeroize::Zeroize;
                        self.state.wizard.decoy_pass.zeroize();
                        self.state.wizard.decoy_enabled = false;
                        self.state.transition(Screen::SetupWizard(WizardStep::Decoy));
                    }
                    _ => {}
                }
            }
            WizardStep::Duress => {
                match key {
                    KeyCode::Char('y') | KeyCode::Char('Y') => {
                        self.state.wizard.duress_enabled = true;
                        self.state.transition(Screen::SetupWizard(WizardStep::DuressPassphrase));
                    }
                    KeyCode::Char('n') | KeyCode::Char('N') => {
                        self.state.wizard.duress_enabled = false;
                        self.state.transition(Screen::SetupWizard(WizardStep::Confirm));
                    }
                    KeyCode::Esc => {
                        if self.state.wizard.decoy_enabled {
                            self.state.transition(Screen::SetupWizard(WizardStep::DecoyPassphrase));
                        } else {
                            self.state.transition(Screen::SetupWizard(WizardStep::Decoy));
                        }
                    }
                    _ => {}
                }
            }
            WizardStep::DuressPassphrase => {
                handle_text_input(&mut self.state.wizard.duress_pass, key);
                match key {
                    KeyCode::Enter if !self.state.wizard.duress_pass.is_empty() => {
                        self.state.transition(Screen::SetupWizard(WizardStep::Confirm));
                    }
                    KeyCode::Esc => {
                        use zeroize::Zeroize;
                        self.state.wizard.duress_pass.zeroize();
                        self.state.wizard.duress_enabled = false;
                        self.state.transition(Screen::SetupWizard(WizardStep::Duress));
                    }
                    _ => {}
                }
            }
            WizardStep::Confirm => {
                match key {
                    KeyCode::Enter => {
                        let result = self.run_vault_init();
                        match result {
                            Ok(()) => {
                                self.state.transition(Screen::Unlock);
                            }
                            Err(e) => {
                                self.state.error_message = e.to_string();
                                self.state.transition(Screen::Error(e.to_string()));
                            }
                        }
                    }
                    KeyCode::Esc => {
                        self.state.transition(Screen::SetupWizard(WizardStep::Duress));
                    }
                    _ => {}
                }
            }
        }
        Ok(false)
    }

    fn run_vault_init(&mut self) -> Result<()> {
        use sigil_core::vault::{init_vault, VaultInitParams};
        use zeroize::Zeroize;

        let dev = self
            .state
            .devices
            .get(self.state.selected_device)
            .ok_or_else(|| anyhow::anyhow!("no device selected"))?;

        let vault_path = dev.path.join("vault.sigil");

        let decoy_pass = if self.state.wizard.decoy_enabled {
            Some(self.state.wizard.decoy_pass.as_bytes().to_vec())
        } else {
            None
        };
        let duress_pass = if self.state.wizard.duress_enabled {
            Some(self.state.wizard.duress_pass.as_bytes().to_vec())
        } else {
            None
        };

        let vault_bytes = init_vault(VaultInitParams {
            main_passphrase: self.state.wizard.main_pass.as_bytes().to_vec(),
            decoy_passphrase: decoy_pass,
            duress_passphrase: duress_pass,
            vault_size: sigil_core::format::DEFAULT_VAULT_SIZE,
            decoy_blob: None,
        })?;

        std::fs::write(&vault_path, &vault_bytes)
            .with_context(|| format!("writing vault to {:?}", vault_path))?;

        // Zeroize wizard state.
        self.state.wizard.main_pass.zeroize();
        self.state.wizard.main_pass_confirm.zeroize();
        self.state.wizard.decoy_pass.zeroize();
        self.state.wizard.duress_pass.zeroize();

        Ok(())
    }

    fn handle_vault(&mut self, key: KeyCode) -> Result<bool> {
        match key {
            KeyCode::Char('q') => {
                self.do_lock();
                return Ok(true);
            }
            KeyCode::Char('l') => {
                self.do_lock();
                self.state.screen = Screen::Locked;
            }
            KeyCode::Tab => {
                self.state.content_focus = !self.state.content_focus;
            }
            KeyCode::Char('j') | KeyCode::Down => {
                self.state.content_cursor = self.state.content_cursor.saturating_add(1);
            }
            KeyCode::Char('k') | KeyCode::Up => {
                self.state.content_cursor = self.state.content_cursor.saturating_sub(1);
            }
            // Sidebar navigation with h/l (or J/K for tabs).
            KeyCode::Char('J') => {
                let view = self.state.vault_view.next();
                self.state.vault_view = view;
                self.state.content_cursor = 0;
            }
            KeyCode::Char('K') => {
                let view = self.state.vault_view.prev();
                self.state.vault_view = view;
                self.state.content_cursor = 0;
            }
            _ => {}
        }
        Ok(false)
    }

    fn do_lock(&mut self) {
        use zeroize::Zeroize;
        if let Some(mut v) = self.state.vault.take() {
            for key in &mut v.slot_keys {
                key.zeroize();
            }
        }
        self.state.passphrase_input.zeroize();
    }
}

fn handle_text_input(field: &mut String, key: KeyCode) {
    match key {
        KeyCode::Char(c) => field.push(c),
        KeyCode::Backspace => { field.pop(); }
        _ => {}
    }
}

fn clear_clipboard() -> Result<()> {
    if let Ok(mut ctx) = arboard::Clipboard::new() {
        let _ = ctx.set_text("");
    }
    Ok(())
}

pub fn run(device: Option<PathBuf>) -> Result<()> {
    enable_raw_mode()?;
    let mut stdout = std::io::stdout();
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    let mut app = App::new(device);
    let result = app.run_loop(&mut terminal);

    // Always restore terminal even if app returned error.
    disable_raw_mode()?;
    execute!(
        terminal.backend_mut(),
        LeaveAlternateScreen,
        DisableMouseCapture
    )?;
    terminal.show_cursor()?;

    result
}

// Re-export for use in try_unlock.
use anyhow::Context;
