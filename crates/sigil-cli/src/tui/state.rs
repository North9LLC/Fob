use zeroize::Zeroize;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Screen {
    Boot,
    DevicePicker,
    Formatting,
    SetupWizard(WizardStep),
    Done,
    Error(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WizardStep {
    ConfirmWipe,
    Master,
    Confirm,
}

pub struct AppState {
    pub screen: Screen,
    pub devices: Vec<crate::device::UsbDevice>,
    pub selected_device: usize,
    pub boot_tick: u8,
    pub wizard: WizardState,
    pub tick: u64,
    pub format_rx: Option<std::sync::mpsc::Receiver<anyhow::Result<()>>>,
}

#[derive(Default)]
pub struct WizardState {
    pub main_pass: String,
    pub main_pass_confirm: String,
    pub field: usize,
    pub mismatch_flash: u8,
}

impl AppState {
    pub fn new(devices: Vec<crate::device::UsbDevice>) -> Self {
        Self {
            screen: Screen::Boot,
            devices,
            selected_device: 0,
            boot_tick: 0,
            wizard: WizardState::default(),
            tick: 0,
            format_rx: None,
        }
    }
}

impl Drop for AppState {
    fn drop(&mut self) {
        self.wizard.main_pass.zeroize();
        self.wizard.main_pass_confirm.zeroize();
    }
}
