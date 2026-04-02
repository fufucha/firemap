use crate::config::{DeviceConfig, LoadedProfile, MappingEntry};
use crate::discovery::{CandidateDevice, enumerate_candidates};
use anyhow::{Context, Result, bail};
use evdev::{AttributeSet, Device, EventSummary, InputEvent, KeyCode, uinput::VirtualDevice};
use std::collections::HashMap;
use std::path::Path;
use std::sync::{
    Arc, Mutex,
    atomic::{AtomicBool, Ordering},
};
use std::thread;
use std::time::Duration;

const TAP_DURATION: Duration = Duration::from_millis(10);
const COMBO_STEP_DELAY: Duration = Duration::from_millis(5);

#[derive(Clone)]
struct WorkerConfig {
    label: String,
    devnode: std::path::PathBuf,
    device_name: String,
    mapping: Vec<MappingEntry>,
}

#[derive(Clone, Copy)]
struct ActionBinding<'a> {
    outputs: &'a [KeyCode],
    fire_ms: Option<u64>,
}

pub fn run(profile: &LoadedProfile) -> Result<()> {
    let candidates = enumerate_candidates()?;
    let keyboard = Arc::new(Mutex::new(create_virtual_keyboard(profile)?));
    let mut workers = Vec::new();

    for device in &profile.devices {
        let candidate = select_device(&candidates, device)?;
        workers.push(WorkerConfig {
            label: device.label.clone(),
            devnode: candidate.devnode,
            device_name: candidate.name,
            mapping: device.mapping.clone(),
        });
    }

    let mut handles = Vec::new();
    for worker in workers {
        let keyboard = Arc::clone(&keyboard);
        handles.push(thread::spawn(move || run_device_loop(worker, keyboard)));
    }

    for handle in handles {
        match handle.join() {
            Ok(result) => result?,
            Err(_) => bail!("an input worker thread panicked"),
        }
    }

    Ok(())
}

/// Reads one raw device and prints each keyboard event it receives.
pub fn run_probe(path: &str, grab: bool) -> Result<()> {
    let path = validate_input_device_path(path, "probe")?;
    let mut device = Device::open(path)
        .with_context(|| format!("failed to open probe device at {}", path.display()))?;
    if grab {
        device
            .grab()
            .with_context(|| format!("failed to grab probe device at {}", path.display()))?;
    }

    println!(
        "[probe] reading {} ({})",
        path.display(),
        device.name().unwrap_or("unknown")
    );
    if grab {
        println!("[probe] device grabbed");
    }

    loop {
        for event in device.fetch_events()? {
            if let EventSummary::Key(_, code, value) = event.destructure() {
                println!("key {:?} value={}", code, value);
            }
        }
    }
}

/// Prints key codes on key press and can exit on `KEY_ESC` when grabbed.
pub fn run_keycodes(path: &str, grab: bool) -> Result<()> {
    let path = validate_input_device_path(path, "keycodes")?;
    let mut device = Device::open(path)
        .with_context(|| format!("failed to open keycode reader at {}", path.display()))?;
    if grab {
        device
            .grab()
            .with_context(|| format!("failed to grab keycode reader at {}", path.display()))?;
    }

    println!(
        "[keycodes] reading {} ({})",
        path.display(),
        device.name().unwrap_or("unknown")
    );
    if grab {
        println!("[keycodes] device grabbed");
        println!("[keycodes] press keys and use KEY_ESC on that device to quit");
    } else {
        println!("[keycodes] press keys and stop with Ctrl+C");
    }

    loop {
        for event in device.fetch_events()? {
            if let EventSummary::Key(_, code, value) = event.destructure()
                && value == 1
            {
                if grab && code == KeyCode::KEY_ESC {
                    println!("[keycodes] exiting on KEY_ESC");
                    return Ok(());
                }
                println!("{:?} ({})", code, code.code());
            }
        }
    }
}

/// Rejects placeholder or missing paths before later code fails less clearly.
fn validate_input_device_path<'a>(path: &'a str, command: &str) -> Result<&'a Path> {
    let path = Path::new(path);
    if path == Path::new("/dev/input/eventX") {
        bail!(
            "replace /dev/input/eventX with a real device path, for example: cargo run -- {command} /dev/input/event29"
        );
    }
    if !path.exists() {
        bail!(
            "input device {} does not exist; run `cargo run -- inspect` first and choose a real /dev/input/eventN path",
            path.display()
        );
    }
    Ok(path)
}

/// Selects exactly one candidate matching the expected name and USB identity.
/// This keeps device resolution deterministic even when event numbers change.
pub fn select_device(
    candidates: &[CandidateDevice],
    config: &DeviceConfig,
) -> Result<CandidateDevice> {
    let exact_matches = candidates
        .iter()
        .filter(|candidate| candidate_matches(candidate, config))
        .cloned()
        .collect::<Vec<_>>();

    match exact_matches.as_slice() {
        [candidate] => Ok(candidate.clone()),
        [] => bail!(
            "no exact match for {} ({}:{} if={})",
            config.label,
            config.selector.vendor_id,
            config.selector.product_id,
            config.selector.interface_number
        ),
        matches => {
            let nodes = matches
                .iter()
                .map(|candidate| candidate.devnode.display().to_string())
                .collect::<Vec<_>>()
                .join(", ");
            bail!(
                "multiple exact matches for {} ({}:{} if={}): {}",
                config.label,
                config.selector.vendor_id,
                config.selector.product_id,
                config.selector.interface_number,
                nodes
            )
        }
    }
}

/// Compares one candidate against the strict selector built from config.
fn candidate_matches(candidate: &CandidateDevice, config: &DeviceConfig) -> bool {
    candidate.name == config.expected_name
        && candidate.usb.vendor_id.as_deref() == Some(config.selector.vendor_id.as_str())
        && candidate.usb.product_id.as_deref() == Some(config.selector.product_id.as_str())
        && candidate.usb.interface_number == Some(config.selector.interface_number)
        && match config.selector.serial.as_deref() {
            Some(serial) => candidate.usb.serial.as_deref() == Some(serial),
            None => true,
        }
}

/// Creates the virtual keyboard with the union of all configured outputs.
fn create_virtual_keyboard(profile: &LoadedProfile) -> Result<VirtualDevice> {
    let mut keys = AttributeSet::<KeyCode>::new();
    for device in &profile.devices {
        for entry in &device.mapping {
            for output in &entry.outputs {
                keys.insert(*output);
            }
        }
    }

    VirtualDevice::builder()
        .context("failed to open /dev/uinput")?
        .name("firemap-virtual-kbd")
        .with_keys(&keys)
        .context("failed to configure virtual keyboard keys")?
        .build()
        .context("failed to create virtual keyboard")
}

/// Processes events in a loop for one already resolved physical device.
fn run_device_loop(worker: WorkerConfig, keyboard: Arc<Mutex<VirtualDevice>>) -> Result<()> {
    let mut device = Device::open(&worker.devnode).with_context(|| {
        format!(
            "failed to open {} at {}",
            worker.label,
            worker.devnode.display()
        )
    })?;
    device.grab().with_context(|| {
        format!(
            "failed to grab {} at {}",
            worker.label,
            worker.devnode.display()
        )
    })?;

    eprintln!(
        "  [grabbed:{}] {} ({})",
        worker.label,
        worker.devnode.display(),
        worker.device_name
    );

    let mapping = worker
        .mapping
        .iter()
        .map(|entry| {
            (
                entry.input,
                ActionBinding {
                    outputs: &entry.outputs,
                    fire_ms: entry.fire_ms,
                },
            )
        })
        .collect::<HashMap<_, _>>();
    let mut autofire_threads: HashMap<KeyCode, Arc<AtomicBool>> = HashMap::new();

    loop {
        let events = device.fetch_events().with_context(|| {
            format!(
                "failed while reading events from {} ({})",
                worker.label,
                worker.devnode.display()
            )
        })?;

        for event in events {
            if let EventSummary::Key(_, code, value) = event.destructure() {
                let Some(binding) = mapping.get(&code) else {
                    continue;
                };

                // Autofire owns key repeat timing, so repeated kernel events are ignored there.
                if let Some(fire_ms) = binding.fire_ms {
                    handle_autofire_event(
                        code,
                        binding.outputs,
                        fire_ms,
                        value,
                        &keyboard,
                        &mut autofire_threads,
                    )?;
                } else {
                    handle_binding_event(binding.outputs, value, &keyboard)?;
                }
            }
        }
    }
}

/// Forwards a simple press or emits a full combo depending on the mapping.
/// Combo mappings are emitted as taps, so release and repeat are ignored.
fn handle_binding_event(
    outputs: &[KeyCode],
    value: i32,
    keyboard: &Arc<Mutex<VirtualDevice>>,
) -> Result<()> {
    if outputs.len() > 1 {
        return match value {
            1 => emit_key_tap(keyboard, outputs),
            0 | 2 => Ok(()),
            other => bail!("unsupported key value: {other}"),
        };
    }

    match value {
        0 => emit_keys_reverse(keyboard, outputs, 0),
        1 => emit_keys(keyboard, outputs, 1),
        2 => {
            emit_keys(keyboard, outputs, 2)?;
            Ok(())
        }
        other => bail!("unsupported key value: {other}"),
    }
}

/// Starts and stops an autofire thread while the source key stays pressed.
fn handle_autofire_event(
    source: KeyCode,
    outputs: &[KeyCode],
    fire_ms: u64,
    value: i32,
    keyboard: &Arc<Mutex<VirtualDevice>>,
    autofire_threads: &mut HashMap<KeyCode, Arc<AtomicBool>>,
) -> Result<()> {
    match value {
        1 => {
            if autofire_threads.contains_key(&source) {
                return Ok(());
            }

            let stop = Arc::new(AtomicBool::new(false));
            let stop_for_thread = Arc::clone(&stop);
            let keyboard = Arc::clone(keyboard);
            let outputs = outputs.to_vec();
            thread::spawn(move || {
                // The worker keeps tapping until the release path flips the stop flag.
                while !stop_for_thread.load(Ordering::Relaxed) {
                    if emit_key_tap(&keyboard, &outputs).is_err() {
                        break;
                    }
                    thread::sleep(Duration::from_millis(fire_ms));
                }
            });
            autofire_threads.insert(source, stop);
        }
        0 => {
            if let Some(stop) = autofire_threads.remove(&source) {
                stop.store(true, Ordering::Relaxed);
            }
        }
        2 => {}
        other => bail!("unsupported key value for autofire: {other}"),
    }

    Ok(())
}

/// Emits a combo as a short tap with ordered press and release steps.
/// This helps modifier combos behave like a deliberate chord on a real keyboard.
fn emit_key_tap(keyboard: &Arc<Mutex<VirtualDevice>>, keys: &[KeyCode]) -> Result<()> {
    emit_keys_stepwise(keyboard, keys, 1)?;
    thread::sleep(TAP_DURATION);
    emit_keys_reverse_stepwise(keyboard, keys, 0)
}

/// Sends a batch of events to the virtual keyboard in the given order.
fn emit_keys(keyboard: &Arc<Mutex<VirtualDevice>>, keys: &[KeyCode], value: i32) -> Result<()> {
    let mut device = keyboard
        .lock()
        .map_err(|_| anyhow::anyhow!("virtual keyboard mutex is poisoned"))?;
    let events = keys
        .iter()
        .map(|key| InputEvent::new(evdev::EventType::KEY.0, key.code(), value))
        .collect::<Vec<_>>();
    device.emit(&events).with_context(|| {
        format!(
            "failed to emit {} key(s) with value={value} to virtual keyboard",
            keys.len()
        )
    })?;
    Ok(())
}

/// Presses keys one by one to make combos more reliable.
fn emit_keys_stepwise(
    keyboard: &Arc<Mutex<VirtualDevice>>,
    keys: &[KeyCode],
    value: i32,
) -> Result<()> {
    for key in keys {
        emit_keys(keyboard, &[*key], value)?;
        thread::sleep(COMBO_STEP_DELAY);
    }
    Ok(())
}

/// Releases keys in reverse order to preserve modifier semantics.
fn emit_keys_reverse(
    keyboard: &Arc<Mutex<VirtualDevice>>,
    keys: &[KeyCode],
    value: i32,
) -> Result<()> {
    let mut reversed = keys.to_vec();
    reversed.reverse();
    let mut device = keyboard
        .lock()
        .map_err(|_| anyhow::anyhow!("virtual keyboard mutex is poisoned"))?;
    let events = reversed
        .iter()
        .map(|key| InputEvent::new(evdev::EventType::KEY.0, key.code(), value))
        .collect::<Vec<_>>();
    device.emit(&events).with_context(|| {
        format!(
            "failed to emit {} reversed key(s) with value={value} to virtual keyboard",
            reversed.len()
        )
    })?;
    Ok(())
}

/// Releases keys one by one in the reverse order of the press.
fn emit_keys_reverse_stepwise(
    keyboard: &Arc<Mutex<VirtualDevice>>,
    keys: &[KeyCode],
    value: i32,
) -> Result<()> {
    let mut reversed = keys.to_vec();
    reversed.reverse();
    for key in reversed {
        emit_keys(keyboard, &[key], value)?;
        thread::sleep(COMBO_STEP_DELAY);
    }
    Ok(())
}
