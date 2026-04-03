use crate::config::{HardwareConfig, default_hardware_config_path};
use crate::discovery::enumerate_candidates;
use crate::runtime;
use anyhow::Result;

use super::shared::exact_device_config;

/// Lists detected devices and shows the matching hardware label when one exists.
pub(crate) fn run() -> Result<()> {
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
