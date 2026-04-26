use sigil_core::vault::{SlotKind, VaultBlob};

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
    DecoyPassphrase,
    Duress,
    DuressPassphrase,
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
            Self::Totp     => "TOTP",
            Self::SshKeys  => "SSH KEYS",
            Self::Files    => "FILES",
            Self::Notes    => "NOTES",
            Self::Settings => "SETTINGS",
        }
    }

    pub fn icon(&self) -> &'static str {
        match self {
            Self::Passwords => "🔑",
            Self::Totp      => "⏱",
            Self::SshKeys   => "🔐",
            Self::Files     => "📁",
            Self::Notes     => "📝",
            Self::Settings  => "⚙",
        }
    }

    pub fn next(&self) -> Self {
        match self {
            Self::Passwords => Self::Totp,
            Self::Totp      => Self::SshKeys,
            Self::SshKeys   => Self::Files,
            Self::Files     => Self::Notes,
            Self::Notes     => Self::Settings,
            Self::Settings  => Self::Passwords,
        }
    }

    pub fn prev(&self) -> Self {
        match self {
            Self::Passwords => Self::Settings,
            Self::Totp      => Self::Passwords,
            Self::SshKeys   => Self::Totp,
            Self::Files     => Self::SshKeys,
            Self::Notes     => Self::Files,
            Self::Settings  => Self::Notes,
        }
    }

    pub fn all() -> &'static [VaultView] {
        &[
            VaultView::Passwords,
            VaultView::Totp,
            VaultView::SshKeys,
            VaultView::Files,
            VaultView::Notes,
            VaultView::Settings,
        ]
    }
}

pub struct OpenVault {
    pub slot: SlotKind,
    pub blob: VaultBlob,
    pub vault_path: std::path::PathBuf,
    pub fingerprint: String,
    pub slot_keys: [[u8; 32]; 4],
    pub last_activity: std::time::Instant,
}

pub struct AppState {
    pub screen: Screen,
    pub devices: Vec<crate::device::UsbDevice>,
    pub selected_device: usize,
    pub passphrase_input: String,
    pub vault: Option<OpenVault>,
    pub content_cursor: usize,
    pub content_focus: bool,
    pub boot_tick: u8,
    pub error_message: String,
    pub vault_view: VaultView,
    pub wizard: WizardState,
    pub clipboard_clear_at: Option<std::time::Instant>,
    pub auto_lock_secs: u64,
    /// Ticks remaining for the red-flash animation on failed unlock.
    pub unlock_flash_ticks: u8,
    /// Number of consecutive failed unlock attempts.
    pub unlock_attempts: u8,
    /// Whether the selected password is revealed in plaintext.
    pub show_password: bool,
    /// Live search query (activated with /).
    pub search_query: Option<String>,
    /// Global tick counter, drives TOTP refresh and animations.
    pub tick: u64,
}

#[derive(Default)]
pub struct WizardState {
    pub main_pass: String,
    pub main_pass_confirm: String,
    pub decoy_pass: String,
    pub duress_pass: String,
    pub decoy_enabled: bool,
    pub duress_enabled: bool,
    pub field: usize,
    /// Mismatch flash for confirm field.
    pub mismatch_flash: u8,
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
            unlock_flash_ticks: 0,
            unlock_attempts: 0,
            show_password: false,
            search_query: None,
            tick: 0,
        }
    }

    pub fn transition(&mut self, next: Screen) {
        use zeroize::Zeroize;
        match &self.screen {
            Screen::Unlock | Screen::SetupWizard(_) => {
                self.passphrase_input.zeroize();
            }
            _ => {}
        }
        self.show_password = false;
        self.search_query = None;
        self.content_cursor = 0;
        self.screen = next;
    }

    pub fn is_unlocked(&self) -> bool {
        self.vault.is_some()
    }

    pub fn vault_fingerprint(&self) -> String {
        self.vault
            .as_ref()
            .map(|v| v.fingerprint.clone())
            .unwrap_or_else(|| "----·----·----·----".into())
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

    pub fn entry_count_for_view(&self, view: &VaultView) -> usize {
        let Some(v) = &self.vault else { return 0 };
        match view {
            VaultView::Passwords => v.blob.passwords.len(),
            VaultView::Totp      => v.blob.totps.len(),
            VaultView::SshKeys   => v.blob.ssh_keys.len(),
            VaultView::Files     => v.blob.files.len(),
            VaultView::Notes     => v.blob.notes.len(),
            VaultView::Settings  => 0,
        }
    }
}
