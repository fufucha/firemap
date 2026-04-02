use anyhow::{Context, Result};
use evdev::{Device, KeyCode};
use std::collections::HashSet;
use std::ffi::OsStr;
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Clone, Debug, Default)]
pub struct UsbIdentity {
    pub vendor_id: Option<String>,
    pub product_id: Option<String>,
    pub interface_number: Option<u8>,
    pub serial: Option<String>,
}

#[derive(Clone, Debug)]
pub struct CandidateDevice {
    pub devnode: PathBuf,
    pub sysname: String,
    pub name: String,
    pub phys: Option<String>,
    pub id_path: Option<String>,
    pub usb: UsbIdentity,
    pub open_error: Option<String>,
    pub supported_keys: Option<HashSet<KeyCode>>,
}

impl CandidateDevice {
    /// Produces a short line suitable for the `devices` command.
    pub fn short_display(&self) -> String {
        format!(
            "{} | {} | {} | usb={}:{} if={:?}",
            self.devnode.display(),
            self.name,
            self.sysname,
            self.usb.vendor_id.as_deref().unwrap_or("-"),
            self.usb.product_id.as_deref().unwrap_or("-"),
            self.usb.interface_number
        )
    }

    /// Produces a detailed summary for device selection diagnostics.
    pub fn describe(&self) -> String {
        let key_summary = match &self.supported_keys {
            Some(keys) => format!("{} keys readable", keys.len()),
            None => match &self.open_error {
                Some(error) => format!("evdev unreadable: {error}"),
                None => "evdev readable, no key bitmap".to_string(),
            },
        };

        format!(
            "[{}] {} | name='{}' usb={}:{} if={:?} serial={:?} path={:?} phys={:?} | {}",
            self.sysname,
            self.devnode.display(),
            self.name,
            self.usb.vendor_id.as_deref().unwrap_or("-"),
            self.usb.product_id.as_deref().unwrap_or("-"),
            self.usb.interface_number,
            self.usb.serial,
            self.id_path,
            self.phys,
            key_summary
        )
    }
}

/// Scans `event` nodes and collects the metadata needed for strict selection.
pub fn enumerate_candidates() -> Result<Vec<CandidateDevice>> {
    let mut candidates = Vec::new();
    for entry in fs::read_dir("/dev/input").context("failed to read /dev/input")? {
        let entry = entry.context("failed to read an entry from /dev/input")?;
        let devnode = entry.path();
        let Some(file_name) = devnode.file_name() else {
            continue;
        };
        if !file_name.to_string_lossy().starts_with("event") {
            continue;
        }

        let sysname = file_name.to_string_lossy().into_owned();
        let udev_device =
            udev::Device::from_subsystem_sysname("input".to_string(), sysname.clone()).ok();
        let usb = udev_device
            .as_ref()
            .map(read_usb_identity)
            .transpose()?
            .unwrap_or_default();

        let mut candidate = CandidateDevice {
            devnode,
            sysname,
            name: String::new(),
            phys: None,
            id_path: udev_device
                .as_ref()
                .and_then(|device| read_property(device, "ID_PATH")),
            usb,
            open_error: None,
            supported_keys: None,
        };

        match Device::open(&candidate.devnode) {
            Ok(device) => {
                candidate.name = device.name().unwrap_or("unknown").to_string();
                candidate.phys = device.physical_path().map(ToOwned::to_owned);
                candidate.supported_keys = device
                    .supported_keys()
                    .map(|keys| keys.iter().collect::<HashSet<_>>());
            }
            Err(error) => {
                candidate.open_error = Some(error.to_string());
            }
        }

        if candidate.name.is_empty() {
            candidate.name = udev_device
                .as_ref()
                .and_then(|device| read_property_preserve_case(device, "NAME"))
                .or_else(|| {
                    udev_device
                        .as_ref()
                        .and_then(|device| read_attribute_chain_preserve_case(device, "name"))
                })
                .unwrap_or_else(|| candidate.sysname.clone());
        }

        candidates.push(candidate);
    }

    candidates.sort_by(|left, right| compare_event_nodes(&left.devnode, &right.devnode));
    Ok(candidates)
}

/// Walks the udev tree to read the stable USB identity for one device.
fn read_usb_identity(input_device: &udev::Device) -> Result<UsbIdentity> {
    let usb_device = input_device
        .parent_with_subsystem_devtype("usb", "usb_device")
        .context("failed to locate usb_device parent")?;
    let usb_interface = input_device
        .parent_with_subsystem_devtype("usb", "usb_interface")
        .context("failed to locate usb_interface parent")?;

    Ok(UsbIdentity {
        vendor_id: usb_device
            .as_ref()
            .and_then(|device| read_attribute(device, "idVendor")),
        product_id: usb_device
            .as_ref()
            .and_then(|device| read_attribute(device, "idProduct")),
        interface_number: usb_interface
            .as_ref()
            .and_then(|device| read_attribute(device, "bInterfaceNumber"))
            .and_then(|value| parse_hex_u8(&value)),
        serial: usb_device
            .as_ref()
            .and_then(|device| read_attribute(device, "serial")),
    })
}

fn read_property(device: &udev::Device, key: &str) -> Option<String> {
    device.property_value(key).map(osstr_to_string)
}

fn read_property_preserve_case(device: &udev::Device, key: &str) -> Option<String> {
    device.property_value(key).map(osstr_to_trimmed_string)
}

fn read_attribute(device: &udev::Device, key: &str) -> Option<String> {
    device.attribute_value(key).map(osstr_to_string)
}

/// Searches parent devices when an attribute is missing on the current node.
fn read_attribute_chain_preserve_case(device: &udev::Device, key: &str) -> Option<String> {
    if let Some(value) = device.attribute_value(key) {
        return Some(osstr_to_trimmed_string(value));
    }

    let mut current = device.parent();
    while let Some(parent) = current {
        if let Some(value) = parent.attribute_value(key) {
            return Some(osstr_to_trimmed_string(value));
        }
        current = parent.parent();
    }

    None
}

/// Normalizes udev values for case insensitive comparisons.
fn osstr_to_string(value: &OsStr) -> String {
    osstr_to_trimmed_string(value).to_ascii_lowercase()
}

fn osstr_to_trimmed_string(value: &OsStr) -> String {
    value.to_string_lossy().trim().trim_matches('"').to_string()
}

fn parse_hex_u8(value: &str) -> Option<u8> {
    u8::from_str_radix(value.trim_start_matches("0x"), 16).ok()
}

/// Sorts `eventN` paths by number to keep output stable.
fn compare_event_nodes(left: &Path, right: &Path) -> std::cmp::Ordering {
    match (event_number(left), event_number(right)) {
        (Some(left_num), Some(right_num)) => left_num.cmp(&right_num),
        _ => left.cmp(right),
    }
}

/// Extracts the trailing number from `eventN` when the name matches that form.
fn event_number(path: &Path) -> Option<u32> {
    path.file_name()
        .and_then(|name| name.to_str())
        .and_then(|name| name.strip_prefix("event"))
        .and_then(|suffix| suffix.parse::<u32>().ok())
}
