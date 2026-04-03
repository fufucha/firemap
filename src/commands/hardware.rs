use crate::discovery::{CandidateDevice, enumerate_candidates};
use anyhow::Result;
use std::cmp::Reverse;
use std::collections::BTreeMap;
use std::env;

/// Prints likely `hardware.toml` blocks for detected input devices.
pub(crate) fn run() -> Result<()> {
    let show_all = env::args().skip(2).any(|arg| arg == "--all");
    let mut candidates = enumerate_candidates()?;
    candidates.sort_by_key(|candidate| {
        (
            !has_complete_hardware_identity(candidate),
            candidate.supported_keys.is_none(),
            candidate.name.to_ascii_lowercase(),
            candidate.devnode.clone(),
        )
    });

    let printed_any = if show_all {
        print_all_hardware_candidates(&candidates)
    } else {
        print_condensed_hardware_candidates(&candidates)
    };

    if !printed_any {
        println!("# no candidates with vendor_id, product_id, and interface_number");
    }

    Ok(())
}

/// Returns true when a candidate exposes the fields required by `hardware.toml`.
fn has_complete_hardware_identity(candidate: &CandidateDevice) -> bool {
    candidate.usb.vendor_id.is_some()
        && candidate.usb.product_id.is_some()
        && candidate.usb.interface_number.is_some()
}

/// Prints one starter block per physical device.
fn print_condensed_hardware_candidates(candidates: &[CandidateDevice]) -> bool {
    let mut groups: BTreeMap<String, Vec<&CandidateDevice>> = BTreeMap::new();
    for candidate in candidates {
        if !has_complete_hardware_identity(candidate) {
            continue;
        }
        groups
            .entry(hardware_group_key(candidate))
            .or_default()
            .push(candidate);
    }

    let mut printed_any = false;
    for group in groups.values() {
        let mut sorted = group.clone();
        sorted.sort_by_key(|candidate| hardware_preference_key(candidate));

        printed_any = true;
        println!("{}", hardware_block(sorted[0]));
        println!();
    }

    printed_any
}

/// Prints every complete candidate block without grouping interfaces together.
fn print_all_hardware_candidates(candidates: &[CandidateDevice]) -> bool {
    let mut printed_any = false;
    for candidate in candidates {
        if !has_complete_hardware_identity(candidate) {
            continue;
        }

        printed_any = true;
        println!("{}", hardware_block(candidate));
        println!();
    }

    printed_any
}

/// Formats one detected device as a starter block for `hardware.toml`.
fn hardware_block(candidate: &CandidateDevice) -> String {
    let mut block = String::new();
    block.push_str(&format!("# {}\n", candidate.devnode.display()));
    block.push_str("[[device]]\n");
    block.push_str(&format!("label = {:?}\n", candidate.name));
    block.push_str(&format!("expected_name = {:?}\n", candidate.name));
    block.push_str(&format!(
        "vendor_id = {:?}\n",
        candidate.usb.vendor_id.as_deref().unwrap_or("")
    ));
    block.push_str(&format!(
        "product_id = {:?}\n",
        candidate.usb.product_id.as_deref().unwrap_or("")
    ));
    block.push_str(&format!(
        "interface_number = {}\n",
        candidate.usb.interface_number.unwrap_or_default()
    ));
    block
}

/// Groups multiple interfaces that appear to belong to the same physical device.
fn hardware_group_key(candidate: &CandidateDevice) -> String {
    let vendor_id = candidate.usb.vendor_id.as_deref().unwrap_or("");
    let product_id = candidate.usb.product_id.as_deref().unwrap_or("");
    let serial = candidate.usb.serial.as_deref().unwrap_or("");
    let port = candidate
        .id_path
        .as_deref()
        .map(strip_interface_suffix)
        .unwrap_or("unknown");

    format!(
        "{}|{}|{}|{}|{}",
        candidate.name, vendor_id, product_id, serial, port
    )
}

/// Ranks candidates so the most plausible key carrying interface is shown first.
fn hardware_preference_key(
    candidate: &CandidateDevice,
) -> (u8, u8, u8, u8, Reverse<usize>, Reverse<usize>, u8, String) {
    let caps = candidate.capabilities.as_ref().cloned().unwrap_or_default();
    let has_keyboard_keys = caps.keyboard_keys > 0;
    let has_pointing_axes = caps.relative_axes > 0 || caps.absolute_axes > 0;
    let button_dominated = caps.button_keys > caps.keyboard_keys;
    let has_keyboard_features = caps.leds > 0 || caps.has_repeat;
    let interface_number = candidate.usb.interface_number.unwrap_or(u8::MAX);

    (
        (!has_keyboard_keys) as u8,
        has_pointing_axes as u8,
        button_dominated as u8,
        (!has_keyboard_features) as u8,
        Reverse(caps.keyboard_keys),
        Reverse(caps.total_keys),
        interface_number,
        candidate.devnode.display().to_string(),
    )
}

/// Removes the trailing interface part from an `ID_PATH` style string.
fn strip_interface_suffix(id_path: &str) -> &str {
    id_path
        .rsplit_once('.')
        .map(|(prefix, _)| prefix)
        .unwrap_or(id_path)
}
