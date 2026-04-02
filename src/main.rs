mod config;
mod discovery;
mod keycodes;
mod runtime;

use anyhow::{Context, Result, bail};
use config::{DeviceConfig, HardwareConfig, default_hardware_config_path, profile_path};
use discovery::{CandidateDevice, enumerate_candidates};
use runtime::{run, run_keycodes, run_probe};
use std::env;

fn main() -> Result<()> {
    match env::args().nth(1).as_deref() {
        Some("inspect") => inspect_mode(),
        Some("devices") => devices_mode(),
        Some("probe") => probe_mode(),
        Some("keycodes") => keycodes_mode(),
        Some("run") | None => run_mode(),
        Some(other) => {
            bail!(
                "unknown subcommand '{other}', use 'run', 'inspect', 'devices', 'probe' or 'keycodes'"
            )
        }
    }
}

/// Prints detected devices and explains how each hardware label resolves.
fn inspect_mode() -> Result<()> {
    let hardware = HardwareConfig::load(&default_hardware_config_path())?;
    let candidates = enumerate_candidates()?;
    if candidates.is_empty() {
        println!("[inspect] no /dev/input/event* nodes found");
    }

    for candidate in &candidates {
        println!("{}", candidate.describe());
    }

    for device in &hardware.devices {
        let exact_device = exact_device_config(device);
        println!(
            "[match:{}] {}",
            device.label,
            explain_matches(&candidates, device)
        );
        match runtime::select_device(&candidates, &exact_device) {
            Ok(candidate) => println!(
                "[selected:{}] {} ({})",
                device.label,
                candidate.devnode.display(),
                candidate.name
            ),
            Err(error) => println!("[selected:{}] {error}", device.label),
        }
    }

    Ok(())
}

/// Loads the requested profile and starts the remapping workers.
fn run_mode() -> Result<()> {
    let profile = env::args().nth(2).unwrap_or_else(|| "ffxiv".to_string());
    let loaded =
        config::LoadedProfile::load(&default_hardware_config_path(), &profile_path(&profile))?;
    eprintln!("  [profile] {}", loaded.profile_name);
    run(&loaded)
}

/// Lists detected devices and shows the matching hardware label when one exists.
fn devices_mode() -> Result<()> {
    let hardware = HardwareConfig::load(&default_hardware_config_path())?;
    let candidates = enumerate_candidates()?;
    for candidate in candidates {
        let matched_label = hardware
            .devices
            .iter()
            .find(|device| {
                runtime::select_device(&[candidate.clone()], &exact_device_config(device)).is_ok()
            })
            .map(|device| device.label.as_str());

        match matched_label {
            Some(label) => println!("{} | label={}", candidate.short_display(), label),
            None => println!("{}", candidate.short_display()),
        }
    }
    Ok(())
}

/// Reads raw events from one device to verify which interface carries the keys.
fn probe_mode() -> Result<()> {
    let args = env::args().skip(2).collect::<Vec<_>>();
    let path = args
        .first()
        .context("usage: firemap probe /dev/input/eventX [--no-grab]")?;
    let grab = !args.iter().any(|arg| arg == "--no-grab");
    run_probe(path, grab)
}

/// Resolves a label or path and prints the observed key codes.
fn keycodes_mode() -> Result<()> {
    let args = env::args().skip(2).collect::<Vec<_>>();
    let target = args
        .first()
        .context("usage: firemap keycodes <Label|/dev/input/eventN> [--no-grab]")?;
    let grab = !args.iter().any(|arg| arg == "--no-grab");
    let path = resolve_device_target(&target)?;
    run_keycodes(&path, grab)
}

/// Explains why each candidate does or does not match one hardware entry.
fn explain_matches(
    candidates: &[CandidateDevice],
    device: &config::HardwareDeviceConfig,
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
fn resolve_device_target(target: &str) -> Result<String> {
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
fn exact_device_config(device: &config::HardwareDeviceConfig) -> DeviceConfig {
    DeviceConfig {
        label: device.label.clone(),
        expected_name: device.expected_name.clone(),
        selector: device.selector.clone(),
        mapping: Vec::new(),
    }
}
