use crate::runtime::run_probe;
use anyhow::{Context, Result};
use std::env;

/// Reads raw events from one device to verify which interface carries the keys.
pub(crate) fn run() -> Result<()> {
    let args = env::args().skip(2).collect::<Vec<_>>();
    let path = args
        .first()
        .context("usage: firemap probe /dev/input/eventX [--no-grab]")?;
    let grab = !args.iter().any(|arg| arg == "--no-grab");
    run_probe(path, grab)
}
