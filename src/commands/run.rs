use crate::config::{self, default_hardware_config_path, profile_path};
use crate::runtime::run as runtime_run;
use anyhow::Result;
use std::env;

/// Loads the requested profile and starts the remapping workers.
pub(crate) fn run() -> Result<()> {
    let profile = env::args().nth(2).unwrap_or_else(|| "ffxiv".to_string());
    let loaded =
        config::LoadedProfile::load(&default_hardware_config_path(), &profile_path(&profile))?;
    eprintln!("  [profile] {}", loaded.profile_name);
    runtime_run(&loaded)
}
