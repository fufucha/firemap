mod devices;
mod hardware;
mod inspect;
mod keycodes;
mod probe;
mod run;
mod shared;

use anyhow::{Result, bail};
use std::env;

pub fn dispatch() -> Result<()> {
    match env::args().nth(1).as_deref() {
        Some("inspect") => inspect::run(),
        Some("devices") => devices::run(),
        Some("hardware") => hardware::run(),
        Some("probe") => probe::run(),
        Some("keycodes") => keycodes::run(),
        Some("run") | None => run::run(),
        Some(other) => {
            bail!(
                "unknown subcommand '{other}', use 'run', 'inspect', 'devices', 'hardware', 'probe' or 'keycodes'"
            )
        }
    }
}
