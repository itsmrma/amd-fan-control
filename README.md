<p align="center"><img src="assets/logo.svg" width="760" alt="AMD Fan Control" /></p>

<p align="center">A small Windows desktop utility for AMD GPU fan curves, built with Rust and Tauri.</p>

<p align="center"><a href="https://github.com/itsmrma/amd-fan-control/releases">Downloads</a> · <a href="docs/ARCHITECTURE.md">Architecture</a> · <a href="docs/TESTING.md">Hardware testing</a></p>

## Why I built this

I got tired of using MSI Afterburner, so I built a fan-control app for my own graphics card: an **AMD Radeon RX 5600 XT**. I wanted a lightweight way to set a fan curve, read temperatures and fan speeds, and get back to the driver's defaults.

This is a personal project built around that card. **It is not a universal AMD fan controller, and I cannot guarantee it will work on other cards.** Even another RX 5600 XT can behave differently with a different board BIOS or driver.

![AMD Fan Control interface preview; telemetry is unavailable in this browser capture](assets/screenshot.png)

## Compatibility

Windows 10/11 x64 with an AMD display driver and Microsoft Edge WebView2 Runtime. The backend uses **ADL Overdrive8 native five-point fan curves**. Architecture names alone do not establish compatibility: the driver must expose curve controls, point limits, and temperature sensors.

These desktop Radeon families are reasonable candidates for the same approach, **not a tested support list**:

| AMD architecture | Desktop GPU family | Project status |
| --- | --- | --- |
| RDNA / Navi 1x | RX 5000, including my RX 5600 XT | Development target. Read-only curve capabilities, limits, telemetry, and Zero RPM toggle observed locally. Hardware writes still need validation. |
| RDNA 2 / Navi 2x | RX 6000 | May work if the driver exposes the required ADL controls. Untested. |
| RDNA 3 / Navi 3x | RX 7000 | May work if the driver exposes the required ADL controls. Untested. |
| RDNA 4 / Navi 4x | RX 9000 | Candidate only; newer hardware/driver interfaces may differ. Untested. |

Family names follow [AMD's RDNA documentation](https://www.amd.com/en/technologies/rdna.html). Possible compatibility is an implementation-based expectation, not an AMD support claim. See the [official ADL SDK](https://github.com/GPUOpen-LibrariesAndSDKs/display-library) for the underlying APIs.

Older GCN cards (Polaris/Vega), integrated graphics, laptop-controlled fans, and Radeon Pro cards are outside the validated scope. Legacy Overdrive5/6/N fan-control backends are not implemented. Linux has a reusable Rust hardware interface, but no working sysfs/hwmon backend yet.

## Features

- Resizable SVG curve editor with dragging, keyboard adjustment, and separate temperature/fan-speed labels.
- GPU edge temperature, junction/hotspot, fan RPM, and driver-reported speed percentage.
- Explicit save/apply and driver readback verification.
- Optional **Zero RPM below a configurable curve-speed threshold**, initially **25%**.
- Persistent Settings panel, beside the curve in wide windows and below it in smaller windows.
- Automatic fan-default restoration, tray, optional Windows autostart, and start in tray.
- System light/dark appearance, local JSON, and a small vanilla TypeScript frontend.
- MSI, NSIS installer, and portable ZIP builds through GitHub Actions.

Only fan-related settings are written. Clocks, voltages, and power limits are untouched.

## Getting started

Download from [Releases](https://github.com/itsmrma/amd-fan-control/releases). Installers handle WebView2; portable builds require it already installed. Alpha binaries are unsigned. ADL is loaded from the AMD driver's `atiadlxx.dll` in System32; no separate DLL download is needed.

1. Close MSI Afterburner and disable other fan controllers, including Radeon Software fan tuning.
2. Open the app and check temperatures and RPM. Select your GPU in **Settings** if necessary.
3. Drag the five points or edit **Temp (°C)** and **Fan (%)**. Focus a chart point to use left/right for temperature and up/down for speed; Shift uses steps of five.
4. Click **Save curve**, then **Apply curve**. Saving alone does not change the fans.
5. Click **Restore automatic** to request the driver's fan defaults.

Closing the window keeps the app in the tray. **Quit** from the tray menu attempts restoration before exiting; if it fails, the app stays open and shows the error.

The app starts in monitoring mode and never silently reapplies a curve. Configuration lives at `%APPDATA%\amd-fan-control\config.json`. Windows autostart takes effect immediately; other settings require **Save settings**, which saves the whole draft including the curve.

### Zero RPM

Enable **Allow fans to stop** and set **When curve speed is below**. The initial threshold is **25%**. Set the first point below that value, for example **40 °C / 20%**, so the curve enters the fan-stop region. Existing profiles keep Zero RPM disabled until you enable and save it.

The threshold refers to **speed requested by the curve**, not measured fan percentage. The app interpolates a temperature cutoff, rounded down to whole degrees. To avoid cycling, it requests fan stop only when the hotter available sensor is more than 3 °C below that cutoff, and disables permission at the cutoff. The cutoff cannot exceed 60 °C. The shaded region shows the cutoff; the line remains the requested native curve.

The worker toggles native Zero RPM permission at polling intervals and verifies it. **This requests fan stop; it does not force electrical 0% PWM.** Firmware may keep the fans running because of temperature, power, or restart conditions. Confirm actual stopping through RPM; an enabled flag is not proof of 0 RPM. Unsupported drivers leave this option unavailable.

Read-only probing confirmed the local RX 5600 XT's toggle but no adjustable native stop-temperature range. The running worker therefore manages the percentage threshold, while the native curve and firmware protections remain in place.

## Alpha limitations

- Hardware writes, physical stop/restart, and lifecycle behavior still need [hardware validation](docs/TESTING.md). Automated checks do not certify fan behavior.
- Exactly five points: increasing temperatures, non-decreasing speeds, and 100% by 85 °C. Running speeds must be 20–100% and within driver limits; stopping uses the separate Zero RPM flag.
- The worker attempts restoration at edge ≥90 °C or hotspot ≥105 °C, repeated telemetry failures, long polling stalls, and detected external changes. These are application safeguards, not official GPU limits.
- No independent watchdog. Forced termination cannot clean up: the native curve and last Zero RPM flag may remain, while the software gate stops updating. Firmware still determines restart. Immediate restoration after crashes, driver resets, or sleep is not guaranteed.
- Reported percentage is the driver's RPM/max-RPM ratio, not electrical PWM duty. Missing/stale sensors show an em dash.
- Firmware controls native-curve execution and restart hysteresis. The software gate uses the hotter reported sensor, which may differ from the native curve's internal sensor.

## Development

Prerequisites: Node.js 22 LTS or newer, Rust stable/MSVC ≥1.90, Visual Studio Build Tools with **Desktop development with C++**, Windows SDK, and WebView2 Runtime. Pinned AMD ADL headers are included.

```powershell
git clone https://github.com/itsmrma/amd-fan-control.git
cd amd-fan-control
npm ci
npm run tauri dev
```

Read-only diagnostics:

```powershell
cargo run --locked -p fan-cli -- list
# Replace with the exact PCI ID returned by list.
cargo run --locked -p fan-cli -- probe 'pci:41:0:0'
```

`probe` reads capabilities, the curve, and three samples without writing fan settings. The explicit `test-curve` command runs a 15-second hardware test and then requests defaults; read [the procedure](docs/TESTING.md) first.

```powershell
npm run build
cargo fmt --all --check
cargo test --locked --workspace
cargo clippy --locked --workspace --all-targets -- -D warnings
npm run tauri build -- --bundles msi,nsis
```

| Directory | Purpose |
| --- | --- |
| `crates/fan-core/` | Curve validation/interpolation, Zero RPM policy, JSON, hardware trait |
| `crates/amd-adl/` | Rust ADL wrapper, C++ bridge, pinned headers |
| `crates/fan-cli/` | Diagnostics, timed test, restoration |
| `src-tauri/src/` | Commands, hardware worker, tray, autostart |
| `src/` | TypeScript, CSS, responsive SVG chart |
| `assets/` | Original logo/icon and interface preview |

See [ARCHITECTURE.md](docs/ARCHITECTURE.md) for ownership and recovery details.

## Builds and releases

[CI](.github/workflows/build.yml) runs core tests on Linux, Windows tests/lint, and MSI/NSIS/ZIP packaging. Normal builds produce Actions artifacts; a `v*` tag publishes a prerelease after checks pass, with SHA-256 checksums. Release publishing uses `GITHUB_TOKEN`, with `contents: write` scoped to the release job.

Keep `Cargo.toml`, `package.json`, lockfiles, and `src-tauri/tauri.conf.json` versions aligned before tagging. MSI uses a separate numeric version.

MIT. Independent project, not affiliated with AMD or MSI. [Third-party notices](THIRD_PARTY_NOTICES.md).
