# firemap

## Overview

This project is a profile based Linux input remapper that uses deterministic `evdev` device selection and `uinput` output.

It was built because the available Linux options were not a great fit for this setup, either because features such as autofire were missing or because the overall stack was heavier than desired. The current hardware configuration targets a Razer Tartarus V2 and a Razer Naga V2 HyperSpeed.

## Prerequisites

You need Rust and Cargo, the `uinput` kernel module, and permission to read `/dev/input/event*` and create a virtual input device.

```bash
rustc --version
cargo --version
lsmod | grep uinput || sudo modprobe uinput
```

## Build

Build the project from the current repository root.

```bash
cargo build
```

## Inspect Devices

Print the detected `event*` devices and their USB identity so you can determine the correct `vendor_id`, `product_id`, `interface_number`, and optional `serial`.

```bash
cargo run -q -- inspect
```

## List Devices

Print a short list of detected `event*` devices when you only need a quick overview or want to choose a target for `keycodes` or `probe`.

```bash
cargo run -q -- devices
```

## Probe One Device

Read raw key events from one specific device to confirm which interface actually carries the keys you want to map.

```bash
sudo cargo run -q -- probe /dev/input/eventX
```

## Read Raw Keycodes

Print raw key names and numeric keycodes on key press. When a label such as `Tartarus` is used, the target device is resolved through `config/hardware.toml`.

```bash
sudo cargo run -q -- keycodes Tartarus
sudo cargo run -q -- keycodes /dev/input/eventX
sudo cargo run -q -- keycodes /dev/input/eventX --no-grab
```

## Configure Devices

### Hardware

Store the stable hardware identity in `config/hardware.toml`.

```toml
[[device]]
label = "Tartarus"
expected_name = "Razer Razer Tartarus V2"
vendor_id = "1532"
product_id = "022b"
interface_number = 0

[[device]]
label = "Naga"
expected_name = "Razer Razer Naga V2 HyperSpeed"
vendor_id = "1532"
product_id = "00b4"
interface_number = 2
```

### Profile

Store profile specific mappings in `config/profiles/<name>.toml`, use `fire_ms` only on entries that should repeat automatically while the source key stays pressed, and use `outputs = [...]` for key combinations.

```toml
profile = "ffxiv"

[[device]]
label = "Tartarus"
mapping = [
  { input = "KEY_Q", output = "KEY_1", fire_ms = 250 },
  { input = "KEY_UP", output = "KEY_W" },
  { input = "KEY_5", outputs = ["KEY_LEFTSHIFT", "KEY_SEMICOLON"] },
]

[[device]]
label = "Naga"
mapping = [
  { input = "KEY_0", output = "KEY_SPACE" },
]
```

Stop the main remapping program before using `probe` or `keycodes`, otherwise the grabbed physical devices may not be readable by the helper command.

Linux input key names are layout agnostic and follow `evdev` naming, so the correct output key may differ from the printed character on your keyboard layout. Use `keycodes` on the real keyboard device and map the observed keycode rather than the printed character.

## Run

Grab the configured physical devices and expose the mapped output through a virtual keyboard created with `uinput`, using the default `ffxiv` profile or the profile name passed on the command line.

```bash
sudo cargo run -q -- run
sudo cargo run -q -- run ffxiv
```

## Run With systemd

Build a release binary before creating a service.

```bash
cargo build --release
```

Create a system service if you want `firemap` to start automatically at boot. Run it as root by default, because `firemap` needs access to `/dev/uinput` and `/dev/input/event*`. Use a user service only if you have already configured those permissions for your account.

```ini
[Unit]
Description=firemap
After=multi-user.target

[Service]
Type=simple
WorkingDirectory=/path/to/firemap
ExecStart=/path/to/firemap/target/release/firemap run ffxiv
Restart=on-failure
RestartSec=1

[Install]
WantedBy=multi-user.target
```

Write that file to `/etc/systemd/system/firemap.service`, then reload and enable it. If you have configured sufficient device permissions for a normal user, you can instead place it in `~/.config/systemd/user/` and add `User=dummy` to the service.

```bash
sudo systemctl daemon-reload
sudo systemctl enable --now firemap.service
```
