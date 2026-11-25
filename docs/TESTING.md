# Hardware validation guide

> **Validation status**: Automated tests cover curve/policy math, `SysfsBackend` filesystem I/O against temporary `sysfs`/`hwmon` trees, worker state-machine safety/recovery, mocked-IPC Playwright UI checks, updater manifest validation, and CI packaging. **Physical hardware writes, Zero RPM fan stopping, and desktop lifecycle behavior have not yet been hardware-validated on Linux GPUs** (and remain pending for Windows writes as well). Do not describe any board or driver as hardware-verified until these checks pass on real hardware.

## Preparation and read-only checks

Close or disable other fan controllers before testing:
- **Windows**: Close MSI Afterburner and disable Radeon Software fan tuning (keep Radeon Software available for independent observation).
- **Linux**: Stop or disable `fancontrol`, `lact`, `corectrl`, or custom `hwmon` scripts. Ensure the `amdgpu` kernel driver is loaded (`lsmod | grep amdgpu`).

For read-only checks, turn custom control off before quitting the GUI so the next launch remains in monitoring mode; new profiles already default to off.

### Windows (`fan-cli.exe`)

```powershell
.\fan-cli.exe list
.\fan-cli.exe probe 'pci:41:0:0'
```

Read-only probing on Windows on 2026-10-27 confirmed one RX 5600 XT, Overdrive8 curves, 25–100 °C / 20–100% driver ranges, and a Zero RPM toggle, without an adjustable stop-temperature range. This confirms API availability, not successful writes or physical fan stopping.

### Linux (`fan-cli`)

Read-only probing works without installing the `udev` rule or running as root:

```bash
./fan-cli list
./fan-cli probe 'pci:03:00.0'
```

Replace the PCI ID with the exact `id` returned by `list`. `probe` prints `curve_info()` JSON followed by three `sample()` JSON lines (2 seconds apart). Verify:
- `host_pwm_supported: true` and `native_curve_supported: false`
- `writable: false` before `udev` setup, or `writable: true` after installing `60-amd-fan-control.rules`
- `zero_rpm_supported` (`true` when `pwm1_min` is absent or `0`)
- Plausible `edge_c`, `hotspot_c` (if exposed), `rpm`, and `speed_percent` readings; missing sensors appear as `null`.

## Explicit 15-second timed test

Before running write tests on Linux, install [`packaging/linux/60-amd-fan-control.rules`](../packaging/linux/60-amd-fan-control.rules) (or install the `.deb`/`.rpm` package) so `writable` is `true`.

Close the GUI first (so `flock` / mutex does not reject concurrent access) and review `config.example.json`:

```powershell
# Windows:
.\fan-cli.exe test-curve 'pci:41:0:0' .\config.example.json
```

```bash
# Linux:
./fan-cli test-curve 'pci:03:00.0' ./config.example.json
```

What `test-curve` does:
- **Windows**: Applies and reads back five OD8 points, samples every 2.5 seconds for 15 seconds (updating Zero RPM if needed), then requests driver defaults.
- **Linux**: Runs 15 one-second ticks using `StepDown` hysteresis (`3.0` °C band, `6` s hold delay), writes `pwm1_enable=1` and `pwm1` (`0..=255`) with readback verification whenever the target changes, then restores `pwm1_enable=2`.

Read failures or protective temperature thresholds (`edge_c >= 90` or `hotspot_c >= 105`) abort and attempt restoration. Do not run a stress test during initial validation or force-terminate the process. Ctrl+C is not a managed exit path.

### Manual restoration

To restore automatic fan control after an interrupted test:

```powershell
# Windows:
.\fan-cli.exe reset 'pci:41:0:0'
```

```bash
# Linux:
./fan-cli reset 'pci:03:00.0'
```

On Windows, `reset` restores ADL fan defaults. On Linux, `reset` writes `2` to `pwm1_enable` and verifies readback (`cat /sys/class/drm/card*/device/hwmon/hwmon*/pwm1_enable` should be `2`). Failed reset is reported as an error.

## GUI and desktop lifecycle

1. **Read-only vs writable startup (Linux)**: Launch the `.AppImage` before installing the `udev` rule and verify the app opens cleanly in read-only monitoring mode with live telemetry and backup export working, and **Use custom fan curve** showing actionable `60-amd-fan-control.rules` guidance. Install the `udev` rule, trigger `udevadm`, and verify writable control becomes available.
2. **Telemetry & English UI**: Wait for three consistent readings. Confirm interface, error messages, and tray menu are English. On Linux, confirm **Driver / firmware sensor** is marked unavailable and disabled.
3. **Layout & themes**: Resize to 680 × 650, 920 × 790, 1366 × 900, and maximized. Check both light/dark themes, logo, chart sizing, explicit input labels, and Settings below/beside the curve. No horizontal page overflow should appear.
4. **Chart editing**: Drag each point from its center and hit-area edge. It should not jump on pointer-down or select text. Drag beyond the graph and release; constraints must hold. Pointer cancellation or window blur must end dragging. Edit numbers after dragging, and use chart arrow keys/Shift. Reject duplicate temperatures, decreasing speeds, invalid values, and a last point below 100% or above 85 °C.
5. **Custom curve & restoration**: Save with **Save curve & settings**, turn on **Use custom fan curve**, and verify independently (`pwm1_enable` = `1` and `pwm1` matches target on Linux; OD8 settings on Windows). Turn the switch off and check driver defaults (`pwm1_enable` = `2` on Linux), including Zero RPM. The saved custom curve must be kept. **Use driver defaults** must also work as an explicit recovery command.
6. **Tray & Quit**: Apply, close window to tray, reopen from tray, and **Quit** from the tray menu. Verify automatic restoration before exit. On GNOME Shell, verify whether an AppIndicator extension is required for tray icon visibility.
7. **Autostart & startup resume**: Test **Launch at login** (Windows Run key / Linux `~/.config/autostart/` `.desktop` entry) separately from **Start in tray**. Save the latter with **Save all changes**. Enable the custom curve, quit, then reopen and reboot: after fresh telemetry, the saved curve must resume on the same GPU and the switch must reflect confirmed readback. Turn it off, restart again, and verify it stays off.
8. **Backup export/import**: Use **Export settings** and choose a temporary JSON file. Change a curve point, use **Import settings** to load the backup, accept the replacement prompt, and confirm the editor is marked as unsaved. Save, then verify the curve and settings were restored. Cancel both dialogs once and confirm no settings change.
9. **Updater UX**: Click **Check for updates**. On `.deb` / `.rpm` installs, confirm it reports that in-place updates are unsupported and directs to GitHub Releases. On Windows NSIS / Linux `.AppImage`, confirm it checks `updater/latest.json` cleanly.
10. **Fault scenarios**: After these checks, evaluate sleep/resume, external `pwm1` / `pwm1_enable` modification, `flock` contention with a second `fan-cli` process, and forced termination. Record the state left in the driver.

## Zero RPM verification

1. Restore automatic. Set points 1 and 2 to 40 °C / 20% and 50 °C / 40%. Keep the rest increasing and reach 100% by 85 °C.
2. Choose **GPU temperature**, enable **Allow fans to stop**, keep 25%, save, and apply. This gives a 42 °C cutoff. Initial permission requires GPU temperature below 39 °C; permission is withdrawn at 42 °C. Repeat separately with **Junction / hotspot** using that sensor. (On Windows, Firmware mode uses the hotter reading.)
3. **Verify via actual RPM (`rpm` / `fan1_input`)**: On Windows, `zero_rpm_enabled = true` is not proof the fans physically stopped; on Linux, `pwm1 = 0` is not proof the fans physically stopped. Firmware/VBIOS may enforce a minimum RPM or custom restart behavior.
4. Let the selected temperature cross naturally and confirm transitions, restart, and the 3 °C re-arm band. Do not deliberately overheat the card. Both emergency thermal guards (edge ≥90 °C, hotspot ≥105 °C) remain active.
5. Restore automatic and verify default behavior (`pwm1_enable = 2` on Linux).

## Selected-sensor speed control

Start with Zero RPM disabled. Test **GPU temperature** and **Junction / hotspot** separately: save the selection, activate, and compare the requested speed from interpolation against driver state (`pwm1` on Linux, OD8 speed-hold points on Windows) and independently observed fan RPM.

Confirm:
- One-second control updates even with a longer monitoring interval.
- Unchanged targets skip redundant writes.
- On Linux, cooling down holds the previous speed until temperature stays at least 3 °C below the held temperature for 6 continuous seconds (`StepDown`).
- Switching off or quitting restores automatic control.

## Automated checks and packaging

Automated checks do **not** write to real GPU hardware:

```bash
npm run build
node --test tests/updater-manifest.test.mjs
npx playwright install chromium
npm run test:ui
cargo fmt --all --check
cargo test --locked --workspace
cargo clippy --locked --workspace --all-targets -- -D warnings
# Linux-only GLib iterator regression:
cargo test --locked --release -p glib-security-regression
```

- `fan-core` unit tests verify curve validation, interpolation, `StepDown`, Zero RPM policy, and config persistence.
- `amd-adl` (`SysfsBackend`) tests exercise real filesystem I/O against temporary `sysfs`/`hwmon` directory trees.
- `src-tauri` worker tests use `FakeGpu`, `GateGpu`, and `SysfsBackend` on temporary trees to verify control loops, `StepDown` smoothing, Zero RPM gating, `udev` error messaging, and safety restoration.
- Playwright browser tests (`npm run test:ui`) use mocked Tauri IPC and cannot validate real ADL, `sysfs`, WebView2, or WebKitGTK behavior.

For hardware test reports, include GPU/board model, OS/kernel/driver version, PCI ID, `config.json`, timestamps, `fan-cli probe` output, and actual observed fan RPM.
