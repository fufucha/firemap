use crate::config::{HardwareConfig, default_hardware_config_path};
use crate::discovery::enumerate_candidates;
use crate::runtime;
use anyhow::Result;

use super::shared::{exact_device_config, explain_matches};

/// Prints detected devices and explains how each hardware label resolves.
pub(crate) fn run() -> Result<()> {
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
