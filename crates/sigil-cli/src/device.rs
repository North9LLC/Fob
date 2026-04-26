use std::path::PathBuf;

/// Metadata about a detected USB mass-storage device.
#[derive(Debug, Clone)]
pub struct UsbDevice {
    pub name: String,
    pub size_bytes: u64,
    pub path: PathBuf,
    pub serial: Option<String>,
    pub has_sigil_vault: bool,
}

impl UsbDevice {
    pub fn size_display(&self) -> String {
        let gb = self.size_bytes as f64 / 1_073_741_824.0;
        if gb >= 1.0 {
            format!("{:.1} GB", gb)
        } else {
            let mb = self.size_bytes as f64 / 1_048_576.0;
            format!("{:.1} MB", mb)
        }
    }
}

/// Enumerate all removable USB mass-storage devices visible to the OS.
///
/// Returns an empty vec if no devices are found or detection is not
/// supported on this platform.
pub fn enumerate_usb_devices() -> Vec<UsbDevice> {
    #[cfg(target_os = "linux")]
    return enumerate_linux();

    #[cfg(target_os = "macos")]
    return enumerate_macos();

    #[cfg(not(any(target_os = "linux", target_os = "macos")))]
    return Vec::new();
}

#[cfg(target_os = "linux")]
fn enumerate_linux() -> Vec<UsbDevice> {
    use std::fs;
    let mut devices = Vec::new();

    let block_dir = std::path::Path::new("/sys/block");
    let Ok(entries) = fs::read_dir(block_dir) else {
        return devices;
    };

    for entry in entries.flatten() {
        let name = entry.file_name().to_string_lossy().to_string();
        let removable_path = entry.path().join("removable");

        let removable = fs::read_to_string(&removable_path)
            .map(|s| s.trim() == "1")
            .unwrap_or(false);

        if !removable {
            continue;
        }

        let size_bytes = fs::read_to_string(entry.path().join("size"))
            .ok()
            .and_then(|s| s.trim().parse::<u64>().ok())
            .map(|sectors| sectors * 512)
            .unwrap_or(0);

        let dev_path = PathBuf::from(format!("/dev/{}", name));

        // Check if a sigil vault exists on any known mount point.
        let has_vault = find_sigil_vault_linux(&name);

        devices.push(UsbDevice {
            name: name.clone(),
            size_bytes,
            path: dev_path,
            serial: read_serial_linux(&name),
            has_sigil_vault: has_vault,
        });
    }

    devices.sort_by(|a, b| a.name.cmp(&b.name));
    devices
}

#[cfg(target_os = "linux")]
fn read_serial_linux(dev_name: &str) -> Option<String> {
    let serial_path = format!("/sys/block/{}/device/serial", dev_name);
    std::fs::read_to_string(serial_path)
        .ok()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
}

#[cfg(target_os = "linux")]
fn find_sigil_vault_linux(dev_name: &str) -> bool {
    // Check /proc/mounts for any mount of a partition on this device.
    let Ok(mounts) = std::fs::read_to_string("/proc/mounts") else {
        return false;
    };
    for line in mounts.lines() {
        let parts: Vec<&str> = line.split_whitespace().collect();
        if parts.len() < 2 {
            continue;
        }
        let dev = parts[0];
        let mount = parts[1];
        // Match sda, sda1, sdb, etc.
        if dev.contains(dev_name) {
            let vault_path = std::path::Path::new(mount).join("vault.sigil");
            if vault_path.exists() {
                return true;
            }
        }
    }
    false
}

#[cfg(target_os = "macos")]
fn enumerate_macos() -> Vec<UsbDevice> {
    // Simplified: use diskutil to list removable volumes.
    // A full implementation would use IOKit.
    let output = std::process::Command::new("diskutil")
        .args(["list", "-plist", "external"])
        .output();

    // Minimal fallback: scan /Volumes for vault files.
    let mut devices = Vec::new();

    let volumes = std::path::Path::new("/Volumes");
    if let Ok(entries) = std::fs::read_dir(volumes) {
        for entry in entries.flatten() {
            let vault_path = entry.path().join("vault.sigil");
            let has_vault = vault_path.exists();
            devices.push(UsbDevice {
                name: entry.file_name().to_string_lossy().to_string(),
                size_bytes: 0,
                path: entry.path(),
                serial: None,
                has_sigil_vault: has_vault,
            });
        }
    }

    devices
}
