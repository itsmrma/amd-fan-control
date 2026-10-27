<p align="center"><img src="assets/logo.svg" width="760" alt="AMD Fan Control" /></p>

<p align="center">A small Windows desktop utility for AMD GPU fan curves, built with Rust and Tauri.</p>

<p align="center"><a href="https://github.com/itsmrma/amd-fan-control/releases">Downloads</a> · <a href="docs/ARCHITECTURE.md">Architecture</a> · <a href="docs/TESTING.md">Hardware testing</a></p>

## Why I built this

I got tired of using MSI Afterburner, so I built a fan-control app for my own graphics card: an **AMD Radeon RX 5600 XT**. I wanted a lightweight way to set a fan curve, read temperatures and fan speeds, and get back to the driver's defaults.

This is a personal project built around that card. **It is not a universal AMD fan controller, and I cannot guarantee it will work on other cards.** Even another RX 5600 XT can behave differently with a different board BIOS or driver.

![AMD Fan Control interface preview with simulated telemetry from browser tests](assets/screenshot.png)

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
- Choose **GPU temperature** (default), **Junction / hotspot**, or **Driver / firmware sensor** above the chart. The selection is saved with your curve.
- A custom-curve switch: on applies your saved curve; off restores driver fan defaults, with driver readback verification.
- Descriptive button labels and hover help, plus a labeled Zero RPM cutoff on the chart.
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
4. Click **Save curve & settings**, then turn on **Use custom fan curve**. Saving alone does not change the fans.
5. Turn the switch off, or click **Use driver defaults**, to restore the driver's fan curve and Zero RPM defaults. Your saved curve is kept.

Closing the window keeps the app in the tray. **Quit** from the tray menu attempts restoration before exiting; if it fails, the app stays open and shows the error.

The app starts in monitoring mode with custom control off and never silently reapplies a curve. It does not reset existing GPU settings at startup. Configuration lives at `%APPDATA%\amd-fan-control\config.json`. Windows autostart takes effect immediately; other settings require **Save all changes**, which saves the whole draft including the curve.

### Curve temperature

**GPU temperature** uses the normal edge reading; **Junction / hotspot** uses that sensor explicitly. Profiles without a selection now default to GPU temperature; startup still leaves custom control off. A missing selected sensor blocks activation or stops active control with a restoration attempt. There is no silent substitution.

These two modes interpolate the saved curve once per second, updating OD8 only when requested speed or Zero RPM changes. The native curve holds that speed across the normal firmware temperature range, retaining a final ramp to 100% at the driver's maximum curve temperature (capped at 100 °C). Firmware can raise speed in that final ramp. This avoids assuming which internal sensor the driver uses. Edge ≥90 °C and hotspot ≥105 °C safeguards remain active for either selection. New sensor-based writes and physical fan response require separate hardware validation.

**Driver / firmware sensor** retains the previous behavior: the driver executes the five saved points using its internal sensor. Zero RPM uses the hotter available reading in this mode. If the driver rejects sensor-based control, restore defaults and select this mode.

### Zero RPM

Enable **Allow fans to stop** and set **When curve speed is below**. The initial threshold is **25%**. Set the first point below that value, for example **40 °C / 20%**, so the curve enters the fan-stop region. Existing profiles keep Zero RPM disabled until you enable and save it.

The threshold refers to **speed requested by the curve**, not measured fan percentage. The app interpolates a temperature cutoff, rounded down to whole degrees. It requests fan stop when the selected sensor is more than 3 °C below that cutoff, and disables permission at the cutoff. Firmware mode uses the hotter available reading. The cutoff cannot exceed 60 °C. The dashed line and shaded region show where fans may stop while custom control is on. The solid line shows your requested curve.

The worker toggles native Zero RPM permission at polling intervals and verifies it. **This requests fan stop; it does not force electrical 0% PWM.** Firmware may keep the fans running because of temperature, power, or restart conditions. Confirm actual stopping through RPM; an enabled flag is not proof of 0 RPM. Unsupported drivers leave this option unavailable.

Read-only probing confirmed the local RX 5600 XT's toggle but no adjustable native stop-temperature range. The running worker therefore manages the percentage threshold, while the native curve and firmware protections remain in place.

If the driver rejects a Zero RPM change with **ADL -8** (operation not supported), the app attempts to restore driver defaults and states whether that succeeded. Turn off **Allow fans to stop**, save, and try the custom curve again. A readable capability does not guarantee the driver accepts writes. Expand **Technical details** to see the driver error. If restoration fails, keep the app open and use **Use driver defaults** to retry.

## Alpha limitations

- Hardware writes, physical stop/restart, and lifecycle behavior still need [hardware validation](docs/TESTING.md). Automated checks do not certify fan behavior.
- Exactly five points: increasing temperatures, non-decreasing speeds, and 100% by 85 °C. Running speeds must be 20–100% and within driver limits; stopping uses the separate Zero RPM flag.
- The worker attempts restoration at edge ≥90 °C or hotspot ≥105 °C, repeated telemetry failures, long polling stalls, and detected external changes. These are application safeguards, not official GPU limits.
- No independent watchdog. Forced termination cannot clean up: the native curve and last Zero RPM flag may remain, while the software gate stops updating. Firmware still determines restart. Immediate restoration after crashes, driver resets, or sleep is not guaranteed.
- Reported percentage is the driver's RPM/max-RPM ratio, not electrical PWM duty. Missing/stale sensors show an em dash.
- Firmware retains native emergency escalation and restart conditions. Driver / firmware sensor mode does not expose which internal temperature drives its curve.

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
npx playwright install chromium
npm run test:ui
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
