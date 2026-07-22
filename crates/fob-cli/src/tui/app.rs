use std::path::PathBuf;
use std::time::{Duration, Instant};

use anyhow::Result;
use crossterm::{
    event::{self, DisableBracketedPaste, EnableBracketedPaste, Event, KeyCode, KeyModifiers},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{backend::CrosstermBackend, Terminal};

use super::{
    screens,
    state::{
        AppState, DashboardState, DashboardTab, Modal, NoteForm, PasswordForm, Screen, SshForm,
        TotpForm, UnlockState, WizardStep,
    },
};
use crate::device;
use crate::fs_util::atomic_write;

const TICK_RATE: Duration = Duration::from_millis(60);
const BOOT_TICKS: u8 = 12;

pub struct App {
    pub state: AppState,
}

impl App {
    pub fn new(device: Option<PathBuf>) -> Self {
        let devices = device::enumerate_usb_devices();
        let mut state = AppState::new(devices);

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

    pub fn run_loop(
        &mut self,
        terminal: &mut Terminal<CrosstermBackend<std::io::Stdout>>,
    ) -> Result<()> {
        let mut last_tick = Instant::now();

        loop {
            terminal.draw(|frame| screens::render(frame, &self.state))?;

            let timeout = TICK_RATE
                .checked_sub(last_tick.elapsed())
                .unwrap_or(Duration::ZERO);

            if event::poll(timeout)? {
                match event::read()? {
                    Event::Key(key) => {
                        if self.handle_key(key.code, key.modifiers)? {
                            return Ok(());
                        }
                    }
                    Event::Paste(text) => self.handle_paste(&text),
                    _ => {}
                }
            }

            if last_tick.elapsed() >= TICK_RATE {
                self.tick();
                last_tick = Instant::now();
            }
        }
    }

    fn tick(&mut self) {
        self.state.tick = self.state.tick.wrapping_add(1);

        if self.state.screen == Screen::Boot {
            self.state.boot_tick = self.state.boot_tick.saturating_add(1);
            if self.state.boot_tick >= BOOT_TICKS {
                // Always land on the picker, even with zero drives — it has
                // its own empty state with a rescan key, so plugging in a
                // drive after launch doesn't require restarting the app.
                self.state.screen = Screen::DevicePicker;
            }
        }

        if self.state.wizard.mismatch_flash > 0 {
            self.state.wizard.mismatch_flash -= 1;
        }

        // Poll background format thread.
        if self.state.screen == Screen::Formatting {
            if let Some(rx) = &self.state.format_rx {
                if let Ok(result) = rx.try_recv() {
                    self.state.format_rx = None;
                    match result {
                        Ok(()) => {
                            self.state.devices = crate::device::enumerate_usb_devices();
                            if let Some(idx) =
                                self.state.devices.iter().position(|d| d.name == "FOB")
                            {
                                self.state.selected_device = idx;
                            }
                            self.state.screen = Screen::SetupWizard(WizardStep::Master);
                            self.state.wizard.field = 0;
                            self.state.wizard.cursor = 0;
                        }
                        Err(e) => {
                            self.state.screen = Screen::Error(e.to_string());
                        }
                    }
                }
            }
        }

        // Auto-clear the clipboard 30s after a copy.
        if let Some(dash) = self.state.dashboard.as_mut() {
            if let Some((text, clear_at)) = &dash.clipboard {
                if std::time::Instant::now() >= *clear_at {
                    let _ = crate::clipboard::clear_if_unchanged(text);
                    dash.clipboard = None;
                }
            }
        }
    }

    fn handle_key(&mut self, key: KeyCode, mods: KeyModifiers) -> Result<bool> {
        if key == KeyCode::Char('c') && mods.contains(KeyModifiers::CONTROL) {
            return Ok(true);
        }

        let screen = self.state.screen.clone();
        match screen {
            Screen::Boot => {
                self.state.boot_tick = BOOT_TICKS;
            }
            Screen::Formatting => {
                // Block all input while the format thread is running.
            }
            Screen::DevicePicker => {
                self.handle_device_picker(key)?;
            }
            Screen::SetupWizard(step) => {
                if self.handle_wizard(key, step)? {
                    return Ok(true);
                }
            }
            Screen::Unlock => {
                self.handle_unlock(key);
            }
            Screen::Dashboard => {
                self.handle_dashboard(key)?;
            }
            Screen::Done => {
                return Ok(true);
            }
            Screen::Error(_) => {
                if matches!(key, KeyCode::Char('q') | KeyCode::Esc | KeyCode::Enter) {
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
                // q exits from device picker
                self.state.screen = Screen::Error("Setup cancelled.".into());
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
                    let next = if dev.has_fob_vault {
                        WizardStep::ExistingVault
                    } else {
                        WizardStep::ConfirmWipe
                    };
                    self.state.screen = Screen::SetupWizard(next);
                    self.state.wizard.field = 0;
                    self.state.wizard.cursor = 0;
                }
            }
            // '1'..='9' only — quick-select maps to devices 1-9, and there's
            // no 10th-device meaning for '0' to map to. Excluding it here
            // (rather than matching is_ascii_digit(), which also accepts
            // '0') avoids an unsigned underflow computing `'0' - '1'`.
            KeyCode::Char(c @ '1'..='9') => {
                let idx = (c as usize) - ('1' as usize);
                if idx < n {
                    self.state.selected_device = idx;
                }
            }
            KeyCode::Char('r') | KeyCode::Char('R') => {
                self.state.devices = crate::device::enumerate_usb_devices();
                if self.state.selected_device >= self.state.devices.len() {
                    self.state.selected_device = self.state.devices.len().saturating_sub(1);
                }
            }
            _ => {}
        }
        Ok(())
    }

    fn handle_wizard(&mut self, key: KeyCode, step: WizardStep) -> Result<bool> {
        match step {
            WizardStep::ExistingVault => match key {
                KeyCode::Char('o') | KeyCode::Char('O') => {
                    self.state.unlock = UnlockState::default();
                    self.state.screen = Screen::Unlock;
                }
                KeyCode::Char('u') | KeyCode::Char('U') => {
                    self.state.update_mode = true;
                    match self.run_vault_update() {
                        Ok(()) => self.state.screen = Screen::Done,
                        Err(e) => self.state.screen = Screen::Error(e.to_string()),
                    }
                }
                KeyCode::Char('f') | KeyCode::Char('F') => {
                    self.state.update_mode = false;
                    self.state.screen = Screen::SetupWizard(WizardStep::ConfirmWipe);
                }
                KeyCode::Esc => {
                    self.state.screen = Screen::DevicePicker;
                }
                _ => {}
            },

            WizardStep::ConfirmWipe => match key {
                KeyCode::Char('y') | KeyCode::Char('Y') => {
                    if let Some(dev) = self.state.devices.get(self.state.selected_device).cloned() {
                        let (tx, rx) = std::sync::mpsc::channel();
                        std::thread::spawn(move || {
                            tx.send(crate::device::format_device(&dev)).ok();
                        });
                        self.state.format_rx = Some(rx);
                        self.state.screen = Screen::Formatting;
                    }
                }
                KeyCode::Char('n') | KeyCode::Char('N') | KeyCode::Esc => {
                    self.state.screen = Screen::DevicePicker;
                }
                _ => {}
            },

            WizardStep::Master => match key {
                KeyCode::Tab => {
                    self.state.wizard.field = (self.state.wizard.field + 1) % 2;
                    self.state.wizard.cursor = match self.state.wizard.field {
                        0 => self.state.wizard.main_pass.chars().count(),
                        _ => self.state.wizard.main_pass_confirm.chars().count(),
                    };
                }
                KeyCode::Enter => {
                    if self.state.wizard.field == 0 {
                        self.state.wizard.field = 1;
                        self.state.wizard.cursor =
                            self.state.wizard.main_pass_confirm.chars().count();
                    } else if !self.state.wizard.main_pass.is_empty()
                        && self.state.wizard.main_pass == self.state.wizard.main_pass_confirm
                    {
                        self.state.screen = Screen::SetupWizard(WizardStep::Confirm);
                        self.state.wizard.field = 0;
                        self.state.wizard.cursor = 0;
                    } else {
                        self.state.wizard.mismatch_flash = 15;
                    }
                }
                KeyCode::Esc => {
                    use zeroize::Zeroize;
                    self.state.wizard.main_pass.zeroize();
                    self.state.wizard.main_pass_confirm.zeroize();
                    self.state.screen = Screen::SetupWizard(WizardStep::ConfirmWipe);
                }
                _ => match self.state.wizard.field {
                    0 => handle_text_input(
                        &mut self.state.wizard.main_pass,
                        &mut self.state.wizard.cursor,
                        key,
                    ),
                    1 => handle_text_input(
                        &mut self.state.wizard.main_pass_confirm,
                        &mut self.state.wizard.cursor,
                        key,
                    ),
                    _ => {}
                },
            },

            WizardStep::Confirm => match key {
                KeyCode::Enter => match self.run_vault_init() {
                    Ok(()) => self.state.screen = Screen::Done,
                    Err(e) => self.state.screen = Screen::Error(e.to_string()),
                },
                KeyCode::Esc => {
                    self.state.screen = Screen::SetupWizard(WizardStep::Master);
                }
                _ => {}
            },
        }
        Ok(false)
    }

    fn run_vault_init(&mut self) -> Result<()> {
        use fob_core::format::DEFAULT_VAULT_SIZE;
        use fob_core::vault::VaultInitParams;
        use zeroize::Zeroize;

        let dev = self
            .state
            .devices
            .get(self.state.selected_device)
            .ok_or_else(|| anyhow::anyhow!("No device selected"))?
            .clone();

        let vault_path = dev.path.join("vault.fob");

        let vault_bytes = fob_core::vault::init_vault(VaultInitParams::new(
            self.state.wizard.main_pass.as_bytes().to_vec(),
            DEFAULT_VAULT_SIZE,
        ))?;

        atomic_write(&vault_path, &vault_bytes)?;
        crate::cli::write_web_ui(&dev.path)?;

        self.state.wizard.main_pass.zeroize();
        self.state.wizard.main_pass_confirm.zeroize();

        Ok(())
    }

    fn run_vault_update(&mut self) -> Result<()> {
        let dev = self
            .state
            .devices
            .get(self.state.selected_device)
            .ok_or_else(|| anyhow::anyhow!("No device selected"))?
            .clone();
        crate::cli::write_web_ui(&dev.path)?;
        Ok(())
    }

    /// Route a bracketed-paste event to whichever text field is currently
    /// active, inserting the whole pasted string at the cursor in one go.
    ///
    /// Without this, a multi-line paste (an SSH private key, a long note)
    /// would arrive as individual `KeyCode::Enter` presses per embedded
    /// newline — and Enter means "next field / save" in every form here, so
    /// the paste would silently truncate at the first line break instead of
    /// landing intact.
    fn handle_paste(&mut self, text: &str) {
        let text = text.replace('\r', "");
        match &self.state.screen {
            Screen::SetupWizard(WizardStep::Master) => match self.state.wizard.field {
                0 => insert_str_at_cursor(
                    &mut self.state.wizard.main_pass,
                    &mut self.state.wizard.cursor,
                    &text,
                ),
                _ => insert_str_at_cursor(
                    &mut self.state.wizard.main_pass_confirm,
                    &mut self.state.wizard.cursor,
                    &text,
                ),
            },
            Screen::Unlock => insert_str_at_cursor(
                &mut self.state.unlock.passphrase,
                &mut self.state.unlock.cursor,
                &text,
            ),
            Screen::Dashboard => {
                if let Some(dash) = self.state.dashboard.as_mut() {
                    match &mut dash.modal {
                        Modal::AddPassword(form) => {
                            let field = match form.field {
                                0 => &mut form.name,
                                1 => &mut form.username,
                                _ => &mut form.password,
                            };
                            insert_str_at_cursor(field, &mut form.cursor, &text);
                        }
                        Modal::AddNote(form) => {
                            let field = match form.field {
                                0 => &mut form.title,
                                _ => &mut form.body,
                            };
                            insert_str_at_cursor(field, &mut form.cursor, &text);
                        }
                        Modal::AddTotp(form) => {
                            let field = match form.field {
                                0 => &mut form.issuer,
                                1 => &mut form.account,
                                _ => &mut form.secret,
                            };
                            insert_str_at_cursor(field, &mut form.cursor, &text);
                        }
                        Modal::AddSsh(form) => {
                            let field = match form.field {
                                0 => &mut form.name,
                                1 => &mut form.public_key,
                                _ => &mut form.private_key,
                            };
                            insert_str_at_cursor(field, &mut form.cursor, &text);
                        }
                        Modal::None | Modal::ConfirmDelete => {}
                    }
                }
            }
            _ => {}
        }
    }

    fn handle_unlock(&mut self, key: KeyCode) {
        match key {
            KeyCode::Enter => {
                if !self.state.unlock.passphrase.is_empty() {
                    self.try_unlock();
                }
            }
            KeyCode::Esc => {
                self.state.unlock = UnlockState::default();
                self.state.screen = Screen::SetupWizard(WizardStep::ExistingVault);
            }
            _ => handle_text_input(
                &mut self.state.unlock.passphrase,
                &mut self.state.unlock.cursor,
                key,
            ),
        }
    }

    /// Attempt to unlock the vault on the selected device with the entered
    /// passphrase. Wrong passphrase and duress passphrase are deliberately
    /// indistinguishable here — both just show "Incorrect passphrase."
    fn try_unlock(&mut self) {
        use zeroize::Zeroize;

        let dev = match self.state.devices.get(self.state.selected_device) {
            Some(d) => d.clone(),
            None => {
                self.state.unlock.error = Some("No device selected.".into());
                return;
            }
        };
        let vault_path = dev.path.join("vault.fob");

        if let Ok(meta) = std::fs::metadata(&vault_path) {
            if meta.len() > fob_core::format::MAX_VAULT_SIZE as u64 {
                self.state.unlock.error = Some(format!(
                    "vault.fob is {} bytes, exceeding the maximum of {} bytes",
                    meta.len(),
                    fob_core::format::MAX_VAULT_SIZE
                ));
                return;
            }
        }

        let bytes = match std::fs::read(&vault_path) {
            Ok(b) => b,
            Err(e) => {
                self.state.unlock.error = Some(format!("Could not read vault: {e}"));
                return;
            }
        };

        let mut passphrase = self.state.unlock.passphrase.clone();
        match fob_core::vault::unlock_vault_with_duress_wipe(
            &bytes,
            passphrase.as_bytes(),
            &vault_path,
        ) {
            Ok((slot, blob)) => {
                let vault_file = match fob_core::vault::VaultFile::from_bytes(bytes) {
                    Ok(vf) => vf,
                    Err(e) => {
                        self.state.unlock.error = Some(e.to_string());
                        return;
                    }
                };
                self.state.dashboard = Some(DashboardState {
                    vault_path,
                    vault_file,
                    slot,
                    passphrase,
                    blob,
                    tab: DashboardTab::Passwords,
                    selected: 0,
                    modal: Modal::None,
                    reveal: false,
                    status: None,
                    ssh_agent: None,
                    clipboard: None,
                });
                self.state.dashboard.as_mut().unwrap().sync_ssh_agent();
                self.state.unlock = UnlockState::default();
                self.state.screen = Screen::Dashboard;
            }
            Err(_) => {
                // `passphrase` (the clone made above for this attempt) never
                // moved anywhere on this path — zeroize it too, not just the
                // original field, or the just-typed passphrase is left
                // sitting in a second, unwiped heap allocation every time
                // someone mistypes it (a routine, frequent event).
                passphrase.zeroize();
                self.state.unlock.passphrase.zeroize();
                self.state.unlock.passphrase.clear();
                self.state.unlock.error = Some("Incorrect passphrase.".into());
            }
        }
    }

    fn handle_dashboard(&mut self, key: KeyCode) -> Result<()> {
        let Some(dash) = self.state.dashboard.as_ref() else {
            return Ok(());
        };

        if !matches!(dash.modal, Modal::None) {
            self.handle_dashboard_modal(key);
            return Ok(());
        }

        let dash = self.state.dashboard.as_mut().unwrap();
        match key {
            KeyCode::Char('q') | KeyCode::Esc => {
                self.state.dashboard = None;
                self.state.screen = Screen::SetupWizard(WizardStep::ExistingVault);
            }
            KeyCode::Tab => {
                let idx = (dash.tab.index() + 1) % DashboardTab::ALL.len();
                dash.tab = DashboardTab::ALL[idx];
                dash.selected = 0;
                dash.reveal = false;
            }
            KeyCode::BackTab => {
                let n = DashboardTab::ALL.len();
                let idx = (dash.tab.index() + n - 1) % n;
                dash.tab = DashboardTab::ALL[idx];
                dash.selected = 0;
                dash.reveal = false;
            }
            KeyCode::Char(c @ '1'..='4') => {
                let idx = (c as usize) - ('1' as usize);
                dash.tab = DashboardTab::ALL[idx];
                dash.selected = 0;
                dash.reveal = false;
            }
            KeyCode::Up | KeyCode::Char('k') => {
                if dash.selected > 0 {
                    dash.selected -= 1;
                    dash.reveal = false;
                }
            }
            KeyCode::Down | KeyCode::Char('j') => {
                if dash.selected + 1 < dash.tab_len() {
                    dash.selected += 1;
                    dash.reveal = false;
                }
            }
            KeyCode::Char('a') => {
                dash.modal = match dash.tab {
                    DashboardTab::Passwords => Modal::AddPassword(PasswordForm::default()),
                    DashboardTab::Totp => Modal::AddTotp(TotpForm::default()),
                    DashboardTab::Ssh => Modal::AddSsh(SshForm::default()),
                    DashboardTab::Notes => Modal::AddNote(NoteForm::default()),
                };
                dash.status = None;
            }
            KeyCode::Char('d') => {
                if dash.tab_len() > 0 {
                    dash.modal = Modal::ConfirmDelete;
                }
            }
            KeyCode::Char('e') => {
                let idx = dash.selected;
                dash.modal = match dash.tab {
                    DashboardTab::Passwords => dash
                        .blob
                        .passwords
                        .get(idx)
                        .map(|e| Modal::AddPassword(PasswordForm::for_edit(idx, e))),
                    DashboardTab::Totp => dash
                        .blob
                        .totps
                        .get(idx)
                        .map(|e| Modal::AddTotp(TotpForm::for_edit(idx, e))),
                    DashboardTab::Ssh => dash
                        .blob
                        .ssh_keys
                        .get(idx)
                        .map(|e| Modal::AddSsh(SshForm::for_edit(idx, e))),
                    DashboardTab::Notes => dash
                        .blob
                        .notes
                        .get(idx)
                        .map(|e| Modal::AddNote(NoteForm::for_edit(idx, e))),
                }
                .unwrap_or(Modal::None);
                dash.status = None;
            }
            KeyCode::Char('r') | KeyCode::Enter => {
                dash.reveal = !dash.reveal;
            }
            KeyCode::Char('c') => {
                if let Some(text) = dash.copyable_text() {
                    match crate::clipboard::copy(&text) {
                        Ok(()) => {
                            dash.clipboard = Some((
                                text,
                                std::time::Instant::now() + crate::clipboard::CLEAR_AFTER,
                            ));
                            dash.status = Some("Copied — clipboard clears in 30s.".into());
                        }
                        Err(e) => dash.status = Some(format!("Copy failed: {e}")),
                    }
                }
            }
            _ => {}
        }
        Ok(())
    }

    fn handle_dashboard_modal(&mut self, key: KeyCode) {
        let Some(dash) = self.state.dashboard.as_mut() else {
            return;
        };

        if key == KeyCode::Esc {
            dash.modal = Modal::None;
            return;
        }

        match &mut dash.modal {
            Modal::None => {}

            Modal::ConfirmDelete => {
                if matches!(key, KeyCode::Char('y') | KeyCode::Char('Y')) {
                    let idx = dash.selected;
                    match dash.tab {
                        DashboardTab::Passwords if idx < dash.blob.passwords.len() => {
                            dash.blob.passwords.remove(idx);
                        }
                        DashboardTab::Totp if idx < dash.blob.totps.len() => {
                            dash.blob.totps.remove(idx);
                        }
                        DashboardTab::Ssh if idx < dash.blob.ssh_keys.len() => {
                            dash.blob.ssh_keys.remove(idx);
                        }
                        DashboardTab::Notes if idx < dash.blob.notes.len() => {
                            dash.blob.notes.remove(idx);
                        }
                        _ => {}
                    }
                    // Keep the highlight on the entry that slid into this
                    // row, rather than always jumping back one row
                    // regardless of which index was actually removed —
                    // only clamp when the deleted row was the last one.
                    let new_len = dash.tab_len();
                    if dash.selected >= new_len {
                        dash.selected = new_len.saturating_sub(1);
                    }
                    dash.modal = Modal::None;
                    dash.status = Some(match dash.save() {
                        Ok(()) => "Deleted.".into(),
                        Err(e) => format!("Save failed: {e}"),
                    });
                    if dash.tab == DashboardTab::Ssh {
                        dash.sync_ssh_agent();
                    }
                } else if matches!(key, KeyCode::Char('n') | KeyCode::Char('N')) {
                    dash.modal = Modal::None;
                }
            }

            Modal::AddPassword(form) => match key {
                KeyCode::Tab => {
                    form.field = (form.field + 1) % 3;
                    form.cursor = match form.field {
                        0 => form.name.chars().count(),
                        1 => form.username.chars().count(),
                        _ => form.password.chars().count(),
                    };
                }
                KeyCode::Enter => {
                    if form.field < 2 {
                        form.field += 1;
                        form.cursor = match form.field {
                            1 => form.username.chars().count(),
                            _ => form.password.chars().count(),
                        };
                    } else if !form.name.is_empty() {
                        if let Some(existing) = form
                            .editing
                            .and_then(|idx| dash.blob.passwords.get_mut(idx))
                        {
                            existing.name = form.name.clone();
                            existing.username = form.username.clone();
                            existing.password =
                                fob_core::types::SecretString::new(form.password.clone());
                            existing.modified = fob_core::vault::unix_now();
                        } else {
                            let entry = fob_core::types::PasswordEntry::new(
                                form.name.clone(),
                                form.username.clone(),
                                form.password.clone(),
                            );
                            dash.blob.passwords.push(entry);
                        }
                        dash.modal = Modal::None;
                        dash.status = Some(match dash.save() {
                            Ok(()) => "Saved.".into(),
                            Err(e) => format!("Save failed: {e}"),
                        });
                    }
                }
                KeyCode::F(2) if form.field == 2 => {
                    if let Ok(pw) = fob_core::generator::generate_password(20) {
                        form.cursor = pw.chars().count();
                        form.password = pw;
                    }
                }
                _ => {
                    let field = match form.field {
                        0 => &mut form.name,
                        1 => &mut form.username,
                        _ => &mut form.password,
                    };
                    handle_text_input(field, &mut form.cursor, key);
                }
            },

            Modal::AddNote(form) => match key {
                KeyCode::Tab => {
                    form.field = (form.field + 1) % 2;
                    form.cursor = match form.field {
                        0 => form.title.chars().count(),
                        _ => form.body.chars().count(),
                    };
                }
                KeyCode::Enter => {
                    if form.field < 1 {
                        form.field += 1;
                        form.cursor = form.body.chars().count();
                    } else if !form.title.is_empty() {
                        if let Some(existing) =
                            form.editing.and_then(|idx| dash.blob.notes.get_mut(idx))
                        {
                            existing.title = form.title.clone();
                            existing.body = fob_core::types::SecretString::new(form.body.clone());
                            existing.modified = fob_core::vault::unix_now();
                        } else {
                            let entry = fob_core::types::NoteEntry::new(
                                form.title.clone(),
                                form.body.clone(),
                            );
                            dash.blob.notes.push(entry);
                        }
                        dash.modal = Modal::None;
                        dash.status = Some(match dash.save() {
                            Ok(()) => "Saved.".into(),
                            Err(e) => format!("Save failed: {e}"),
                        });
                    }
                }
                _ => {
                    let field = match form.field {
                        0 => &mut form.title,
                        _ => &mut form.body,
                    };
                    handle_text_input(field, &mut form.cursor, key);
                }
            },

            Modal::AddTotp(form) => match key {
                KeyCode::Tab => {
                    form.field = (form.field + 1) % 3;
                    form.cursor = match form.field {
                        0 => form.issuer.chars().count(),
                        1 => form.account.chars().count(),
                        _ => form.secret.chars().count(),
                    };
                }
                KeyCode::Enter => {
                    if form.field < 2 {
                        form.field += 1;
                        form.cursor = match form.field {
                            1 => form.account.chars().count(),
                            _ => form.secret.chars().count(),
                        };
                    } else if !form.issuer.is_empty() && !form.secret.is_empty() {
                        match fob_core::totp::decode_secret(&form.secret) {
                            Ok(secret_bytes) => {
                                if let Some(existing) =
                                    form.editing.and_then(|idx| dash.blob.totps.get_mut(idx))
                                {
                                    existing.issuer = form.issuer.clone();
                                    existing.account = form.account.clone();
                                    existing.secret = fob_core::types::SecretBytes(secret_bytes);
                                } else {
                                    let entry = fob_core::types::TotpEntry::new(
                                        form.issuer.clone(),
                                        form.account.clone(),
                                        secret_bytes,
                                    );
                                    dash.blob.totps.push(entry);
                                }
                                dash.modal = Modal::None;
                                dash.status = Some(match dash.save() {
                                    Ok(()) => "Saved.".into(),
                                    Err(e) => format!("Save failed: {e}"),
                                });
                            }
                            Err(e) => dash.status = Some(format!("Invalid secret: {e}")),
                        }
                    }
                }
                _ => {
                    let field = match form.field {
                        0 => &mut form.issuer,
                        1 => &mut form.account,
                        _ => &mut form.secret,
                    };
                    handle_text_input(field, &mut form.cursor, key);
                }
            },

            Modal::AddSsh(form) => match key {
                KeyCode::Tab => {
                    form.field = (form.field + 1) % 3;
                    form.cursor = match form.field {
                        0 => form.name.chars().count(),
                        1 => form.public_key.chars().count(),
                        _ => form.private_key.chars().count(),
                    };
                }
                KeyCode::Enter => {
                    if form.field < 2 {
                        form.field += 1;
                        form.cursor = match form.field {
                            1 => form.public_key.chars().count(),
                            _ => form.private_key.chars().count(),
                        };
                    } else if !form.name.is_empty() && !form.public_key.is_empty() {
                        if let Some(idx) = form.editing {
                            match fob_core::sshkey::fingerprint(&form.public_key) {
                                Ok(fingerprint) => {
                                    let algorithm = fob_core::sshkey::algorithm(&form.public_key);
                                    if let Some(existing) = dash.blob.ssh_keys.get_mut(idx) {
                                        existing.name = form.name.clone();
                                        existing.public_key = form.public_key.clone();
                                        existing.private_key = fob_core::types::SecretString::new(
                                            form.private_key.clone(),
                                        );
                                        existing.fingerprint = fingerprint;
                                        existing.algorithm = algorithm;
                                    }
                                    dash.modal = Modal::None;
                                    dash.status = Some(match dash.save() {
                                        Ok(()) => "Saved.".into(),
                                        Err(e) => format!("Save failed: {e}"),
                                    });
                                    dash.sync_ssh_agent();
                                }
                                Err(e) => dash.status = Some(format!("Invalid public key: {e}")),
                            }
                        } else {
                            match fob_core::types::SshKeyEntry::new(
                                form.name.clone(),
                                form.public_key.clone(),
                                form.private_key.clone(),
                            ) {
                                Ok(entry) => {
                                    dash.blob.ssh_keys.push(entry);
                                    dash.modal = Modal::None;
                                    dash.status = Some(match dash.save() {
                                        Ok(()) => "Saved.".into(),
                                        Err(e) => format!("Save failed: {e}"),
                                    });
                                    dash.sync_ssh_agent();
                                }
                                Err(e) => dash.status = Some(format!("Invalid public key: {e}")),
                            }
                        }
                    }
                }
                _ => {
                    let field = match form.field {
                        0 => &mut form.name,
                        1 => &mut form.public_key,
                        _ => &mut form.private_key,
                    };
                    handle_text_input(field, &mut form.cursor, key);
                }
            },
        }
    }
}

/// Byte offset of the `char_idx`-th character in `s` (or `s.len()` if
/// `char_idx` is at or past the end) — every mutation below needs this since
/// `String::insert`/`remove` take byte offsets but the cursor tracks chars.
fn char_to_byte_idx(s: &str, char_idx: usize) -> usize {
    s.char_indices()
        .nth(char_idx)
        .map(|(b, _)| b)
        .unwrap_or(s.len())
}

/// Insert a whole pasted string at the cursor in one go (as opposed to
/// `handle_text_input`, which handles one key at a time).
fn insert_str_at_cursor(field: &mut String, cursor: &mut usize, text: &str) {
    let byte_idx = char_to_byte_idx(field, *cursor);
    field.insert_str(byte_idx, text);
    *cursor += text.chars().count();
}

fn handle_text_input(field: &mut String, cursor: &mut usize, key: KeyCode) {
    match key {
        KeyCode::Char(c) => {
            let byte_idx = char_to_byte_idx(field, *cursor);
            field.insert(byte_idx, c);
            *cursor += 1;
        }
        KeyCode::Backspace => {
            if *cursor > 0 {
                *cursor -= 1;
                let byte_idx = char_to_byte_idx(field, *cursor);
                field.remove(byte_idx);
            }
        }
        KeyCode::Delete => {
            if *cursor < field.chars().count() {
                let byte_idx = char_to_byte_idx(field, *cursor);
                field.remove(byte_idx);
            }
        }
        KeyCode::Left => *cursor = cursor.saturating_sub(1),
        KeyCode::Right => *cursor = (*cursor + 1).min(field.chars().count()),
        KeyCode::Home => *cursor = 0,
        KeyCode::End => *cursor = field.chars().count(),
        _ => {}
    }
}

pub fn run(device: Option<PathBuf>) -> Result<()> {
    enable_raw_mode()?;
    let mut stdout = std::io::stdout();
    execute!(stdout, EnterAlternateScreen, EnableBracketedPaste)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    let mut app = App::new(device);
    let result = app.run_loop(&mut terminal);

    disable_raw_mode()?;
    execute!(
        terminal.backend_mut(),
        DisableBracketedPaste,
        LeaveAlternateScreen
    )?;
    terminal.show_cursor()?;

    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tui::state::AppState;
    use fob_core::types::PasswordEntry;
    use fob_core::vault::{unlock_vault, VaultFile, VaultInitParams};

    fn app_with_dashboard() -> (App, std::path::PathBuf, tempfile_dir::TempDir) {
        let dir = tempfile_dir::TempDir::new();
        let vault_path = dir.path().join("vault.fob");

        let bytes = fob_core::vault::init_vault(VaultInitParams {
            main_passphrase: b"test-pass".to_vec(),
            decoy_passphrase: None,
            duress_passphrase: None,
            vault_size: 256 * 1024,
            decoy_blob: None,
            kdf_iterations: 1000,
        })
        .unwrap();
        std::fs::write(&vault_path, &bytes).unwrap();

        let (slot, mut blob) = unlock_vault(&bytes, b"test-pass").unwrap();
        blob.passwords
            .push(PasswordEntry::new("GitHub", "alice", "hunter1"));
        let vault_file = VaultFile::from_bytes(bytes).unwrap();

        let dash = DashboardState {
            vault_path: vault_path.clone(),
            vault_file,
            slot,
            passphrase: "test-pass".to_string(),
            blob,
            tab: DashboardTab::Passwords,
            selected: 0,
            modal: Modal::None,
            reveal: false,
            status: None,
            ssh_agent: None,
            clipboard: None,
        };
        // Persist the entry so the dashboard's on-disk state matches its in-memory state.
        let mut dash = dash;
        dash.save().unwrap();

        let mut state = AppState::new(Vec::new());
        state.screen = Screen::Dashboard;
        state.dashboard = Some(dash);

        (App { state }, vault_path, dir)
    }

    fn press(app: &mut App, key: KeyCode) {
        app.handle_key(key, KeyModifiers::NONE).unwrap();
    }

    #[test]
    fn edit_updates_existing_entry_without_duplicating_it() {
        let (mut app, vault_path, _dir) = app_with_dashboard();
        let original_id = app.state.dashboard.as_ref().unwrap().blob.passwords[0].id;

        press(&mut app, KeyCode::Char('e')); // open edit modal, prefilled
        press(&mut app, KeyCode::Tab); // name -> username
        press(&mut app, KeyCode::Tab); // username -> password field
        for _ in 0.."hunter1".len() {
            press(&mut app, KeyCode::Backspace);
        }
        for c in "hunter2".chars() {
            press(&mut app, KeyCode::Char(c));
        }
        press(&mut app, KeyCode::Enter); // submit

        let dash = app.state.dashboard.as_ref().unwrap();
        assert!(matches!(dash.modal, Modal::None));
        assert_eq!(
            dash.blob.passwords.len(),
            1,
            "edit must not create a duplicate entry"
        );
        assert_eq!(
            dash.blob.passwords[0].id, original_id,
            "editing must preserve the entry's id"
        );
        assert_eq!(
            dash.blob.passwords[0].name, "GitHub",
            "untouched field must be preserved"
        );
        assert_eq!(dash.blob.passwords[0].password.expose(), "hunter2");

        // And the edit actually reached disk, not just in-memory state.
        let reloaded_bytes = std::fs::read(&vault_path).unwrap();
        let (_, reloaded) = unlock_vault(&reloaded_bytes, b"test-pass").unwrap();
        assert_eq!(reloaded.passwords.len(), 1);
        assert_eq!(reloaded.passwords[0].password.expose(), "hunter2");
    }

    #[test]
    fn escape_cancels_edit_without_changing_the_entry() {
        let (mut app, _vault_path, _dir) = app_with_dashboard();

        press(&mut app, KeyCode::Char('e'));
        for c in "should-not-be-saved".chars() {
            press(&mut app, KeyCode::Char(c));
        }
        press(&mut app, KeyCode::Esc);

        let dash = app.state.dashboard.as_ref().unwrap();
        assert!(matches!(dash.modal, Modal::None));
        assert_eq!(dash.blob.passwords[0].name, "GitHub");
    }

    #[test]
    fn delete_keeps_selection_on_the_entry_that_slid_into_this_row() {
        // Regression test: deleting index 1 out of [A,B,C,D] must land the
        // selection on C (which slides into row 1), not unconditionally
        // jump back to row 0 (A) regardless of which index was removed.
        let (mut app, _vault_path, _dir) = app_with_dashboard();
        let dash = app.state.dashboard.as_mut().unwrap();
        dash.blob.passwords.clear();
        for name in ["A", "B", "C", "D"] {
            dash.blob
                .passwords
                .push(PasswordEntry::new(name, "user", "pw"));
        }
        dash.selected = 1; // "B"

        press(&mut app, KeyCode::Char('d'));
        press(&mut app, KeyCode::Char('y'));

        let dash = app.state.dashboard.as_ref().unwrap();
        assert_eq!(dash.blob.passwords.len(), 3);
        assert_eq!(
            dash.blob.passwords[dash.selected].name, "C",
            "selection should follow the entry that slid into the deleted row"
        );
    }

    #[test]
    fn deleting_the_last_entry_clamps_selection_to_the_new_last_entry() {
        let (mut app, _vault_path, _dir) = app_with_dashboard();
        let dash = app.state.dashboard.as_mut().unwrap();
        dash.blob.passwords.clear();
        for name in ["A", "B", "C"] {
            dash.blob
                .passwords
                .push(PasswordEntry::new(name, "user", "pw"));
        }
        dash.selected = 2; // "C", the last entry

        press(&mut app, KeyCode::Char('d'));
        press(&mut app, KeyCode::Char('y'));

        let dash = app.state.dashboard.as_ref().unwrap();
        assert_eq!(dash.blob.passwords.len(), 2);
        assert_eq!(dash.selected, 1);
        assert_eq!(dash.blob.passwords[dash.selected].name, "B");
    }

    #[test]
    fn zero_digit_in_device_picker_does_not_panic() {
        // Regression test: '0' is_ascii_digit() but the quick-select maps
        // '1'..='9' to indices 0..=8 — computing ('0' as usize) - ('1' as
        // usize) used to underflow-panic in debug builds.
        let mut state = AppState::new(Vec::new());
        state.screen = Screen::DevicePicker;
        let mut app = App { state };
        press(&mut app, KeyCode::Char('0'));
        assert_eq!(app.state.selected_device, 0);
    }

    #[test]
    fn text_input_inserts_at_cursor_not_just_at_end() {
        let mut field = "helloworld".to_string();
        let mut cursor = 5; // between "hello" and "world"
        handle_text_input(&mut field, &mut cursor, KeyCode::Char(' '));
        assert_eq!(field, "hello world");
        assert_eq!(cursor, 6);
    }

    #[test]
    fn text_input_backspace_removes_char_before_cursor() {
        let mut field = "hello world".to_string();
        let mut cursor = 6; // right after the space
        handle_text_input(&mut field, &mut cursor, KeyCode::Backspace);
        assert_eq!(field, "helloworld");
        assert_eq!(cursor, 5);
    }

    #[test]
    fn text_input_backspace_at_start_is_a_no_op() {
        let mut field = "hello".to_string();
        let mut cursor = 0;
        handle_text_input(&mut field, &mut cursor, KeyCode::Backspace);
        assert_eq!(field, "hello");
        assert_eq!(cursor, 0);
    }

    #[test]
    fn text_input_delete_removes_char_at_cursor() {
        let mut field = "hello".to_string();
        let mut cursor = 0;
        handle_text_input(&mut field, &mut cursor, KeyCode::Delete);
        assert_eq!(field, "ello");
        assert_eq!(cursor, 0);
    }

    #[test]
    fn text_input_delete_at_end_is_a_no_op() {
        let mut field = "hello".to_string();
        let mut cursor = 5;
        handle_text_input(&mut field, &mut cursor, KeyCode::Delete);
        assert_eq!(field, "hello");
        assert_eq!(cursor, 5);
    }

    #[test]
    fn text_input_left_right_home_end_move_cursor_within_bounds() {
        let mut field = "abc".to_string();
        let mut cursor = 1;
        handle_text_input(&mut field, &mut cursor, KeyCode::Left);
        assert_eq!(cursor, 0);
        handle_text_input(&mut field, &mut cursor, KeyCode::Left);
        assert_eq!(cursor, 0, "cursor must not go below 0");

        handle_text_input(&mut field, &mut cursor, KeyCode::End);
        assert_eq!(cursor, 3);
        handle_text_input(&mut field, &mut cursor, KeyCode::Right);
        assert_eq!(cursor, 3, "cursor must not exceed the field length");

        handle_text_input(&mut field, &mut cursor, KeyCode::Home);
        assert_eq!(cursor, 0);
    }

    #[test]
    fn text_input_is_unicode_safe() {
        // "café" — 'é' is 2 bytes in UTF-8, so a naive byte-offset cursor
        // would panic (or silently corrupt the string) inserting/removing
        // right after it. The cursor tracks *chars*, not bytes.
        let mut field = "café".to_string();
        let mut cursor = 4; // after the 'é', i.e. at the end
        assert_eq!(cursor, field.chars().count());

        handle_text_input(&mut field, &mut cursor, KeyCode::Char('!'));
        assert_eq!(field, "café!");

        handle_text_input(&mut field, &mut cursor, KeyCode::Backspace);
        handle_text_input(&mut field, &mut cursor, KeyCode::Backspace);
        assert_eq!(field, "caf");
    }

    #[test]
    fn f2_generates_a_password_only_when_the_password_field_is_active() {
        let (mut app, _vault_path, _dir) = app_with_dashboard();
        press(&mut app, KeyCode::Char('a')); // open Add Password
        press(&mut app, KeyCode::F(2)); // field 0 (Name) is active — must be a no-op
        {
            let dash = app.state.dashboard.as_ref().unwrap();
            let Modal::AddPassword(form) = &dash.modal else {
                panic!("expected AddPassword modal");
            };
            assert_eq!(form.name, "", "F2 must not touch the Name field");
        }

        press(&mut app, KeyCode::Tab); // -> Username
        press(&mut app, KeyCode::Tab); // -> Password
        press(&mut app, KeyCode::F(2));

        let dash = app.state.dashboard.as_ref().unwrap();
        let Modal::AddPassword(form) = &dash.modal else {
            panic!("expected AddPassword modal");
        };
        assert_eq!(form.password.chars().count(), 20);
        assert_eq!(form.cursor, 20);
    }

    #[test]
    fn insert_str_at_cursor_splices_in_the_whole_string_at_once() {
        let mut field = "ab".to_string();
        let mut cursor = 1; // between 'a' and 'b'
        insert_str_at_cursor(&mut field, &mut cursor, "XYZ");
        assert_eq!(field, "aXYZb");
        assert_eq!(cursor, 4);
    }

    #[test]
    fn paste_into_multiline_field_preserves_every_line_without_truncating() {
        // Regression test: before bracketed paste was enabled, a multi-line
        // paste (e.g. a real SSH private key) arrived as individual key
        // events, and each embedded newline fired as a plain `Enter`
        // keypress — which every form here treats as "advance field / save",
        // silently truncating the paste at the first line break.
        let (mut app, _vault_path, _dir) = app_with_dashboard();
        let dash = app.state.dashboard.as_mut().unwrap();
        dash.modal = Modal::AddSsh(SshForm {
            name: String::new(),
            public_key: String::new(),
            private_key: String::new(),
            field: 2, // private_key
            cursor: 0,
            editing: None,
        });

        let pem =
            "-----BEGIN OPENSSH PRIVATE KEY-----\nline2\nline3\n-----END OPENSSH PRIVATE KEY-----";
        app.handle_paste(pem);

        let dash = app.state.dashboard.as_ref().unwrap();
        let Modal::AddSsh(form) = &dash.modal else {
            panic!("modal changed unexpectedly — paste must not trigger save/advance");
        };
        assert_eq!(form.private_key, pem);
        assert_eq!(form.cursor, pem.chars().count());
    }

    mod tempfile_dir {
        //! Minimal disposable-directory helper — avoids adding a `tempfile`
        //! dependency just for two tests.
        pub struct TempDir(std::path::PathBuf);

        impl TempDir {
            pub fn new() -> Self {
                let path = std::env::temp_dir().join(format!(
                    "fob-app-test-{}-{}",
                    std::process::id(),
                    std::time::SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH)
                        .unwrap()
                        .as_nanos()
                ));
                std::fs::create_dir_all(&path).unwrap();
                Self(path)
            }

            pub fn path(&self) -> &std::path::Path {
                &self.0
            }
        }

        impl Drop for TempDir {
            fn drop(&mut self) {
                let _ = std::fs::remove_dir_all(&self.0);
            }
        }
    }
}
