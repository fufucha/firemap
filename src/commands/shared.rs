use crate::config::{
    DeviceConfig, HardwareConfig, HardwareDeviceConfig, default_hardware_config_path,
};
use crate::discovery::{CandidateDevice, enumerate_candidates};
use crate::runtime;
use anyhow::{Context, Result};

/// Explains why each candidate does or does not match one hardware entry.
pub(crate) fn explain_matches(
    candidates: &[CandidateDevice],
    device: &HardwareDeviceConfig,
) -> String {
    let mut lines = Vec::new();

    for candidate in candidates {
        let mut reasons = Vec::new();
        if candidate.name != device.expected_name {
            reasons.push(format!("name mismatch: '{}'", candidate.name));
        }
        if candidate.usb.vendor_id.as_deref() != Some(device.selector.vendor_id.as_str()) {
            reasons.push(format!("vendor mismatch: {:?}", candidate.usb.vendor_id));
        }
        if candidate.usb.product_id.as_deref() != Some(device.selector.product_id.as_str()) {
            reasons.push(format!("product mismatch: {:?}", candidate.usb.product_id));
        }
        if candidate.usb.interface_number != Some(device.selector.interface_number) {
            reasons.push(format!(
                "interface mismatch: {:?}",
                candidate.usb.interface_number
            ));
        }
        if let Some(serial) = device.selector.serial.as_deref()
            && candidate.usb.serial.as_deref() != Some(serial)
        {
            reasons.push(format!("serial mismatch: {:?}", candidate.usb.serial));
        }

        if reasons.is_empty() {
            lines.push(format!("{} accepted", candidate.devnode.display()));
        } else {
            lines.push(format!(
                "{} rejected: {}",
                candidate.devnode.display(),
                reasons.join(" | ")
            ));
        }
    }

    if lines.is_empty() {
        "no candidates".to_string()
    } else {
        lines.join("\n")
    }
}

/// Accepts either a direct path or a label defined in the hardware config.
pub(crate) fn resolve_device_target(target: &str) -> Result<String> {
    if target.starts_with("/dev/input/event") {
        return Ok(target.to_string());
    }

    let hardware = HardwareConfig::load(&default_hardware_config_path())?;
    let candidates = enumerate_candidates()?;
    let device = hardware
        .devices
        .iter()
        .find(|device| device.label == target)
        .with_context(|| format!("unknown device label '{target}' in hardware config"))?;

    let exact_device = exact_device_config(device);
    let candidate = runtime::select_device(&candidates, &exact_device)?;
    Ok(candidate.devnode.display().to_string())
}

/// Converts one hardware entry into a strict selector without profile mappings.
pub(crate) fn exact_device_config(device: &HardwareDeviceConfig) -> DeviceConfig {
    DeviceConfig {
        label: device.label.clone(),
        expected_name: device.expected_name.clone(),
        selector: device.selector.clone(),
        mapping: Vec::new(),
    }
}
