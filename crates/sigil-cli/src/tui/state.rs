use sigil_core::{
    types::{PasswordEntry, TotpEntry, SshKeyEntry, FileEntry, NoteEntry},
    vault::{SlotKind, VaultBlob},
};

/// Top-level screen state machine.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Screen {
    Boot,
    DevicePicker,
    SetupWizard(WizardStep),
    Unlock,
    Vault(VaultView),
    Locked,
    Error(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WizardStep {
    Master,
    Decoy,
    Duress,
    Confirm,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum VaultView {
    Passwords,
    Totp,
    SshKeys,
    Files,
    Notes,
    Settings,
}

impl VaultView {
    pub fn label(&self) -> &'static str {
        match self {
            Self::Passwords => "PASSWORDS",
            Self::Totp => "TOTP",
            Self::SshKeys => "SSH",
            Self::Files => "FILES",
            Self::Notes => "NOTES",
            Self::Settings => "SETTINGS",
        }
    }

    pub fn next(&self) -> Self {
        match self {
            Self::Passwords => Self::Totp,
            Self::Totp => Self::SshKeys,
            Self::SshKeys => Self::Files,
            Self::Files => Self::Notes,
            Self::Notes => Self::Settings,
            Self::Settings => Self::Passwords,
        }
    }

    pub fn prev(&self) -> Self {
        match self {
            Self::Passwords => Self::Settings,
            Self::Totp => Self::Passwords,
            Self::SshKeys => Self::Totp,
            Self::Files => Self::SshKeys,
            Self::Notes => Self::Files,
            Self::Settings => Self::Notes,
        }
    }
}

/// In-memory unlocked vault state.
pub struct OpenVault {
    pub slot: SlotKind,
    pub blob: VaultBlob,
    pub vault_path: std::path::PathBuf,
    /// Fingerprint: first 8 hex chars of BLAKE3(vault header salt).
    pub fingerprint: String,
    /// Slot keys for re-encryption on save (must be zeroized on lock).
    pub slot_keys: [[u8; 32]; 4],
    pub last_activity: std::time::Instant,
}

/// Application state, threaded through all screens.
pub struct AppState {
    pub screen: Screen,
    /// Detected USB devices.
    pub devices: Vec<crate::device::UsbDevice>,
    /// Selected device index in the device picker.
    pub selected_device: usize,
    /// Passphrase input buffer (zeroized on navigation away).
    pub passphrase_input: String,
    /// Open vault, if unlocked.
    pub vault: Option<OpenVault>,
    /// Currently selected item index in the vault content pane.
    pub content_cursor: usize,
    /// Whether the content pane has focus (vs sidebar).
    pub content_focus: bool,
    /// Boot animation tick counter.
    pub boot_tick: u8,
    /// Error message to display on the Error screen.
    pub error_message: String,
    /// Which vault view is selected in sidebar.
    pub vault_view: VaultView,
    /// Wizard fields.
    pub wizard: WizardState,
    /// Clipboard auto-clear timestamp.
    pub clipboard_clear_at: Option<std::time::Instant>,
    /// Auto-lock timeout in seconds.
    pub auto_lock_secs: u64,
}

#[derive(Default)]
pub struct WizardState {
    pub main_pass: String,
    pub main_pass_confirm: String,
    pub decoy_pass: String,
    pub duress_pass: String,
    pub decoy_enabled: bool,
    pub duress_enabled: bool,
    /// Cursor position for currently active wizard input.
    pub cursor: usize,
    /// Which field is active within a wizard step.
    pub field: usize,
}

impl AppState {
    pub fn new(devices: Vec<crate::device::UsbDevice>) -> Self {
        Self {
            screen: Screen::Boot,
            devices,
            selected_device: 0,
            passphrase_input: String::new(),
            vault: None,
            content_cursor: 0,
            content_focus: false,
            boot_tick: 0,
            error_message: String::new(),
            vault_view: VaultView::Passwords,
            wizard: WizardState::default(),
            clipboard_clear_at: None,
            auto_lock_secs: 15 * 60,
        }
    }

    pub fn transition(&mut self, next: Screen) {
        // Zeroize passphrase when leaving auth screens.
        match &self.screen {
            Screen::Unlock | Screen::SetupWizard(_) => {
                use zeroize::Zeroize;
                self.passphrase_input.zeroize();
            }
            _ => {}
        }
        self.screen = next;
    }

    pub fn is_unlocked(&self) -> bool {
        self.vault.is_some()
    }

    /// Fingerprint display — first 4 bytes as hex with dash: "A3F2-9B84".
    pub fn vault_fingerprint(&self) -> String {
        self.vault
            .as_ref()
            .map(|v| v.fingerprint.clone())
            .unwrap_or_else(|| "----".to_string())
    }

    pub fn touch_activity(&mut self) {
        if let Some(v) = &mut self.vault {
            v.last_activity = std::time::Instant::now();
        }
    }

    pub fn is_auto_lock_due(&self) -> bool {
        if let Some(v) = &self.vault {
            v.last_activity.elapsed().as_secs() >= self.auto_lock_secs
        } else {
            false
        }
    }
}
