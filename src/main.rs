mod commands;
mod config;
mod discovery;
mod keycodes;
mod runtime;

use anyhow::Result;

fn main() -> Result<()> {
    commands::dispatch()
}
