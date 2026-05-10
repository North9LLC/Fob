use std::path::PathBuf;
use std::time::{Duration, Instant};

use anyhow::Result;
use crossterm::{
    event::{self, Event, KeyCode, KeyModifiers},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{backend::CrosstermBackend, Terminal};

use super::{screens, state::{AppState, Screen, WizardStep}};
use crate::device;

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

    pub fn run_loop(&mut self, terminal: &mut Terminal<CrosstermBackend<std::io::Stdout>>) -> Result<()> {
        let mut last_tick = Instant::now();

        loop {
            terminal.draw(|frame| screens::render(frame, &self.state))?;

            let timeout = TICK_RATE.checked_sub(last_tick.elapsed()).unwrap_or(Duration::ZERO);

            if event::poll(timeout)? {
                if let Event::Key(key) = event::read()? {
                    if self.handle_key(key.code, key.modifiers)? {
                        return Ok(());
                    }
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
                self.state.screen = if self.state.devices.is_empty() {
                    Screen::Error(
                        "No USB drives detected.\nInsert a USB drive and restart fob.".into(),
                    )
                } else {
                    Screen::DevicePicker
                };
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
                            if let Some(idx) = self.state.devices.iter().position(|d| d.name == "FOB") {
                                self.state.selected_device = idx;
                            }
                            self.state.screen = Screen::SetupWizard(WizardStep::Master);
                            self.state.wizard.field = 0;
                        }
                        Err(e) => {
                            self.state.screen = Screen::Error(e.to_string());
                        }
                    }
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

    fn handle_wizard(&mut self, key: KeyCode, step: WizardStep) -> Result<bool> {
        match step {
            WizardStep::ExistingVault => match key {
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

            WizardStep::Master => {
                match key {
                    KeyCode::Tab => {
                        self.state.wizard.field = (self.state.wizard.field + 1) % 2;
                    }
                    KeyCode::Enter => {
                        if self.state.wizard.field == 0 {
                            self.state.wizard.field = 1;
                        } else if !self.state.wizard.main_pass.is_empty()
                            && self.state.wizard.main_pass == self.state.wizard.main_pass_confirm
                        {
                            self.state.screen = Screen::SetupWizard(WizardStep::Confirm);
                            self.state.wizard.field = 0;
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
                    _ => {
                        match self.state.wizard.field {
                            0 => handle_text_input(&mut self.state.wizard.main_pass, key),
                            1 => handle_text_input(&mut self.state.wizard.main_pass_confirm, key),
                            _ => {}
                        }
                    }
                }
            }

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
        use zeroize::Zeroize;

        let dev = self
            .state
            .devices
            .get(self.state.selected_device)
            .ok_or_else(|| anyhow::anyhow!("No device selected"))?
            .clone();

        let vault_path = dev.path.join("vault.fob");

        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();

        let blob_json = format!(
            r#"{{"v":1,"created":{now},"modified":{now},"passwords":[],"totp":[],"ssh_keys":[],"notes":[],"files":[]}}"#
        );

        let vault_bytes = fob_core::browser_vault::create(
            self.state.wizard.main_pass.as_bytes(),
            &blob_json,
        )?;

        std::fs::write(&vault_path, &vault_bytes)?;
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
}

fn handle_text_input(field: &mut String, key: KeyCode) {
    match key {
        KeyCode::Char(c) => field.push(c),
        KeyCode::Backspace => {
            field.pop();
        }
        _ => {}
    }
}

pub fn run(device: Option<PathBuf>) -> Result<()> {
    enable_raw_mode()?;
    let mut stdout = std::io::stdout();
    execute!(stdout, EnterAlternateScreen)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    let mut app = App::new(device);
    let result = app.run_loop(&mut terminal);

    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
    terminal.show_cursor()?;

    result
}
