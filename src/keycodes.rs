use anyhow::{Context, Result};
use evdev::KeyCode;
use std::str::FromStr;

/// Converts an `evdev` name into a `KeyCode` with a clearer error message.
pub fn parse_key_code(name: &str) -> Result<KeyCode> {
    KeyCode::from_str(name).with_context(|| format!("unsupported key code in config: {name}"))
}
