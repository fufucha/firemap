use crate::runtime::run_keycodes;
use anyhow::{Context, Result};
use std::env;

use super::shared::resolve_device_target;

/// Resolves a label or path and prints the observed key codes.
pub(crate) fn run() -> Result<()> {
    let args = env::args().skip(2).collect::<Vec<_>>();
    let target = args
        .first()
        .context("usage: firemap keycodes <Label|/dev/input/eventN> [--no-grab]")?;
    let grab = !args.iter().any(|arg| arg == "--no-grab");
    let path = resolve_device_target(target)?;
    run_keycodes(&path, grab)
}
