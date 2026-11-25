Windows x64 and Linux x64 alpha 6, introducing the Linux `amdgpu` sysfs backend and Linux packaging.

Changes since alpha 5:

- **Linux `amdgpu` sysfs backend.** Added Linux GPU discovery and telemetry (`amdgpu` / `hwmon` sysfs), host-PWM fan curve control with StepDown smoothing, `flock` single-writer locking, automatic driver-default restoration on exit, and Zero RPM gating.
- **Linux packaging and udev rules.** Added `.deb`, `.rpm`, and `.AppImage` builds alongside the `amd-fan-control-linux-x64-cli.tar.gz` diagnostic archive, bundling `60-amd-fan-control.rules` for non-root `hwmon` permissions and actionable permission guidance in the UI.
- **Linux updater support.** Added signed AppImage in-app updates via the `latest.json` manifest (`linux-x86_64` and `windows-x86_64`), and package-managed update guidance for `.deb` and `.rpm` installations.
- **OS-neutral capabilities contract and docs.** Prefactored the backend capabilities contract and UI, and added Linux backend, packaging, udev, and validation documentation.
- Version metadata is aligned at `0.1.0-alpha6`.

Downloads:
- **Windows x64:** NSIS `-setup.exe` (with `.sig`) and portable GUI/CLI ZIP.
- **Linux x64:** `.deb`, `.rpm`, `.AppImage` (with `.sig`), and `amd-fan-control-linux-x64-cli.tar.gz`.
- **Verification:** `SHA256SUMS.txt` and `latest.json` updater manifest.

Alpha installers are not Authenticode-signed by Windows; the in-app updater requires a valid project signature for NSIS and AppImage updates.

Hardware notes:

- Windows read-only capabilities and telemetry were observed on one RX 5600 XT. Linux `amdgpu` sysfs and Windows fan writes, physical stop/restart, and reboot behavior still require hardware validation per `docs/TESTING.md`. Automated tests use mocked hardware/sysfs fixtures.
- Zero RPM permission does not force physical 0 RPM. If restoration fails, keep the app open and retry **Use driver defaults**.
- Closing hides to tray. Quit attempts driver-default restoration and stays open on failure, retaining the startup preference.
- Forced termination cannot clean up: the native curve/PWM mode and last Zero RPM flag may remain while the software gate stops updating.
