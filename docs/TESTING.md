# Hardware validation on the RX 5600 XT

## Preparation and read-only checks

Use Windows 10/11 x64 with the AMD driver installed. Close MSI Afterburner and disable other fan controllers. The app opens in monitoring mode without writing fan settings. Keep Radeon Software available for independent observations with its fan tuning disabled.

```powershell
.\fan-cli.exe list
.\fan-cli.exe probe 'pci:41:0:0'
```

Replace the PCI ID with the exact value from `list`. The probe prints curve capabilities, Zero RPM support/state, point limits, and three samples. Missing sensors are `null`. Compare readings independently; unsynchronized samples can differ.

Read-only probing on 2026-10-27 confirmed one RX 5600 XT, Overdrive8 curves, 25–100 °C / 20–100% driver ranges, and a Zero RPM toggle, without an adjustable stop-temperature range. This confirms API availability, not successful writes or physical fan stopping.

## Explicit timed test

Close the GUI first and review `config.example.json`:

```powershell
.\fan-cli.exe test-curve 'pci:41:0:0' .\config.example.json
```

The command applies and reads back five points, samples for 15 seconds, then requests fan defaults. Read failures or temperature protection abort and attempt restoration. Do not run a stress test during initial validation or force-terminate the process. Ctrl+C is not a managed exit path.

To request defaults after an interrupted test:

```powershell
.\fan-cli.exe reset 'pci:41:0:0'
```

Automatic means ADL fan defaults. Failed reset is reported as an error. If the driver stops responding, use official driver controls and restart the system if needed.

## GUI and lifecycle

1. Wait for three consistent readings. Confirm interface, error messages, and tray menu are English.
2. Resize to 680 × 650, 920 × 790, 1366 × 900, and maximized. Check both themes, logo, chart sizing, explicit input labels, and Settings below/beside the curve. No horizontal page overflow should appear.
3. Drag each point from its center and hit-area edge. It should not jump on pointer-down or select text. Drag beyond the graph and release; constraints must hold. Pointer cancellation or window blur must end dragging.
4. Edit numbers after dragging, and use chart arrow keys/Shift. Reject duplicate temperatures, decreasing speeds, invalid values, and a last point below 100% or above 85 °C.
5. Save/apply, verify independently, restore automatic, and check defaults.
6. Apply, close/reopen via tray, and quit. Verify restoration before exit.
7. Test Windows autostart separately from start in tray. Save the latter with **Save settings**. Restarting must not silently apply the curve.
8. After these checks, evaluate sleep/resume, driver resets, external changes, and forced termination. Record the state left in the driver.

## Zero RPM below 25%

1. Restore automatic. Set points 1 and 2 to 40 °C / 20% and 50 °C / 40%. Keep the rest increasing and reach 100% by 85 °C.
2. Enable **Allow fans to stop**, keep 25%, save, and apply. This gives a conservative 42 °C cutoff. Initial permission requires the hotter sensor below 39 °C; permission is withdrawn at 42 °C.
3. Observe actual RPM. Permission accepted by the driver is not proof the fans physically stopped. Firmware can veto stopping and choose its own restart behavior.
4. Let temperature cross naturally and confirm transitions, restart, and the re-arm band. Do not deliberately overheat the card. A hotter hotspot must keep the gate from stopping the fans.
5. Restore automatic and verify default curve points and Zero RPM state.
6. Test unsupported capability, write/readback failures, missing sensors, and external changes with a fake backend before hardware failure scenarios.

Physical stop/restart validation remains pending. Do not describe this as hardware-verified until these checks pass on the specific board and driver.

## Automated checks and packaging

```powershell
npm run build
cargo fmt --all --check
cargo test --locked --workspace
cargo clippy --locked --workspace --all-targets -- -D warnings
npm run tauri build -- --bundles msi,nsis
```

Automated tests cover curve/policy/configuration and fake-backend failure handling without GPU writes. Browser checks use mocked IPC; they cannot validate ADL/WebView2 behavior.

CI packages MSI, NSIS, and a GUI/CLI ZIP with SHA-256 checksums. Installers bootstrap WebView2; ZIP builds need an existing runtime. Measure release CPU/memory including associated WebView2 processes, separately for visible and hidden windows.

For issues, include GPU/board, driver version, PCI ID, config, timestamps, readings, and actual fan behavior.
