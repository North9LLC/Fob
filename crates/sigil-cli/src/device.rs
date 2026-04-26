use std::path::PathBuf;
use std::process::Command;

#[derive(Debug, Clone)]
pub struct UsbDevice {
    pub name: String,
    pub size_bytes: u64,
    pub path: PathBuf,         // mount point, e.g. /Volumes/MyDrive
    pub disk_node: String,     // e.g. "disk4" — used for formatting
    pub serial: Option<String>,
    pub has_sigil_vault: bool,
}

impl UsbDevice {
    pub fn size_display(&self) -> String {
        let gb = self.size_bytes as f64 / 1_073_741_824.0;
        if gb >= 1.0 { format!("{:.1} GB", gb) }
        else         { format!("{:.1} MB", self.size_bytes as f64 / 1_048_576.0) }
    }

    pub fn is_system_drive(&self) -> bool {
        let p = self.path.to_string_lossy();
        // Never allow the primary boot volume.
        p == "/Volumes/Macintosh HD" || p == "/" || p == "/Volumes/Macintosh HD - Data"
    }
}

/// Enumerate only removable, external USB drives visible to the OS.
///
/// On macOS this uses `diskutil info -all` to get authoritative removability data.
/// System drives are always excluded.
pub fn enumerate_usb_devices() -> Vec<UsbDevice> {
    #[cfg(target_os = "macos")]
    return enumerate_macos();

    #[cfg(target_os = "linux")]
    return enumerate_linux();

    #[cfg(not(any(target_os = "macos", target_os = "linux")))]
    return Vec::new();
}

// ── macOS ─────────────────────────────────────────────────────────────────

#[cfg(target_os = "macos")]
fn enumerate_macos() -> Vec<UsbDevice> {
    // `diskutil list -plist external` returns only external drives.
    // We parse /Volumes for mounted volumes on those disks.
    let mut devices = Vec::new();

    let out = match Command::new("diskutil").args(["list", "external"]).output() {
        Ok(o) => String::from_utf8_lossy(&o.stdout).into_owned(),
        Err(_) => return devices,
    };

    // Parse disk identifiers like "/dev/disk4 (external, physical):"
    let mut disks: Vec<String> = Vec::new();
    for line in out.lines() {
        if line.starts_with("/dev/disk") {
            // e.g. "/dev/disk4 (external, physical):"
            if let Some(dev) = line.split_whitespace().next() {
                let node = dev.trim_start_matches("/dev/").to_string();
                disks.push(node);
            }
        }
    }

    for disk in disks {
        if let Some(dev) = probe_disk_macos(&disk) {
            if !dev.is_system_drive() {
                devices.push(dev);
            }
        }
    }

    // Also scan /Volumes for anything we may have missed (e.g. disk already mounted).
    if let Ok(entries) = std::fs::read_dir("/Volumes") {
        for entry in entries.flatten() {
            let mount = entry.path();
            // Skip Macintosh HD
            let name = entry.file_name().to_string_lossy().to_string();
            if name == "Macintosh HD" || name == "Macintosh HD - Data" { continue; }

            // Check if already in devices list
            if devices.iter().any(|d| d.path == mount) { continue; }

            // Probe via diskutil info
            if let Some(dev) = probe_mount_macos(&mount) {
                if !dev.is_system_drive() {
                    devices.push(dev);
                }
            }
        }
    }

    devices.sort_by(|a,b| a.name.cmp(&b.name));
    devices
}

#[cfg(target_os = "macos")]
fn probe_disk_macos(disk_node: &str) -> Option<UsbDevice> {
    // Get disk info
    let info_out = Command::new("diskutil")
        .args(["info", disk_node])
        .output()
        .ok()?;
    let info = String::from_utf8_lossy(&info_out.stdout).into_owned();

    let removable = info.lines().any(|l| {
        let l = l.to_lowercase();
        l.contains("removable media") && (l.contains("yes") || l.contains("removable"))
    }) || info.lines().any(|l| {
        l.contains("Protocol") && (l.contains("USB") || l.contains("SD") || l.contains("Thunderbolt"))
    });

    // Find the mounted volume
    let mount_point = info.lines()
        .find(|l| l.trim_start().starts_with("Mount Point:"))?
        .trim_start_matches("Mount Point:")
        .trim()
        .to_string();

    if mount_point.is_empty() || mount_point == "Not applicable" { return None; }
    let mount = PathBuf::from(&mount_point);

    let name = info.lines()
        .find(|l| l.trim_start().starts_with("Volume Name:"))
        .map(|l| l.trim().trim_start_matches("Volume Name:").trim().to_string())
        .unwrap_or_else(|| disk_node.to_string());

    // diskutil format: "   Disk Size:   15.6 GB (15636365312 Bytes) ..."
    let size_bytes = info.lines()
        .find(|l| l.contains("Disk Size:") || l.contains("Total Size:"))
        .and_then(|l| l.split('(').nth(1))
        .and_then(|s| s.split_whitespace().next())
        .and_then(|s| s.replace(',', "").parse::<u64>().ok())
        .unwrap_or(0);

    let has_sigil_vault = mount.join("vault.sigil").exists();

    Some(UsbDevice {
        name, size_bytes,
        path: mount,
        disk_node: disk_node.to_string(),
        serial: None,
        has_sigil_vault,
    })
}

#[cfg(target_os = "macos")]
fn probe_mount_macos(mount: &std::path::Path) -> Option<UsbDevice> {
    // Reverse-lookup: find disk node for this mount point.
    let info_out = Command::new("diskutil")
        .args(["info", &mount.to_string_lossy()])
        .output()
        .ok()?;
    let info = String::from_utf8_lossy(&info_out.stdout).into_owned();

    // If not external/removable, skip.
    let protocol_line = info.lines().find(|l| l.contains("Protocol:"))?;
    let is_external = protocol_line.contains("USB")
        || protocol_line.contains("SD")
        || info.lines().any(|l| l.contains("External:") && l.contains("Yes"));
    if !is_external { return None; }

    let disk_node = info.lines()
        .find(|l| l.trim_start().starts_with("Device Node:"))
        .map(|l| l.trim_start_matches("Device Node:").trim().trim_start_matches("/dev/").to_string())
        .unwrap_or_default();

    let name = info.lines()
        .find(|l| l.trim_start().starts_with("Volume Name:"))
        .map(|l| l.trim().trim_start_matches("Volume Name:").trim().to_string())
        .unwrap_or_else(|| mount.file_name().unwrap_or_default().to_string_lossy().to_string());

    let size_bytes = info.lines()
        .find(|l| l.contains("Disk Size:") || l.contains("Total Size:"))
        .and_then(|l| l.split('(').nth(1))
        .and_then(|s| s.split_whitespace().next())
        .and_then(|s| s.replace(',', "").parse::<u64>().ok())
        .unwrap_or(0);

    let has_sigil_vault = mount.join("vault.sigil").exists();

    Some(UsbDevice {
        name, size_bytes, path: mount.to_path_buf(),
        disk_node, serial: None, has_sigil_vault,
    })
}

// ── Linux ─────────────────────────────────────────────────────────────────

#[cfg(target_os = "linux")]
fn enumerate_linux() -> Vec<UsbDevice> {
    let mut devices = Vec::new();
    let Ok(entries) = std::fs::read_dir("/sys/block") else { return devices };

    for entry in entries.flatten() {
        let name = entry.file_name().to_string_lossy().to_string();
        let removable = std::fs::read_to_string(entry.path().join("removable"))
            .map(|s| s.trim() == "1").unwrap_or(false);
        if !removable { continue; }

        // Skip system drive heuristically
        if name.starts_with("sda") && name.len() == 3 { continue; }

        let size_bytes = std::fs::read_to_string(entry.path().join("size"))
            .ok()
            .and_then(|s| s.trim().parse::<u64>().ok())
            .map(|s| s * 512)
            .unwrap_or(0);

        // Find mount point from /proc/mounts
        let Ok(mounts) = std::fs::read_to_string("/proc/mounts") else { continue };
        for line in mounts.lines() {
            let parts: Vec<&str> = line.split_whitespace().collect();
            if parts.len() < 2 { continue; }
            if !parts[0].contains(&name) { continue; }
            let mount = PathBuf::from(parts[1]);
            let has_vault = mount.join("vault.sigil").exists();
            devices.push(UsbDevice {
                name: name.clone(), size_bytes,
                path: mount,
                disk_node: name.clone(),
                serial: None,
                has_sigil_vault: has_vault,
            });
            break;
        }
    }

    devices.sort_by(|a,b| a.name.cmp(&b.name));
    devices
}

// ── Format (wipe) ─────────────────────────────────────────────────────────

/// Format the given USB device as ExFAT with the label "SIGIL".
/// This is destructive — all data is erased.
///
/// On macOS uses `diskutil eraseDisk`.
/// On Linux uses `mkfs.exfat`.
pub fn format_device(dev: &UsbDevice) -> anyhow::Result<()> {
    if dev.is_system_drive() {
        anyhow::bail!("Refusing to format system drive.");
    }

    #[cfg(target_os = "macos")]
    {
        // eraseDisk needs the whole disk (disk4), not a partition (disk4s1).
        let whole_disk = dev.disk_node
            .trim_end_matches(|c: char| c.is_ascii_digit())
            .trim_end_matches('s')
            .to_string();
        let cmd = format!("diskutil eraseDisk ExFAT SIGIL {}", whole_disk);
        // Use osascript to request admin privileges via the standard macOS dialog.
        let out = Command::new("osascript")
            .args([
                "-e",
                &format!("do shell script \"{}\" with administrator privileges", cmd),
            ])
            .output()?;
        if !out.status.success() {
            let stderr = String::from_utf8_lossy(&out.stderr);
            anyhow::bail!("Format failed: {}", stderr.trim());
        }
    }

    #[cfg(target_os = "linux")]
    {
        // Unmount first
        let _ = Command::new("umount").arg(&dev.disk_node).status();
        let status = Command::new("mkfs.exfat")
            .args(["-n", "SIGIL", &format!("/dev/{}", dev.disk_node)])
            .status()?;
        if !status.success() {
            anyhow::bail!("mkfs.exfat failed. Install exfat-utils.");
        }
    }

    Ok(())
}

/// Find the new mount point after formatting (diskutil remounts automatically).
pub fn find_mount_after_format(old_disk_node: &str) -> Option<PathBuf> {
    // Give the OS a moment to remount.
    std::thread::sleep(std::time::Duration::from_secs(2));
    let mount = PathBuf::from("/Volumes/SIGIL");
    if mount.exists() { return Some(mount); }
    // Fallback: try probing
    #[cfg(target_os = "macos")]
    if let Some(dev) = probe_disk_macos(old_disk_node) { return Some(dev.path); }
    None
}
