use crate::keycodes::parse_key_code;
use anyhow::{Context, Result, bail};
use evdev::KeyCode;
use serde::Deserialize;
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Clone)]
pub struct UsbSelector {
    pub vendor_id: String,
    pub product_id: String,
    pub interface_number: u8,
    pub serial: Option<String>,
}

#[derive(Clone)]
pub struct HardwareDeviceConfig {
    pub label: String,
    pub expected_name: String,
    pub selector: UsbSelector,
}

#[derive(Clone)]
pub struct DeviceConfig {
    pub label: String,
    pub expected_name: String,
    pub selector: UsbSelector,
    pub mapping: Vec<MappingEntry>,
}

#[derive(Clone)]
pub struct MappingEntry {
    pub input: KeyCode,
    pub outputs: Vec<KeyCode>,
    pub fire_ms: Option<u64>,
}

pub struct LoadedProfile {
    pub profile_name: String,
    pub devices: Vec<DeviceConfig>,
}

pub struct HardwareConfig {
    pub devices: Vec<HardwareDeviceConfig>,
}

#[derive(Deserialize)]
struct HardwareFile {
    device: Vec<HardwareFileDevice>,
}

#[derive(Deserialize)]
struct HardwareFileDevice {
    label: String,
    expected_name: String,
    vendor_id: String,
    product_id: String,
    interface_number: u8,
    serial: Option<String>,
}

#[derive(Deserialize)]
struct ProfileFile {
    profile: Option<String>,
    device: Vec<ProfileFileDevice>,
}

#[derive(Deserialize)]
struct ProfileFileDevice {
    label: String,
    mapping: Vec<FileMappingEntry>,
}

#[derive(Deserialize)]
struct FileMappingEntry {
    input: String,
    output: Option<String>,
    outputs: Option<Vec<String>>,
    fire_ms: Option<u64>,
}

impl HardwareConfig {
    /// Loads the stable hardware config used to find the same physical devices.
    pub fn load(path: &Path) -> Result<Self> {
        let raw = fs::read_to_string(path)
            .with_context(|| format!("failed to read hardware config {}", path.display()))?;
        let parsed: HardwareFile =
            toml::from_str(&raw).with_context(|| format!("failed to parse {}", path.display()))?;

        let devices = parsed
            .device
            .into_iter()
            .map(|device| HardwareDeviceConfig {
                label: device.label,
                expected_name: device.expected_name,
                selector: UsbSelector {
                    vendor_id: device.vendor_id.to_ascii_lowercase(),
                    product_id: device.product_id.to_ascii_lowercase(),
                    interface_number: device.interface_number,
                    serial: device.serial.map(|serial| serial.to_ascii_lowercase()),
                },
            })
            .collect();

        Ok(Self { devices })
    }
}

impl LoadedProfile {
    /// Merges one profile with hardware data and resolves configured key names.
    /// Each profile device must match a hardware label from the stable config.
    pub fn load(hardware_path: &Path, profile_path: &Path) -> Result<Self> {
        let hardware = HardwareConfig::load(hardware_path)?;
        let raw = fs::read_to_string(profile_path)
            .with_context(|| format!("failed to read profile {}", profile_path.display()))?;
        let parsed: ProfileFile = toml::from_str(&raw)
            .with_context(|| format!("failed to parse {}", profile_path.display()))?;

        let mut hardware_by_label = hardware
            .devices
            .into_iter()
            .map(|device| (device.label.clone(), device))
            .collect::<HashMap<_, _>>();

        let mut devices = Vec::new();
        for profile_device in parsed.device {
            let Some(hardware_device) = hardware_by_label.remove(&profile_device.label) else {
                bail!(
                    "profile device '{}' is missing from hardware config",
                    profile_device.label
                );
            };

            let mapping = profile_device
                .mapping
                .into_iter()
                .map(|entry| {
                    let outputs = match (entry.output, entry.outputs) {
                        (Some(output), None) => vec![parse_key_code(&output)?],
                        (None, Some(outputs)) => outputs
                            .into_iter()
                            .map(|key| parse_key_code(&key))
                            .collect::<Result<Vec<_>>>()?,
                        (Some(_), Some(_)) => {
                            bail!("mapping entries must use either `output` or `outputs`, not both")
                        }
                        (None, None) => {
                            bail!("mapping entries must define `output` or `outputs`")
                        }
                    };
                    Ok(MappingEntry {
                        input: parse_key_code(&entry.input)?,
                        outputs,
                        fire_ms: entry.fire_ms,
                    })
                })
                .collect::<Result<Vec<_>>>()?;

            devices.push(DeviceConfig {
                label: hardware_device.label,
                expected_name: hardware_device.expected_name,
                selector: hardware_device.selector,
                mapping,
            });
        }

        Ok(Self {
            profile_name: parsed
                .profile
                .unwrap_or_else(|| profile_name_from_path(profile_path)),
            devices,
        })
    }
}

/// Derives the profile name from the file when the `profile` field is missing.
fn profile_name_from_path(path: &Path) -> String {
    path.file_stem()
        .and_then(|stem| stem.to_str())
        .unwrap_or("default")
        .to_string()
}

/// Returns the canonical path to the repository hardware config.
pub fn default_hardware_config_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("config/hardware.toml")
}

/// Builds the path for a named profile in `config/profiles`.
pub fn profile_path(profile_name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("config")
        .join("profiles")
        .join(format!("{profile_name}.toml"))
}
