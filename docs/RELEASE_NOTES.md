Windows x64 alpha 4, built around the AMD Radeon RX 5600 XT.

Changes since alpha 3:

- **Check for updates** is now in the header. It asks GitHub for a newer release, then offers a one-click install. The app verifies the package's Tauri minisign signature, runs the NSIS installer, and restarts. Update checks are manual only.
- Every Windows artifact is built by GitHub Actions. Release files carry GitHub build-provenance attestations, and installers are signed by the release workflow with the project's update key. The private key exists only in Actions secrets.
- The rolling `updater` release publishes `latest.json`, so prerelease tags such as alpha 4 are discoverable without a non-prerelease release.
- Security triage for the vendored GLib 0.18 backport: the unreachable log-level panic no longer formats raw values, debug-only reference-count reads were removed, and `GValue` FFI helpers now reject null pointers before dereferencing. The remaining CodeQL reports are documented FFI false positives in this Linux-only dependency; no advisory is suppressed and the RUSTSEC-2024-0429 fix remains.
- `main` now requires the portable core, Windows, and CodeQL checks before merging, with stale branches blocked and force pushes/deletions disabled.
- Version metadata is aligned at `0.1.0-alpha.4`; the MSI numeric version is `0.1.0.4`.

Downloads: Windows x64 MSI installer, NSIS setup executable, and portable GUI/CLI ZIP, with SHA-256 checksums. Installers handle WebView2; portable builds need it already installed. Alpha installers are not Authenticode-signed by Windows; the in-app updater still requires a valid project signature.

Hardware notes:

- Read-only capabilities and telemetry were observed on one RX 5600 XT. Fan writes, physical stop/restart and reboot behavior still require the procedure in `docs/TESTING.md`. Automated tests use mocked hardware. Other RDNA generations are candidates, not confirmed supported hardware.
- Zero RPM permission does not force physical 0 RPM. A readable capability does not guarantee the driver accepts changes. If ADL -8 appears, disable **Allow fans to stop**, save, and retry the custom curve. If restoration fails, keep the app open and retry **Use driver defaults**.
- Closing hides to tray. Quit attempts driver-default restoration and stays open on failure, retaining the startup preference.
- Forced termination cannot clean up: the native curve and last Zero RPM flag may remain while the software gate stops updating. Firmware retains its own fan conditions. No independent watchdog exists.
- Linux/sysfs and legacy ADL control backends are not implemented.
