Windows x64 alpha 3, built around the AMD Radeon RX 5600 XT.

Changes since alpha 2:

- **Custom fan control survives restarts.** Enabling **Use custom fan curve** immediately saves the choice and selected GPU. On the next app launch, including Windows login, the saved curve resumes after fresh telemetry and the normal sensor, temperature and driver checks pass.
- Turning the switch off or using **Use driver defaults** cancels startup restoration. **Quit** restores driver defaults for the current session while retaining the choice for the next launch. Unsafe readings or rejected writes show an error without repeated automatic activation attempts.
- Older profiles remain off until explicitly enabled: after installing this update, turn the switch on once to save your startup choice. The old version did not record whether custom control was active.
- Failed preference saves trigger recovery; older settings drafts cannot overwrite the switch choice or lose the pinned GPU. Rust and browser regression tests cover persistence, restart, recovery and legacy profiles.
- All GitHub Actions references are pinned to verified commit SHAs. Tauri's GLib 0.18 dependency uses a documented local backport for RUSTSEC-2024-0429, plus pointer-slice cleanup that passes CodeQL. Optimized Linux regression tests exercise the affected library code.
- Updated dependencies, including TypeScript 7, Vite 8 and dirs 7.

Downloads: Windows x64 MSI installer, NSIS setup executable, and portable GUI/CLI ZIP, with SHA-256 checksums. Installers handle WebView2; portable builds need it already installed. Alpha binaries are unsigned.

Hardware notes:

- Read-only capabilities and telemetry were observed on one RX 5600 XT. Fan writes, physical stop/restart and reboot behavior still require the procedure in `docs/TESTING.md`. Automated tests use mocked hardware. Other RDNA generations are candidates, not confirmed supported hardware.
- Zero RPM permission does not force physical 0 RPM. A readable capability does not guarantee the driver accepts changes. If ADL -8 appears, disable **Allow fans to stop**, save, and retry the custom curve. If restoration fails, keep the app open and retry **Use driver defaults**.
- Closing hides to tray. Quit attempts driver-default restoration and stays open on failure, retaining the startup preference.
- Forced termination cannot clean up: the native curve and last Zero RPM flag may remain while the software gate stops updating. Firmware retains its own fan conditions. No independent watchdog exists.
- Linux/sysfs and legacy ADL control backends are not implemented.
