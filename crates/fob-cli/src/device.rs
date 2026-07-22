use std::path::PathBuf;
use std::process::Command;

#[derive(Debug, Clone)]
pub struct UsbDevice {
    pub name: String,
    pub size_bytes: u64,
    pub path: PathBuf,     // mount point, e.g. /Volumes/MyDrive
    pub disk_node: String, // e.g. "disk4" — used for formatting
    #[allow(dead_code)]
    pub serial: Option<String>,
    pub has_fob_vault: bool,
}

impl UsbDevice {
    pub fn size_display(&self) -> String {
        let gb = self.size_bytes as f64 / 1_073_741_824.0;
        if gb >= 1.0 {
            format!("{:.1} GB", gb)
        } else {
            format!("{:.1} MB", self.size_bytes as f64 / 1_048_576.0)
        }
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

// ── diskutil output parsing (pure — platform-independent, unit tested) ────
//
// Split out from the macOS I/O-driving code below so the parsing logic —
// where real bugs hide — can be tested on any OS, not just under
// `#[cfg(target_os = "macos")]` on a macOS CI runner.

/// Parse a `diskutil info` block's "Mount Point:" line.
#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
fn parse_mount_point(info: &str) -> Option<PathBuf> {
    let raw = info
        .lines()
        .find(|l| l.trim_start().starts_with("Mount Point:"))?
        .trim_start()
        .trim_start_matches("Mount Point:")
        .trim();
    if raw.is_empty() || raw == "Not applicable" {
        None
    } else {
        Some(PathBuf::from(raw))
    }
}

/// Parse a `diskutil info` block's "Volume Name:" line. Returns `None` for
/// a blank label (common on cheap/factory-formatted FAT32 sticks — real
/// diskutil renders these as an empty "Volume Name:" value, not the word
/// "Untitled" that only appears for the *default OS-assigned* name), not
/// `Some("")` — callers fall back to the disk node identifier via
/// `unwrap_or_else`, and that fallback never triggers if this returns
/// `Some` for an empty string.
#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
fn parse_volume_name(info: &str) -> Option<String> {
    let name = info
        .lines()
        .find(|l| l.trim_start().starts_with("Volume Name:"))?
        .trim()
        .trim_start_matches("Volume Name:")
        .trim()
        .to_string();
    if name.is_empty() {
        None
    } else {
        Some(name)
    }
}

/// Parse a `diskutil info` block's size line, e.g.
/// `   Disk Size:   15.6 GB (15636365312 Bytes) ...` → `15636365312`.
///
/// Checks "Disk Size:"/"Total Size:" (ExFAT/FAT32/plain disks) as well as
/// "Volume Total Space:"/"Container Total Space:" (real diskutil's field
/// names for HFS+ volumes and APFS containers respectively) — without the
/// latter two, any pre-existing non-ExFAT drive (e.g. a Mac-formatted
/// external HFS+/APFS drive the user is about to reformat) silently shows
/// a size of 0.
#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
fn parse_disk_size_bytes(info: &str) -> u64 {
    info.lines()
        .find(|l| {
            l.contains("Disk Size:")
                || l.contains("Total Size:")
                || l.contains("Volume Total Space:")
                || l.contains("Container Total Space:")
        })
        .and_then(|l| l.split('(').nth(1))
        .and_then(|s| s.split_whitespace().next())
        .and_then(|s| s.replace(',', "").parse::<u64>().ok())
        .unwrap_or(0)
}

/// Parse a `diskutil info` block's "Device Node:" line into a bare node name
/// (`/dev/disk4s1` → `disk4s1`).
#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
fn parse_device_node(info: &str) -> Option<String> {
    info.lines()
        .find_map(|l| l.trim_start().strip_prefix("Device Node:"))
        .map(|rest| rest.trim().trim_start_matches("/dev/").to_string())
}

/// Does this `diskutil info` block (queried by mount point) describe an
/// external disk?
#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
fn is_external_diskutil_info(info: &str) -> bool {
    let protocol_says_external = info
        .lines()
        .find(|l| l.contains("Protocol:"))
        .is_some_and(|l| l.contains("USB") || l.contains("SD") || l.contains("Thunderbolt"));
    // Real diskutil's actual field for this is "Device Location: External"
    // (vs. "Internal") — a literal "External: Yes" line, the previous
    // fallback here, doesn't correspond to any real diskutil output and
    // was dead code. This fallback also catches Thunderbolt/USB4 NVMe
    // enclosures that diskutil sometimes reports as `Protocol: PCI-Express`
    // instead of a bus name the check above would recognize.
    let device_location_says_external = info
        .lines()
        .any(|l| l.contains("Device Location:") && l.contains("External"));
    protocol_says_external || device_location_says_external
}

/// Extract every probeable disk node identifier from `diskutil list
/// external` output: both whole-disk headers (`/dev/disk4 (external,
/// physical):` → `disk4`) and each disk's partition rows (an indented
/// numbered row like `1:  Windows_FAT_32 FOB  15.6 GB  disk4s1` →
/// `disk4s1`, identified by the last whitespace-separated token).
///
/// Real `diskutil info` only reports Mount Point/Volume Name on a
/// PARTITION identifier, never on the whole-disk identifier of a normally
/// partitioned drive — probing only whole-disk ids (this function's
/// previous behavior) silently found nothing for any real partitioned USB
/// stick, leaving detection entirely dependent on the separate `/Volumes`
/// fallback scan (which only sees already-mounted volumes). Whole-disk ids
/// are still included too, since that's the one case where a whole-disk
/// query *does* succeed: an unpartitioned "superfloppy"-formatted drive.
#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
fn parse_external_disk_nodes(list_output: &str) -> Vec<String> {
    let mut nodes: Vec<String> = Vec::new();
    let mut push_unique = |node: String| {
        if !nodes.contains(&node) {
            nodes.push(node);
        }
    };

    for line in list_output.lines() {
        if let Some(rest) = line.strip_prefix("/dev/") {
            if let Some(node) = rest.split_whitespace().next() {
                push_unique(node.to_string());
            }
            continue;
        }

        // An indented row like "   1:   Windows_FAT_32 FOB   15.6 GB   disk4s1"
        // — the header row ("#:  TYPE NAME  SIZE  IDENTIFIER") starts with
        // '#' rather than a digit, so it's naturally excluded here.
        let trimmed = line.trim_start();
        let is_numbered_row = trimmed
            .split_once(':')
            .is_some_and(|(n, _)| !n.is_empty() && n.bytes().all(|b| b.is_ascii_digit()));
        if is_numbered_row {
            if let Some(identifier) = line.split_whitespace().last() {
                if identifier.starts_with("disk") {
                    push_unique(identifier.to_string());
                }
            }
        }
    }
    nodes
}

/// The whole-disk identifier for a given disk node — `diskutil eraseDisk`
/// needs the whole disk (`disk4`), not a partition (`disk4s1`).
///
/// Only strips the `sM` partition suffix when one is actually present.
/// The previous version unconditionally trimmed trailing digits then a
/// trailing `s`, which also mangled a disk node that's *already* a bare
/// whole-disk identifier with no partition suffix at all (an unpartitioned
/// "superfloppy"-formatted drive, the one case where a whole-disk
/// `diskutil info` query actually succeeds) — `"disk4"` (no partition)
/// would incorrectly become `"disk"`, an invalid device id.
#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
fn whole_disk_identifier(disk_node: &str) -> String {
    if let Some(s_pos) = disk_node.rfind('s') {
        let (before, after) = disk_node.split_at(s_pos);
        let partition_suffix = &after[1..]; // skip the 's' itself
        let looks_like_partition = !partition_suffix.is_empty()
            && partition_suffix.bytes().all(|b| b.is_ascii_digit())
            && before.ends_with(|c: char| c.is_ascii_digit());
        if looks_like_partition {
            return before.to_string();
        }
    }
    disk_node.to_string()
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

    for disk in parse_external_disk_nodes(&out) {
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
            if name == "Macintosh HD" || name == "Macintosh HD - Data" {
                continue;
            }

            // Check if already in devices list
            if devices.iter().any(|d| d.path == mount) {
                continue;
            }

            // Probe via diskutil info
            if let Some(dev) = probe_mount_macos(&mount) {
                if !dev.is_system_drive() {
                    devices.push(dev);
                }
            }
        }
    }

    devices.sort_by(|a, b| a.name.cmp(&b.name));
    devices
}

#[cfg(target_os = "macos")]
fn probe_disk_macos(disk_node: &str) -> Option<UsbDevice> {
    let info_out = Command::new("diskutil")
        .args(["info", disk_node])
        .output()
        .ok()?;
    let info = String::from_utf8_lossy(&info_out.stdout).into_owned();

    // No is_removable_diskutil_info() check here: every disk_node this is
    // called with already came from `diskutil list external`, so it's
    // already known to be external at the listing stage — that's the real
    // gate, not the separate "Removable Media:" flag (which some external
    // SSDs report as "Fixed" despite genuinely being external/USB-attached).
    let mount = parse_mount_point(&info)?;
    let name = parse_volume_name(&info).unwrap_or_else(|| disk_node.to_string());
    let size_bytes = parse_disk_size_bytes(&info);
    let has_fob_vault = mount.join("vault.fob").exists();

    Some(UsbDevice {
        name,
        size_bytes,
        path: mount,
        disk_node: disk_node.to_string(),
        serial: None,
        has_fob_vault,
    })
}

#[cfg(target_os = "macos")]
fn probe_mount_macos(mount: &std::path::Path) -> Option<UsbDevice> {
    let info_out = Command::new("diskutil")
        .args(["info", &mount.to_string_lossy()])
        .output()
        .ok()?;
    let info = String::from_utf8_lossy(&info_out.stdout).into_owned();

    if !is_external_diskutil_info(&info) {
        return None;
    }

    let disk_node = parse_device_node(&info).unwrap_or_default();
    let name = parse_volume_name(&info).unwrap_or_else(|| {
        mount
            .file_name()
            .unwrap_or_default()
            .to_string_lossy()
            .to_string()
    });
    let size_bytes = parse_disk_size_bytes(&info);
    let has_fob_vault = mount.join("vault.fob").exists();

    Some(UsbDevice {
        name,
        size_bytes,
        path: mount.to_path_buf(),
        disk_node,
        serial: None,
        has_fob_vault,
    })
}

// ── Linux ─────────────────────────────────────────────────────────────────

/// Is this `/sys/block` entry name likely the system's primary disk? Purely
/// a heuristic (`sda` with no partition suffix) — real safety comes from
/// `UsbDevice::is_system_drive`'s mount-point check.
#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
fn is_likely_system_disk_linux(name: &str) -> bool {
    name.starts_with("sda") && name.len() == 3
}

/// Decode the octal escapes (`\040` space, `\011` tab, `\012` newline,
/// `\134` backslash) the kernel uses for special characters in
/// `/proc/mounts` fields. Without this, any USB drive whose volume label
/// contains a space — extremely common (Windows-default names, "FOB
/// BACKUP", "My Passport") — resolves to a mount path containing a literal
/// `\040` instead of a space, which doesn't exist on disk, so `vault.fob`
/// detection and every subsequent file operation silently fail.
#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
fn decode_mounts_escapes(s: &str) -> String {
    let bytes = s.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'\\'
            && i + 3 < bytes.len()
            && bytes[i + 1..i + 4].iter().all(u8::is_ascii_digit)
        {
            let octal = std::str::from_utf8(&bytes[i + 1..i + 4]).unwrap();
            if let Ok(byte) = u8::from_str_radix(octal, 8) {
                out.push(byte);
                i += 4;
                continue;
            }
        }
        out.push(bytes[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

/// Find the mount point for a block device named `device_name` (a whole
/// disk like `sdb`, or a specific partition like `sdb1`) by scanning
/// `/proc/mounts`-formatted content.
///
/// Matches the device itself or any of its partitions (`sdb` matches
/// `/dev/sdb1`, `/dev/sdb2`, ...) via a boundary-anchored prefix check, not
/// a bare substring — a raw `.contains()` could also match an unrelated
/// device whose name happens to contain the same characters. If a disk has
/// multiple mounted partitions, prefers whichever one already has a
/// `vault.fob` (there's no other principled way to pick among them), else
/// falls back to the first one found.
#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
fn find_mount_point(mounts_content: &str, device_name: &str) -> Option<PathBuf> {
    let candidates: Vec<PathBuf> = mounts_content
        .lines()
        .filter_map(|line| {
            let parts: Vec<&str> = line.split_whitespace().collect();
            if parts.len() < 2 {
                return None;
            }
            let dev = parts[0].strip_prefix("/dev/").unwrap_or(parts[0]);
            let is_this_disk = dev == device_name
                || (dev.len() > device_name.len()
                    && dev.starts_with(device_name)
                    && dev[device_name.len()..].bytes().all(|b| b.is_ascii_digit()));
            is_this_disk.then(|| PathBuf::from(decode_mounts_escapes(parts[1])))
        })
        .collect();

    candidates
        .iter()
        .find(|p| p.join("vault.fob").exists())
        .cloned()
        .or_else(|| candidates.into_iter().next())
}

/// Does this resolved/symlink-target `/sys/block/<name>` device path
/// indicate the device is attached via USB (a path component that's
/// exactly `usb` followed by a bus number, e.g. `.../usb1/1-1/...`),
/// regardless of what the device's own `removable` sysfs flag says?
///
/// Many USB-to-SATA and USB-to-NVMe bridge chips report `removable=0`
/// (reflecting the bridge's own SCSI RMB bit, not whether the whole
/// enclosure is actually hot-pluggable) — relying on `removable` alone
/// silently hides external SSDs/NVMe enclosures from `fob devices` while
/// plain flash drives (which do report `removable=1`) work fine.
#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
fn resolved_path_indicates_usb(resolved_path: &str) -> bool {
    resolved_path.split('/').any(|component| {
        component
            .strip_prefix("usb")
            .is_some_and(|rest| !rest.is_empty() && rest.bytes().all(|b| b.is_ascii_digit()))
    })
}

#[cfg(target_os = "linux")]
fn enumerate_linux() -> Vec<UsbDevice> {
    let mut devices = Vec::new();
    let Ok(entries) = std::fs::read_dir("/sys/block") else {
        return devices;
    };

    let Ok(mounts) = std::fs::read_to_string("/proc/mounts") else {
        return devices;
    };

    for entry in entries.flatten() {
        let name = entry.file_name().to_string_lossy().to_string();
        let removable = std::fs::read_to_string(entry.path().join("removable"))
            .map(|s| s.trim() == "1")
            .unwrap_or(false);
        // `removable` alone misses USB-SATA/USB-NVMe bridges that report
        // removable=0 — also accept devices whose sysfs symlink resolves
        // through a USB bus, so those enclosures aren't silently invisible.
        let is_usb = std::fs::read_link(entry.path())
            .map(|p| resolved_path_indicates_usb(&p.to_string_lossy()))
            .unwrap_or(false);
        if (!removable && !is_usb) || is_likely_system_disk_linux(&name) {
            continue;
        }

        let size_bytes = std::fs::read_to_string(entry.path().join("size"))
            .ok()
            .and_then(|s| s.trim().parse::<u64>().ok())
            .map(|s| s * 512)
            .unwrap_or(0);

        let Some(mount) = find_mount_point(&mounts, &name) else {
            continue;
        };
        let has_fob_vault = mount.join("vault.fob").exists();
        devices.push(UsbDevice {
            name: name.clone(),
            size_bytes,
            path: mount,
            disk_node: name,
            serial: None,
            has_fob_vault,
        });
    }

    devices.sort_by(|a, b| a.name.cmp(&b.name));
    devices
}

// ── Format (wipe) ─────────────────────────────────────────────────────────

/// Format the given USB device as ExFAT with the label "FOB".
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
        let whole_disk = whole_disk_identifier(&dev.disk_node);
        let cmd = format!("diskutil eraseDisk ExFAT FOB {}", whole_disk);
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
            .args(["-n", "FOB", &format!("/dev/{}", dev.disk_node)])
            .status()?;
        if !status.success() {
            anyhow::bail!("mkfs.exfat failed. Install exfat-utils.");
        }
    }

    Ok(())
}

/// Find the new mount point after formatting (diskutil remounts automatically).
#[allow(dead_code)]
pub fn find_mount_after_format(_old_disk_node: &str) -> Option<PathBuf> {
    // Give the OS a moment to remount.
    std::thread::sleep(std::time::Duration::from_secs(2));
    let mount = PathBuf::from("/Volumes/FOB");
    if mount.exists() {
        return Some(mount);
    }
    // Fallback: try probing
    #[cfg(target_os = "macos")]
    if let Some(dev) = probe_disk_macos(_old_disk_node) {
        return Some(dev.path);
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    const DISKUTIL_INFO_EXTERNAL_USB: &str = "
   Device Identifier:        disk4s1
   Device Node:              /dev/disk4s1
   Whole:                    No
   Part of Whole:            disk4

   Volume Name:              FOB
   Mounted:                  Yes
   Mount Point:              /Volumes/FOB

   Partition Type:           Windows_FAT_32
   File System Personality:  ExFAT

   Removable Media:          Removable
   Media Type:               Generic

   Protocol:                 USB

   Disk Size:                15.6 GB (15636365312 Bytes) (exactly 30539776 512-Byte-Units)
";

    const DISKUTIL_INFO_INTERNAL: &str = "
   Device Identifier:        disk1s1
   Device Node:              /dev/disk1s1
   Volume Name:              Macintosh HD
   Mounted:                  Yes
   Mount Point:              /

   Protocol:                 Apple Fabric
   Disk Size:                500.3 GB (500277790720 Bytes)
";

    const DISKUTIL_INFO_UNMOUNTED: &str = "
   Device Identifier:        disk5s1
   Device Node:              /dev/disk5s1
   Volume Name:              Untitled
   Mounted:                  No
   Mount Point:              Not applicable

   Protocol:                 USB
   Disk Size:                8.0 GB (8000000000 Bytes)
";

    #[test]
    fn parses_mount_point_from_real_diskutil_output() {
        assert_eq!(
            parse_mount_point(DISKUTIL_INFO_EXTERNAL_USB),
            Some(PathBuf::from("/Volumes/FOB"))
        );
    }

    #[test]
    fn parses_mount_point_none_when_not_applicable() {
        assert_eq!(parse_mount_point(DISKUTIL_INFO_UNMOUNTED), None);
    }

    #[test]
    fn parses_volume_name() {
        assert_eq!(
            parse_volume_name(DISKUTIL_INFO_EXTERNAL_USB),
            Some("FOB".to_string())
        );
    }

    #[test]
    fn parses_volume_name_returns_none_for_blank_label() {
        let info = "
   Device Identifier:        disk6s1
   Device Node:              /dev/disk6s1
   Volume Name:
   Mounted:                  Yes
   Mount Point:              /Volumes/Untitled 1

   Protocol:                 USB
   Disk Size:                4.0 GB (4000000000 Bytes)
";
        assert_eq!(parse_volume_name(info), None);
    }

    #[test]
    fn parses_disk_size_bytes() {
        assert_eq!(
            parse_disk_size_bytes(DISKUTIL_INFO_EXTERNAL_USB),
            15636365312
        );
        assert_eq!(parse_disk_size_bytes(DISKUTIL_INFO_INTERNAL), 500277790720);
    }

    #[test]
    fn parses_disk_size_bytes_for_hfs_volume_total_space() {
        let info = "
   Volume Name:              Backup
   Protocol:                 USB
   Volume Total Space:       128.0 GB (128035676160 Bytes)
";
        assert_eq!(parse_disk_size_bytes(info), 128035676160);
    }

    #[test]
    fn parses_disk_size_bytes_for_apfs_container_total_space() {
        let info = "
   Volume Name:              MyAPFS
   Protocol:                 USB
   Container Total Space:    250.0 GB (250059350016 Bytes)
";
        assert_eq!(parse_disk_size_bytes(info), 250059350016);
    }

    #[test]
    fn parses_disk_size_bytes_zero_when_missing() {
        assert_eq!(parse_disk_size_bytes("no size info here"), 0);
    }

    #[test]
    fn parses_device_node_strips_dev_prefix() {
        assert_eq!(
            parse_device_node(DISKUTIL_INFO_EXTERNAL_USB),
            Some("disk4s1".to_string())
        );
    }

    #[test]
    fn detects_external_by_protocol() {
        assert!(is_external_diskutil_info(DISKUTIL_INFO_EXTERNAL_USB));
        assert!(!is_external_diskutil_info(DISKUTIL_INFO_INTERNAL));
    }

    #[test]
    fn detects_external_by_thunderbolt_protocol() {
        let info = "
   Volume Name:              TBDrive
   Protocol:                 Thunderbolt
";
        assert!(is_external_diskutil_info(info));
    }

    #[test]
    fn detects_external_via_device_location_fallback() {
        // A Thunderbolt/USB4 NVMe enclosure diskutil reports as
        // Protocol: PCI-Express — must still be caught via the real
        // "Device Location: External" field.
        let info = "
   Volume Name:              NVMeDrive
   Protocol:                 PCI-Express
   Device Location:          External
";
        assert!(is_external_diskutil_info(info));
    }

    #[test]
    fn internal_pcie_drive_is_not_external() {
        let info = "
   Volume Name:              Macintosh HD
   Protocol:                 PCI-Express
   Device Location:          Internal
";
        assert!(!is_external_diskutil_info(info));
    }

    #[test]
    fn parses_external_disk_nodes_from_list_output() {
        let list_output = "\
/dev/disk4 (external, physical):
   #:                       TYPE NAME                    SIZE       IDENTIFIER
   0:      FDisk_partition_scheme                        *15.6 GB    disk4
   1:                 Windows_FAT_32 FOB                  15.6 GB    disk4s1

/dev/disk5 (external, physical):
   #:                       TYPE NAME                    SIZE       IDENTIFIER
   0:      FDisk_partition_scheme                        *8.0 GB     disk5
";
        // Must include the real partition identifier "disk4s1" — real
        // diskutil only reports Mount Point/Volume Name on a partition
        // identifier, never on a normally-partitioned drive's whole-disk
        // identifier, so probing only "disk4"/"disk5" (the old behavior)
        // silently found nothing for either drive.
        assert_eq!(
            parse_external_disk_nodes(list_output),
            vec![
                "disk4".to_string(),
                "disk4s1".to_string(),
                "disk5".to_string()
            ]
        );
    }

    #[test]
    fn parses_external_disk_nodes_empty_when_none_present() {
        assert_eq!(
            parse_external_disk_nodes("No disks found.\n"),
            Vec::<String>::new()
        );
    }

    #[test]
    fn whole_disk_identifier_strips_partition_suffix() {
        assert_eq!(whole_disk_identifier("disk4s1"), "disk4");
        assert_eq!(whole_disk_identifier("disk10s2"), "disk10");
    }

    #[test]
    fn whole_disk_identifier_leaves_unpartitioned_disk_unchanged() {
        // Regression test: an unpartitioned "superfloppy"-formatted drive
        // has a bare whole-disk identifier with no "sM" suffix at all —
        // the previous unconditional trim-trailing-digits-then-'s' logic
        // mangled "disk4" into the invalid "disk".
        assert_eq!(whole_disk_identifier("disk4"), "disk4");
        assert_eq!(whole_disk_identifier("disk10"), "disk10");
    }

    const PROC_MOUNTS_SAMPLE: &str = "\
/dev/sda1 / ext4 rw,relatime 0 0
/dev/sdb1 /media/user/FOB exfat rw,nosuid,nodev,relatime 0 0
tmpfs /tmp tmpfs rw 0 0
";

    #[test]
    fn finds_mount_point_for_matching_device() {
        assert_eq!(
            find_mount_point(PROC_MOUNTS_SAMPLE, "sdb1"),
            Some(PathBuf::from("/media/user/FOB"))
        );
    }

    #[test]
    fn finds_no_mount_point_for_unknown_device() {
        assert_eq!(find_mount_point(PROC_MOUNTS_SAMPLE, "sdz1"), None);
    }

    #[test]
    fn ignores_malformed_mounts_lines() {
        assert_eq!(find_mount_point("garbage-line-no-spaces", "sdb1"), None);
    }

    #[test]
    fn finds_mount_point_via_whole_disk_name_matching_a_partition() {
        // enumerate_linux() passes the whole-disk /sys/block name ("sdb"),
        // not a specific partition — must match /dev/sdb1 via that name.
        assert_eq!(
            find_mount_point(PROC_MOUNTS_SAMPLE, "sdb"),
            Some(PathBuf::from("/media/user/FOB"))
        );
    }

    #[test]
    fn whole_disk_name_does_not_substring_match_an_unrelated_device() {
        // "sdb" must not match "/dev/sdbx1" (a different, unrelated device
        // that merely starts with the same characters) via a bare
        // substring/prefix check with no boundary.
        let mounts = "/dev/sdbx1 /mnt/other ext4 rw 0 0\n";
        assert_eq!(find_mount_point(mounts, "sdb"), None);
    }

    #[test]
    fn decodes_octal_escaped_spaces_in_mount_path() {
        // Real /proc/mounts octal-escapes spaces in the mount path as
        // \040 — a volume literally named "FOB BACKUP" (or any
        // Windows-default label with a space) must still resolve to a
        // real, existing-on-disk path, not a literal "\040".
        let mounts = "/dev/sdb1 /media/user/FOB\\040BACKUP exfat rw 0 0\n";
        assert_eq!(
            find_mount_point(mounts, "sdb1"),
            Some(PathBuf::from("/media/user/FOB BACKUP"))
        );
    }

    #[test]
    fn decode_mounts_escapes_handles_all_four_kernel_escapes() {
        assert_eq!(decode_mounts_escapes("a\\040b"), "a b");
        assert_eq!(decode_mounts_escapes("a\\011b"), "a\tb");
        assert_eq!(decode_mounts_escapes("a\\012b"), "a\nb");
        assert_eq!(decode_mounts_escapes("a\\134b"), "a\\b");
        assert_eq!(decode_mounts_escapes("no escapes here"), "no escapes here");
    }

    #[test]
    fn prefers_the_partition_that_already_has_a_vault_when_disk_has_several() {
        // A disk with multiple mounted partitions (sdb1, sdb2) must not
        // just silently return whichever happens to appear first in
        // /proc/mounts with no regard for which one is actually the vault.
        let dir = std::env::temp_dir().join(format!(
            "fob-device-test-{}-{}",
            std::process::id(),
            "prefers_vault_partition"
        ));
        let empty = dir.join("empty_partition");
        let has_vault = dir.join("vault_partition");
        std::fs::create_dir_all(&empty).unwrap();
        std::fs::create_dir_all(&has_vault).unwrap();
        std::fs::write(
            has_vault.join("vault.fob"),
            b"not a real vault, just a marker",
        )
        .unwrap();

        let mounts = format!(
            "/dev/sdb1 {} ext4 rw 0 0\n/dev/sdb2 {} exfat rw 0 0\n",
            empty.display(),
            has_vault.display()
        );

        assert_eq!(find_mount_point(&mounts, "sdb"), Some(has_vault.clone()));

        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn detects_usb_bus_component_in_resolved_sysfs_path() {
        // A real /sys/block/sdb symlink target for a USB-attached device.
        let usb_path =
            "../devices/pci0000:00/0000:00:14.0/usb1/1-1/1-1:1.0/host0/target0:0:0/0:0:0:0/block/sdb";
        assert!(resolved_path_indicates_usb(usb_path));
    }

    #[test]
    fn does_not_flag_internal_sata_path_as_usb() {
        let sata_path =
            "../devices/pci0000:00/0000:00:17.0/ata1/host0/target0:0:0/0:0:0:0/block/sda";
        assert!(!resolved_path_indicates_usb(sata_path));
    }

    #[test]
    fn does_not_falsely_match_a_component_that_merely_starts_with_usb() {
        // "usbfoo" is not "usb" + a bus number — must not match.
        assert!(!resolved_path_indicates_usb("../devices/usbfoo/block/sdb"));
    }

    #[test]
    fn system_disk_heuristic_matches_bare_sda_only() {
        assert!(is_likely_system_disk_linux("sda"));
        assert!(!is_likely_system_disk_linux("sda1")); // a partition, not the whole disk
        assert!(!is_likely_system_disk_linux("sdb"));
    }

    #[test]
    fn size_display_formats_gb_and_mb() {
        let gb_dev = UsbDevice {
            name: "x".into(),
            size_bytes: 16 * 1_073_741_824,
            path: PathBuf::new(),
            disk_node: "disk4".into(),
            serial: None,
            has_fob_vault: false,
        };
        assert_eq!(gb_dev.size_display(), "16.0 GB");

        let mb_dev = UsbDevice {
            size_bytes: 512 * 1_048_576,
            ..gb_dev
        };
        assert_eq!(mb_dev.size_display(), "512.0 MB");
    }

    #[test]
    fn is_system_drive_matches_known_boot_volumes() {
        let make = |path: &str| UsbDevice {
            name: "x".into(),
            size_bytes: 0,
            path: PathBuf::from(path),
            disk_node: "disk1".into(),
            serial: None,
            has_fob_vault: false,
        };
        assert!(make("/").is_system_drive());
        assert!(make("/Volumes/Macintosh HD").is_system_drive());
        assert!(make("/Volumes/Macintosh HD - Data").is_system_drive());
        assert!(!make("/Volumes/FOB").is_system_drive());
    }
}
