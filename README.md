# x56linux

[![Rust](https://img.shields.io/badge/language-Rust-orange.svg)](https://www.rust-lang.org/)
[![License: GPL-3.0](https://img.shields.io/badge/License-GPLv3-blue.svg)](LICENSE)
[![Platform: Linux](https://img.shields.io/badge/platform-Linux-green.svg)]()
[![Tests: 45 passing](https://img.shields.io/badge/tests-45%20passed-brightgreen.svg)]()

A modern, safe, and high-performance **Rust** tool for configuring RGB LED lighting, axis response curves, deadbands, and hardware calibration on the **Saitek / Mad Catz Pro Flight X-56 Rhino** and **Logitech G X56** HOTAS systems.

Specifically engineered to operate on Linux **without root privileges (`sudo`)** and **without detaching the kernel joystick driver (`usbhid`/`joydev`/`evdev`)**, ensuring that flight and space simulators (*DCS World, X-Plane, Microsoft Flight Simulator, Star Citizen, Elite Dangerous, FlightGear*) continue receiving uninterrupted axis and button inputs while adjusting configuration live.

---

## Table of Contents

- [Key Features](#key-features)
- [Supported Devices](#supported-devices)
- [Canonical Axis Mapping and Ranges](#canonical-axis-mapping-and-ranges)
- [Installation and Packaging](#installation-and-packaging)
- [TOML Profile Format Specification](#toml-profile-format-specification)
- [CLI User Guide](#cli-user-guide)
  - [System Diagnostics (`x56 doctor`)](#1-system-diagnostics-x56-doctor)
  - [Device Detection (`x56 list`)](#2-device-detection-x56-list)
  - [RGB LED Lighting (`x56 rgb`)](#3-rgb-led-lighting-x56-rgb)
  - [Curves and Deadbands (`x56 axis`)](#4-curves-and-deadbands-x56-axis)
  - [TOML Profiles (`x56 profile`)](#5-toml-profiles-x56-profile)
  - [Button and Mode Programming (`x56 buttons`)](#6-button-and-mode-programming-x56-buttons)
  - [Hall Sensor Calibration (`x56 calibrate`)](#7-hall-sensor-calibration-x56-calibrate)
  - [Factory Reset (`x56 reset-defaults`)](#8-factory-reset-x56-reset-defaults)
- [Graphical Control Center (`x56-gui`)](#graphical-control-center-x56-gui)
  - [Avionics Vector Architecture](#avionics-vector-architecture)
  - [Curve Editor with Live Sampling](#1-curve-editor-with-live-sampling)
  - [RGB Lighting Manager](#2-rgb-lighting-manager)
  - [Hall Calibration Wizard](#3-hall-calibration-wizard)
  - [Integrated System Diagnostics](#4-integrated-system-diagnostics)
  - [Button and Mode Programming (Manual Section 8)](#5-button-and-mode-programming-manual-section-8)
  - [TOML Profile Management](#6-toml-profile-management)
- [HID Binary Protocol Specification](#hid-binary-protocol-specification)
- [Workspace Architecture](#workspace-architecture)
- [Roadmap Status](#roadmap-status)
- [License](#license)

---

## Key Features

- **Secure Rootless Operation**: Managed automatically by `logind` and `uaccess` upon user login.
- **RGB Lighting Control**: Full support for hexadecimal colors (`#00ffff`), shorthand (`#0ff`), or common color names (`cyan`, `orange`, `red`, `off`, etc.) for Stick, Throttle, or both peripherals simultaneously.
- **High-Resolution Response Curves**: Mathematical range `0..=1000` (500 = linear) for X/Y saturation, curvature, and deadband with symmetric (**S-curve**) or progressive (**J-curve**) profiles.
- **Button and Mode Programming (M1/M2/S1 + SLD)**: Per-mode macro and keybinding mapping with physical rotary switch tracking and live event emission via the Linux `/dev/uinput` subsystem.
- **TOML Profile Management**: Export and import complete configuration profiles (axes, RGB, and button programming) in a human-readable, versioned format.
- **Non-Invasive Hall Calibration**: Passive sampling of physical center position on Interface 0 (`/dev/hidraw*`), offset (delta) calculation, and a safe `--dry-run` mode to prevent accidental miscalibration.
- **Diagnostic Engine (`x56 doctor`)**: Comprehensive 6-layer system check (udev, permissions, USB hardware, hidraw, kernel joydev, and firmware handshake).
- **Multilingual Localization (Project Fluent)**: Native English (`en-US`) as the primary locale and Spanish (`es-ES`) with instant live runtime switching across the entire Graphical Control Center and System Tray.
- **Dual Human / JSON Output**: All operations support `--json` for automation scripts, continuous integration, or graphical frontends.

---

## Supported Devices

| Manufacturer | Peripheral | USB VID:PID | Role / Primary Axes |
|---|---|---|---|
| **Saitek / Mad Catz** | Pro Flight X-56 Rhino Stick | `0738:2221` | Main stick (Roll, Pitch, Twist/Yaw) |
| **Saitek / Mad Catz** | Pro Flight X-56 Rhino Throttle | `0738:a221` | Dual independent throttles and 4 rotaries |
| **Logitech G** | X56 HOTAS Stick | `046d:c22b` | Main stick (black/grey version) |
| **Logitech G** | X56 HOTAS Throttle | `046d:c22a` | Dual throttles and rotaries |

---

## Canonical Axis Mapping and Ranges

The protocol uses USB HID Generic Desktop usage codes (in hexadecimal format):

### Stick
| Axis | CLI Accepted Names | Hex ID | Dec ID | Default Profile | Factory Deadband |
|---|---|---|---|---|---|
| **X (Roll)** | `x`, `roll` | `0x30` | `48` | `S` (Symmetric) | 60 / 1000 |
| **Y (Pitch)** | `y`, `pitch` | `0x31` | `49` | `S` (Symmetric) | 50 / 1000 |
| **Z (Twist / Yaw)** | `z`, `yaw`, `twist`, `rudder` | `0x35` | `53` | `S` (Symmetric) | 0 / 1000 |

### Throttle
| Axis | CLI Accepted Names | Hex ID | Dec ID | Default Profile | Factory Deadband |
|---|---|---|---|---|---|
| **Throttle Left** | `left`, `throttle_left`, `tl`, `t1` | `0x30` | `48` | `J` (Asymmetric/Progressive) | 0 / 1000 |
| **Throttle Right** | `right`, `throttle_right`, `tr`, `t2`| `0x31` | `49` | `J` (Asymmetric/Progressive) | 0 / 1000 |
| **Rotary 1** | `rotary1`, `r1` | `0x32` | `50` | `S` (Symmetric) | 0 / 1000 |
| **Rotary 2** | `rotary2`, `r2` | `0x35` | `53` | `S` (Symmetric) | 0 / 1000 |
| **Rotary 4** | `rotary4`, `r4` | `0x36` | `54` | `S` (Symmetric) | 0 / 1000 |
| **Rotary 3** | `rotary3`, `r3` | `0x37` | `55` | `S` (Symmetric) | 0 / 1000 |

> **Parameter Scaling**: All configuration parameters (`xsat`, `ysat`, `deadband`, `curvature`) operate within the `0..=1000` range. A value of `curvature = 500` represents a 1:1 linear response.

---

## Installation and Packaging

### 1. Official Precompiled Packages (Releases)

Each release published on [GitHub Releases](https://github.com/angelgnz/x56linux/releases) includes ready-to-use binaries and packages for major Linux distributions:

#### Arch Linux / CachyOS / Manjaro
Available via the [`PKGBUILD`](packaging/PKGBUILD) file or prebuilt `.pkg.tar.zst` package:
```bash
# Via AUR (x56linux-git)
paru -S x56linux-git
# or with yay:
yay -S x56linux-git

# Or direct installation of the precompiled package:
sudo pacman -U x56linux-*.pkg.tar.zst
```

#### Debian / Ubuntu / Linux Mint / Pop!_OS
```bash
sudo dpkg -i x56linux_*_amd64.deb
sudo apt-get install -f  # Resolves dependencies if needed
```

#### Fedora / RHEL / openSUSE
```bash
sudo dnf install ./x56linux-*.x86_64.rpm
# or on openSUSE:
sudo zypper install ./x56linux-*.x86_64.rpm
```

#### Universal Standalone Installer (Any Linux Distribution)
Download and extract the `x56linux-v*-x86_64.tar.gz` archive:
```bash
tar -xzf x56linux-v*-x86_64.tar.gz
cd x56linux-v*-x86_64
sudo ./install.sh
```
*(To uninstall at any time: `sudo ./uninstall.sh`)*.

---

### 2. Manual Compilation from Source

Requires a stable Rust toolchain (Rust 1.75+):

```bash
git clone https://github.com/angelgnz/x56linux.git
cd x56linux
cargo build --release
```

Optimized binaries are generated in `target/release/`:
- `x56`: CLI tool for scripting, diagnostics, and services.
- `x56-gui`: Avionics Vector tactical graphical control center.

---

### 3. udev Rule (Rootless Access)

If compiling manually (the `.deb`, `.rpm`, and Arch packages handle this automatically), install the udev rule and reload udev:

```bash
sudo cp packaging/70-x56.rules /etc/udev/rules.d/
sudo udevadm control --reload-rules
sudo udevadm trigger --subsystem-match=hidraw
```

---

## TOML Profile Format Specification

`x56linux` profiles use the standard **TOML (v1)** format to serialize the complete HOTAS configuration into a single human-readable, versionable file that is easily transferable between flight simulators and systems.

### General Schema Structure

```toml
version = 1
name = "Profile Name"
description = "Description of tactical role or spacecraft"

[stick]
rgb = "#ff7700"          # Hexadecimal color (#RRGGBB or #RGB)
spring = "green"         # Physical spring ("red", "green", "yellow", "blue")

[[stick.axes]]
name = "X (Roll)"
axis_id = 48             # HID axis ID (48 = X, 49 = Y, 53 = Z/Twist)
xsat = 1000              # Physical input saturation (0..=1000)
ysat = 1000              # Processed output saturation (0..=1000)
deadband = 30            # Center deadband (0..=1000)
curvature = 620          # Sensitivity/Curvature (0..=1000, 500 = linear 1:1)
profile = "s"            # "s" (Symmetric / S-Curve) or "j" (Progressive asymmetric)
hall_zero = 32768        # Hall zero-point offset adjustment (optional)

[throttle]
rgb = "#ff7700"

[[throttle.axes]]
name = "Throttle Left"
axis_id = 48             # 48 = Left Throttle, 49 = Right Throttle, 50 = R1, 53 = R2, 54 = R4, 55 = R3
xsat = 1000
ysat = 1000
deadband = 0
curvature = 500
profile = "j"

[programming.m1]
name = "Flight and Supercruise"
rgb = "#9d4edd"          # Dynamic LED color for this mode (cosmic purple)

[programming.m1.cells.stick_trigger]
button_id = "stick_trigger"
name = "Primary Fire"
keys = ["Space"]

[programming.m2]
name = "Combat and Attack"
rgb = "#ff1100"          # Combat alert red
# stick_rgb = "#ff0000"  # Optional: specific override for Stick
# throttle_rgb = "#ff1100" # Optional: specific override for Throttle

[programming.m2.cells.stick_trigger]
button_id = "stick_trigger"
name = "Energy Weapons // Combat Mode"
keys = ["Ctrl", "Space"]

[programming.s1]
name = "Mining and Operations"
rgb = "#0088ff"          # Scanning & mining blue
```

### Field and Parameter Details

| Field | Type | Required | Description |
|---|---|---|---|
| `version` | Integer | Yes | File schema version (current: `1`). |
| `name` | String | Yes | Identifying name displayed in the GUI and CLI. |
| `description` | String | No | Notes or role details (e.g., spacecraft, fighter, simulator). |
| `[stick].rgb` | String | No | Default stick hex LED color (e.g., `"#ff7700"`, `"#00f0ff"`). |
| `[stick].spring` | String | No | Installed physical spring: `"red"` (52mm), `"green"` (47mm), `"yellow"` (42mm), or `"blue"` (36mm). |
| `[throttle].rgb` | String | No | Default throttle quadrant hex LED color. |
| `*.axes[].axis_id` | Integer | Yes | Canonical HID usage code (48=X, 49=Y, 50=R1, 53=Z/R2, 54=R4, 55=R3). |
| `*.axes[].deadband`| Integer | Yes | Center deadband in permille range `0..=1000`. |
| `*.axes[].curvature`| Integer | Yes | Curvature in range `0..=1000` (`500` = linear 1:1, `>500` = aggressive S-curve/exponential). |
| `*.axes[].profile` | String | Yes | Mathematical profile: `"s"` (symmetric / center-zero) or `"j"` (unipolar progressive for throttles). |
| `programming.<m>` | Table | No | Configuration for mode `m1`, `m2`, or `s1`. |
| `programming.<m>.name` | String | No | Descriptive mode name visible in CLI and GUI (e.g., `"Flight and Supercruise"`). |
| `programming.<m>.rgb` | String | No | Dynamic hex LED color applied when activating this mode (`"#9d4edd"`, `"#ff1100"`, etc.). Applied automatically when switching the physical rotary or connecting hardware. |
| `programming.<m>.stick_rgb` | String | No | Specific LED color override for Stick in this mode. |
| `programming.<m>.throttle_rgb` | String | No | Specific LED color override for Throttle in this mode. |
| `cells.<id>.keys` | Array | Yes | Key name sequence for `/dev/uinput` (e.g., `["Space"]`, `["Ctrl", "Alt", "Delete"]`). |

### Official Profiles Included in [`examples/`](examples/)

- [`default_profile.toml`](examples/default_profile.toml): Linear 1:1 profile with factory-standard values and deadbands.
- [`combat_profile.toml`](examples/combat_profile.toml): Aggressive S-curves, tight deadbands for atmospheric and space combat (*Star Citizen*, *DCS World*), green spring, and tactical orange illumination.
- [`elite_dangerous_profile.toml`](examples/elite_dangerous_profile.toml): Complete official profile for **Elite Dangerous**, featuring a 47mm green spring and **automatic dynamic colors per mode**:
  - **Mode M1 — Normal Flight / Supercruise (Purple `#9d4edd`)**: Primary/secondary fire, engine boost (`Tab`), FA Off/On (`Z`), 4-way power distributor (`Up`/`Left`/`Right`/`Down`), cockpit UI panels 1-4, FSD hyperspace jump (`J`), supercruise (`K`), landing gear (`L`), cargo scoop (`Home`), ship lights (`Insert`), night vision (`Delete`), and hardpoints on `thr_sld` (`U`).
  - **Mode M2 — Tactical Combat / Attack (Red `#ff1100`)**: Active countermeasures (Chaff, Heat Sink, Shield Cell Bank, ECM, Silent Running), subsystem targeting, and highest-threat lock.
  - **Mode S1 — Surface Operations / Mining / SRV (Blue `#0088ff`)**: Cargo scoop deployment, mining lasers, SRV handbrake, turret mode, composition scanner, and planetary driving.

---

## CLI User Guide

### 1. System Diagnostics (`x56 doctor`)

Verify in a single step that your system meets all requirements:

```bash
x56 doctor
```

Typical output:
```text
Saitek / Logitech X56 System Diagnostics (x56 doctor)
======================================================
✓ [OK]   udev rule for rootless access: Active and complete rule in /etc/udev/rules.d/70-x56.rules
✓ [OK]   'input' group: Current user belongs to 'input' group (headless access guaranteed)
✓ [OK]   HOTAS peripherals detected: Detected: Stick: CONNECTED, Throttle: CONNECTED
✓ [OK]   Read and write permissions: Rootless access granted across all hidraw nodes (4 nodes verified)
✓ [OK]   Kernel joystick driver (Interface 0): /dev/input/js* nodes present; flight simulators will receive axes continuously
✓ [OK]   X56 firmware handshake (Interface 2): Bidirectional communication confirmed on 2 device(s)
------------------------------------------------------
Summary: 6 OK, 0 warnings, 0 failures.
System status: 100% OPERATIONAL.
```

### 2. Device Detection (`x56 list`)

Lists detected X56 devices exclusively on Interface 2:

```bash
x56 list
x56 list --json
```

### 3. RGB LED Lighting (`x56 rgb`)

Control LED lighting on Stick, Throttle, or both:

```bash
# Factory default cyan color on both devices
x56 rgb cyan

# Set Stick to cyan and Throttle to orange using hex codes
x56 rgb --device stick '#00ffff'
x56 rgb --device throttle '#ff5500'

# Turn off lighting completely
x56 rgb off
```

### 4. Curves and Deadbands (`x56 axis`)

#### Query the current axis configuration:
```bash
# Stick axes
x56 axis get --device stick --axis x
x56 axis get --device stick --axis y
x56 axis get --device stick --axis z --json

# Throttle axes
x56 axis get --device throttle --axis left
x56 axis get --device throttle --axis r1
```

#### Modify axis parameters:
```bash
# Set deadband to 65 and curvature to 520 on Stick X axis
x56 axis set --device stick --axis x --deadband 65 --curvature 520

# Change left throttle curve to progressive (J)
x56 axis set --device throttle --axis left --curvature 480 --profile j
```

### 5. TOML Profiles (`x56 profile`)

#### Export current hardware configuration:
```bash
x56 profile save my_profile.toml --name "DCS Hornet" --stick-rgb '#00ffff' --throttle-rgb '#ff5500'
```

#### Load and apply a profile:
```bash
x56 profile load my_profile.toml
x56 profile load my_profile.toml --json
```

Ready-to-use sample profiles are included in the [`examples/`](examples/) directory:
- [`default_profile.toml`](examples/default_profile.toml): Factory linear curves and standard deadbands.
- [`combat_profile.toml`](examples/combat_profile.toml): Tight deadbands, aggressive curves for combat, and tactical orange LEDs.
- [`elite_dangerous_profile.toml`](examples/elite_dangerous_profile.toml): Full official profile for Elite Dangerous (3 flight modes and HUD amber lighting `#ff7700`).

#### Structure of the `[programming]` table in profiles:
TOML profiles can include the `[programming]` section to configure key assignments and macros across the three rotary switch modes (`m1`, `m2`, `s1`):

```toml
[programming.m1.cells.stick_trigger]
button_id = "stick_trigger"
name = "Primary Fire"
keys = ["Space"]

[programming.m2.cells.stick_trigger]
button_id = "stick_trigger"
name = "Air-to-Air Missile"
keys = ["Ctrl", "Space"]

[programming.s1.cells.thr_sw_1]
button_id = "thr_sw_1"
name = "Landing Gear"
keys = ["G"]

[programming.s1.cells.thr_sld]
button_id = "thr_sld"
name = "Speedbrakes"
keys = ["B"]
```

### 6. Button and Mode Programming (`x56 buttons`)

Allows inspecting the physical button catalog, checking live hardware status, binding key combinations in TOML profiles, and running the background monitor to emit keystrokes via `/dev/uinput`.

#### List button catalog (`x56 buttons list`):
Displays the full catalog of assignable controls with their stable ID, manual section code, Linux evdev code, and functional group:
```bash
# List all buttons (Stick and Throttle)
x56 buttons list

# Filter stick or throttle buttons only
x56 buttons list --device stick
x56 buttons list --device throttle

# Export catalog in JSON format
x56 buttons list --json
```

#### Check live hardware status (`x56 buttons status`):
Passively reads the current physical position of mode switch F (`M1`, `M2`, `S1`), `SLD` slider state, and currently pressed buttons via evdev:
```bash
x56 buttons status
x56 buttons status --json
```

Typical output:
```text
X56 Hardware Status
===================
Connected devices      : Stick [YES], Throttle [YES]
Active physical mode   : M1 (Mode 1: Navigation / Standard)
SLD slider switch      : INACTIVE (OFF)
Pressed buttons        : none
```

#### Bind keys to a button in a mode (`x56 buttons bind`):
Binds key shortcuts or commands to a specific button in any of the three modes (`m1`, `m2`, `s1`) and commits the changes to the specified TOML file:
```bash
# Bind Spacebar to trigger in Mode 1 (M1)
x56 buttons bind --file my_profile.toml --mode m1 --button stick_trigger --keys "Space" --name "Primary Fire"

# Bind modifier combination in Mode 2 (M2)
x56 buttons bind --file my_profile.toml --mode m2 --button stick_btn_a --keys "Ctrl+Shift+F" --name "Countermeasures"

# Bind G key to TGL 1 switch in Special Mode (S1)
x56 buttons bind --file my_profile.toml --mode s1 --button thr_sw_1 --keys "G" --name "Landing Gear"

# Bind speedbrakes to the 2-position SLD slider
x56 buttons bind --file my_profile.toml --mode s1 --button thr_sld --keys "B" --name "Speedbrakes"
```

> **Key Syntax**: Combinations separated by `+` or `,` are supported (e.g., `Ctrl+Alt+Delete`, `Shift+F2`, `Space`, `Enter`, `Escape`, `Up`, `Down`, `A..Z`, `0..9`).

#### Run live emission monitor (`x56 buttons run`):
Loads a TOML profile, creates a virtual Linux keyboard via `/dev/uinput`, and monitors hardware at 50 Hz. When physical buttons are pressed, it instantly emits the configured keystrokes according to the actual physical position of the mode selector (`M1`, `M2`, `S1`):
```bash
x56 buttons run my_profile.toml
```

Runtime output:
```text
Profile 'DCS Hornet' active: 8 binding(s). Press Ctrl+C to exit.
Current mode: M1 - Mode 1 (Navigation / Standard)
[M1] stick_trigger (Trigger) -> Space
[M1] thr_sld (SLD) -> B
Mode -> M2 - Mode 2 (Combat / Attack)
[M2] stick_trigger (Trigger) -> Ctrl + Space
```

### 7. Hall Sensor Calibration (`x56 calibrate`)

Passively samples the joystick center position via Interface 0 (without modifying the kernel driver) and calculates the exact zero-point adjustment for the Hall effect sensor:

```bash
# Simulation mode (Dry-Run): Displays calculated values without writing to EEPROM
x56 calibrate stick --dry-run

# Apply real physical calibration averaging 100 samples
x56 calibrate stick --samples 100
```

Example output:
```text
Hall Effect Sensor Calibration (Stick X and Y)
==============================================
ACTIVE MODE: Zero points applied and stored in hardware.
Samples analyzed: 100 packets from Interface 0 (read-only)

• X (Roll)
  Previous zero point : 0x8859 (34905)
  Sampled zero point  : 0x7f40 (32576)
  Adjustment (Delta)  : -2329
  Status              : APPLIED

• Y (Pitch)
  Previous zero point : 0x83f7 (33783)
  Sampled zero point  : 0x8737 (34615)
  Adjustment (Delta)  : +832
  Status              : APPLIED
```

### 8. Factory Reset (`x56 reset-defaults`)

Restores manufacturer default curve and deadband values:

```bash
x56 reset-defaults --device all
```

---

## Graphical Control Center (`x56-gui`)

`x56-gui` is a modern, native immediate-mode graphical interface built with **egui / eframe** and **egui_plot**. It runs at 30+ FPS on X11 or Wayland with hardware-accelerated OpenGL rendering.

```bash
# Launch the graphical application
x56-gui

# Or build and run directly with Cargo
cargo run -p x56-gui --release
```

### Avionics Vector Architecture
The interface features a military cockpit tactical aesthetic inspired by HUD systems and flight instruments:
- **Tactical HeaderBar (48 px)**: Clean, minimalist top bar without redundant window decorations (close, minimize, and maximize controls are cleanly delegated to the OS window manager and tiling environments), integrating the TOML profile selector, rapid connection indicators, and the `/dev/uinput` injection toggle (*Blue Target*).
- **Navigation Sidebar (256 px)**: Quick access with tactical icons to all tools: Axis Curves, RGB Lighting, Hall Calibration, Button Programming, and Doctor Diagnostics.
- **Hardware Bottom Dock**: Real-time telemetry bar displaying active `/dev/hidraw*` nodes, USB bus state, and the **Physical Spring Selector** (`PhysicalSpring`):
  - 🔴 **Red (52 mm)**: Extra Strong / Maximum Resistance.
  - 🟢 **Green (47 mm)**: Strong / Standard Resistance (Recommended).
  - 🟡 **Yellow (42 mm)**: Medium / Moderate Resistance.
  - 🔵 **Blue (36 mm)**: Soft / Light Resistance.

The selected spring is automatically stored and serialized within the active TOML profile (`spring = "green"`).

### 1. Curve Editor with Live Sampling
- **Reactive 2D Visualization**: Plots the mathematical transfer function in real time, calculating curvature transformations (S/J), X/Y saturation, and deadband.
- **Live Physical Input**: A background poller thread passively reads HID reports from **Interface 0** (`/dev/hidraw*`) without detaching the kernel driver, displaying an animated red dot at the exact physical stick position.
- **Interactive Controls**: Sliders for curvature (`0..1000`), deadband (`0..1000`), X/Y saturation (`0..1000`), and profile selector (Symmetric S vs Progressive J).
- **Immediate Action Buttons**:
  - `Apply to Hardware`: Writes and commits configuration to peripheral EEPROM.
  - `Read from Hardware`: Refreshes controls by reading current firmware settings.
  - `Linear (1:1) Values`: Quickly resets curve to 1:1 response with zero deadband.

### 2. RGB Lighting Manager
- Integrated visual color picker (RGB Color Picker).
- Real-time preview and instant hardware application.
- Optional single-click synchronization between Stick and Throttle.
- Quick preset palette: *Rhino Cyan, Neon Green, Solar Orange, Deep Red, Pure White, Royal Purple, Stealth (Off)*.

### 3. Hall Calibration Wizard
- Step-by-step guided wizard to eliminate off-center bias and physical drift on Hall Effect sensors (Stick X and Y axes).
- Passive 50-packet center sampling without interfering with active simulators.
- Deviation (Delta) calculation and drift threshold evaluation (< 1000 = Excellent, 1000..3000 = Acceptable, > 3000 = High Deviation).
- Safe `--dry-run` mode to preview values before committing to EEPROM.

### 4. Integrated System Diagnostics
- Visual dashboard for real-time system health checks.
- 6-layer inspection: udev permissions, `/dev/hidraw*` nodes, USB device detection, simulator support (`/dev/input/js*`), and bidirectional firmware handshake.

### 5. Button and Mode Programming (Manual Section 8)
Directly inspired by **Section 8 of the Official Logitech G X56 HOTAS User Manual**, this tab provides a professional environment for creating command mappings, macros, and keybindings with full support for the three rotary mode positions (`M1`, `M2`, `S1`) and the `SLD` slider switch.

- **Interactive 2D Technical Schematic with `egui::Painter`**:
  - Renders highly legible technical vector schematics for Stick and Throttle structured by manufacturer manual sections:
    - **Stick**: Section A (*8-Way POV*), Section B (*Push C Analog Mini-Stick*), Section C (*H1 Witches and H2 Castle Thumb HATs*), Section D (*Primary Trigger and A, B, D Buttons*), Section F (*Pinkie Switch*).
    - **Throttle**: Section A (*Push Rotary F/G and Scroll Wheel*), Section B (*2-Position SLD Slider Switch*), Section C (*E, H, I Buttons, Push Mini-Stick, and H3/H4 HATs*), Section D (*Twin Dual Throttles*), Section F (*Rotary Mode Switch M1/M2/S1*), Section G (*Base TGL 1 to 7 with SW 1 to 14*).
  - **Live Visual Feedback**:
    - **Neon Green**: Illuminates instantly when a button is physically pressed on the hardware.
    - **Bright Cyan**: Highlights when hovering over the corresponding command cell or node.
    - **Amber**: Indicates that the button already has a key or command bound in the currently visible mode.
  - **Direct Interaction**: Clicking any schematic node immediately opens the corresponding cell for editing.
- **Command Cells per Mode (`M1`, `M2`, `S1`)**:
  - Organized by functional groups with a dropdown filter and binding counters.
  - Displayed via **high-contrast white tiles** for each bound key (e.g., `[LShift] + [F2]`).
  - **Interactive Editing Mode**: Clicking a cell displays a blinking cursor (`▌ Waiting for keystroke...`), captures any pressed key combination in real time (including `Ctrl`, `Shift`, `Alt` modifiers, `F1..F12` function keys, `Space`, `Enter`, `Tab`, `Escape`, directional arrows, and alphanumeric keys), allows naming the command (e.g., *"Landing Gear"*), and confirming with green checkmark (`✔`) or cancelling with red cross (`✖`).
- **Automatic Tracking of Mode Selector F and SLD Slider**:
  - 50 Hz real-time evdev polling.
  - **Physical Synchronization**: Enabling *"🔄 Follow physical M1/M2/S1 selector"* automatically switches the editor view between `M1`, `M2`, and `S1` as the pilot physically turns the rotary switch on the throttle base.
  - Status badges in the top bar indicating active physical mode and `SLD` slider state (`ACTIVE (ON)` / `INACTIVE (OFF)`).
- **"Blue Target" Emission Activation via `/dev/uinput`**:
  - Prominent toolbar button to activate or pause live OS keystroke injection.
  - When the "Blue Target" is active, pressing buttons on the X56 instantly triggers bound key combinations via the `/dev/uinput` virtual keyboard.
  - Provides clear visual indicators regarding `uinput` kernel module availability and access permissions.
- **Modal Test Window ("Test Profile")**:
  - Accessible via the `⚙️ Test Profile` gear icon.
  - Live monitors all triggered commands with local timestamp (`HH:MM:SS`), active mode, pressed physical button, command name, emitted key tiles, and `uinput` verification status.
  - Includes an interactive test input field to immediately verify that keystrokes are properly typed and dispatched without needing an external application.

### 6. TOML Profile Management
- Direct top-bar access to `Save TOML Profile` or `Load TOML Profile`.
- Allows backing up complete profiles (axes, curves, RGB lighting, and full 3-mode button bindings) and sharing them across different flight simulators (*DCS*, *MSFS*, *X-Plane*, *Star Citizen*).

---

## HID Binary Protocol Specification

The Saitek/Logitech X56 peripheral exposes two primary USB interfaces:
- **Interface 0 (HID Class 3, SubClass 0)**: Game input channel (`/dev/input/js*`). Endpoint 1 Interrupt IN (11-byte reports with axis positions and button states in Little-Endian).
- **Interface 2 (Vendor Specific 0xFF00)**: Bidirectional configuration channel. Endpoint 0 (Control) and Endpoint 2 Interrupt IN.

### Report IDs on Interface 2
| Report ID | Type | Length | Purpose |
|---|---|---|---|
| `0x01` | Feature Set | 64 bytes | Packet 4: Commit / End of configuration block |
| `0x08` | Feature Get | 64 bytes | Packet 6: Hardware query (hardcoded first axis) |
| `0x09` | Feature Set | 64 bytes | Packet 5: RGB LED color configuration |
| `0x0B` | Feature Set | 64 bytes | Axis configuration and query (subcommands 0x01, 0x02, 0x03, 0x04) |
| `0xFF` | Interrupt IN | 64 bytes | Packet 8: Asynchronous axis query response |

### Axis Write Sequence (Reverse-Engineered)
To persistently apply an axis configuration:
1. **Packet 1 (Select Stage 1)**: Feature Report `0x0B`, subcommand `0x03` (`[0x0b, 0x03, 0x00, axis_id, ...]`).
2. **Cadence pause**: `20 ms` delay.
3. **Packet 2 (Select Stage 2)**: Feature Report `0x0B`, subcommand `0x04` (`[0x0b, 0x04, 0x00, axis_id, ...]`).
4. **Cadence pause**: `20 ms` delay. Non-blocking drain of Interrupt IN acknowledgment.
5. **Packet 3 (Configuration)**: Feature Report `0x0B`, subcommand `0x01` with full Big-Endian payload:
   - Bytes 8-9: `xsat`
   - Bytes 10-11: `ysat`
   - Bytes 12-13: `deadband`
   - Bytes 14-15: `curvature`
   - Byte 16: `profile` (`0` = S, `1` = J)
   - Bytes 17-18: `hall_zero`
6. **Packet 4 (Commit)**: Feature Report `0x01` (`[0x01, 0x01, ...]`).

---

## Workspace Architecture

The project is structured as a modular Rust workspace:

```text
x56linux/
├── .github/workflows/
│   └── release.yml              # Multi-distro CI/CD (deb, rpm, arch, tarball)
├── Cargo.toml
├── packaging/
│   ├── 70-x56.rules             # Modern uaccess udev rule
│   ├── com.saitek.x56.desktop   # Linux desktop entry
│   ├── x56linux.svg             # Official application vector icon
│   ├── PKGBUILD                 # Arch Linux / AUR package definition
│   ├── install.sh               # Universal installation script
│   └── uninstall.sh             # Clean uninstallation script
├── examples/
│   ├── default_profile.toml     # Factory linear profile
│   ├── combat_profile.toml      # Combat profile for flight simulators
│   └── elite_dangerous_profile.toml # Full official profile for Elite Dangerous
└── crates/
    ├── x56-proto/               # Pure binary decoder/encoder (no_std friendly)
    ├── x56-hid/                 # hidraw HID transport (Interface 2 isolation)
    ├── x56-core/                # Domain logic, TOML profiles, Hall calibrator, and doctor
    ├── x56-cli/                 # clap-based CLI with formatted and --json outputs
    └── x56-gui/                 # Native graphical control center (egui / egui_plot)
```

---

## Roadmap Status

- [x] **Phase 0 — Foundation and Verification**
  - [x] Structured Cargo workspace and preserved `legacy/cpp` branch.
  - [x] hidraw capture and inspection lab on real hardware.
  - [x] Validated modern `70-x56.rules` udev rule.
- [x] **Phase 1 — Functional CLI and Complete Protocol**
  - [x] `x56 list`: Interface 2 detection without joystick driver detach.
  - [x] `x56 rgb`: LED light control by name and hex code.
  - [x] `x56 axis get`: Read all axes via Interrupt Report 0xFF.
  - [x] `x56 axis set`: Persistent curve modification and round-trip.
  - [x] `x56 profile save/load`: Exportable and applicable TOML profiles.
  - [x] `x56 buttons`: Button catalog, live hardware status, bind, and background monitor (`run`).
  - [x] `x56 calibrate`: Passive sampling and Hall sensor calibration.
  - [x] `x56 doctor`: Comprehensive system diagnostics.
  - [x] `x56 reset-defaults`: Factory restore.
- [x] **Phase 2 — Packaging and Distribution**
  - [x] Package generation for `.deb` (`cargo-deb`), `.rpm` (`cargo-generate-rpm`), and Arch Linux (`PKGBUILD` / AUR).
  - [x] Standalone installer and uninstaller (`install.sh` / `uninstall.sh`).
  - [x] Automated CI/CD pipeline on GitHub Actions for Releases with prebuilt artifacts.
  - [ ] `systemd --user` service for automatic profile reapplication upon reconnect.
- [x] **Phase 3 — Graphical User Interface (GUI)**
  - [x] Native egui control center (`x56-gui`) at 30+ FPS with Avionics Vector styling.
  - [x] Tactical HeaderBar without redundant window decorations, integrated with OS window manager.
  - [x] Physical spring selector and persistence (`PhysicalSpring`).
  - [x] Interactive 2D visual curve editor with `egui_plot`.
  - [x] Real-time plotting of physical input point (passive sampling on Interface 0).
  - [x] RGB color manager with interactive picker and Stick + Throttle synchronization.
  - [x] Step-by-step Hall sensor calibration wizard with drift thresholds.
  - [x] Integrated system diagnostics tab (`DoctorTab`).
  - [x] Button and Mode Programming tab (Section 8) with interactive 2D schematic, uinput injection, and modal test window.
  - [x] TOML profile export / import modal (axes, RGB, springs, and buttons).

---

## License

This project is licensed under the GNU General Public License v3.0 (GPLv3). See the [`LICENSE`](LICENSE) file for full details.
