Windows x64 alpha 5, built around the AMD Radeon RX 5600 XT.

Changes since alpha 4:

- **Backup moved into Settings.** Export and import of the JSON settings file now live in a dedicated **Backup** section at the bottom of the Settings panel, with balanced spacing around the save row.
- **Compact update messages.** The update banner no longer prints the whole release notes inline: notes are collapsed under **Release notes**, the "you are up to date" confirmation disappears on its own after a few seconds, and every message can be dismissed with the × button.
- **NSIS setup and portable ZIP only.** The MSI installer is discontinued; downloads are the NSIS `-setup.exe` and the portable GUI/CLI ZIP, with SHA-256 checksums. Existing MSI installs can switch by running the NSIS setup.
- CI fix: a malformed `if:` expression in the release workflow prevented the required build checks from running on pull requests; the workflow now parses and reports checks on every PR.
- Version metadata is aligned at `0.1.0-alpha5`.

Downloads: Windows x64 NSIS setup executable and portable GUI/CLI ZIP, with SHA-256 checksums. The setup bootstraps WebView2; portable builds need it already installed. Alpha installers are not Authenticode-signed by Windows; the in-app updater still requires a valid project signature.

Hardware notes:

- Read-only capabilities and telemetry were observed on one RX 5600 XT. Fan writes, physical stop/restart and reboot behavior still require the procedure in `docs/TESTING.md`. Automated tests use mocked hardware. Other RDNA generations are candidates, not confirmed supported hardware.
- Zero RPM permission does not force physical 0 RPM. A readable capability does not guarantee the driver accepts changes. If ADL -8 appears, disable **Allow fans to stop**, save, and retry the custom curve. If restoration fails, keep the app open and retry **Use driver defaults**.
- Closing hides to tray. Quit attempts driver-default restoration and stays open on failure, retaining the startup preference.
- Forced termination cannot clean up: the native curve and last Zero RPM flag may remain while the software gate stops updating. Firmware retains its own fan conditions. No independent watchdog exists.
- Linux/sysfs and legacy ADL control backends are not implemented.
