Windows x64 alpha 2, built around the AMD Radeon RX 5600 XT.

Changes since alpha 1:

- **Selectable curve temperature** above the chart: normal GPU temperature (default), junction/hotspot, or the previous driver/firmware mode. The chart and Zero RPM policy follow the saved selection; both temperature safeguards remain active.
- GPU/hotspot modes update requested speed once per second through verified OD8 speed holds, retaining native emergency escalation. Missing selected sensors and rejected changes trigger recovery. Physical fan response for these new modes still needs hardware validation.
- Curve fields adapt to panel width, with consistent input sizes and readable labels. Settings move below the curve before either panel gets cramped. Wide windows keep the two panels aligned.
- SHA-256 entries use the exact published installer filenames, including GitHub's filename normalization.
- Desktop layout fits the available window height without unnecessary scrolling; small windows retain scrolling to keep controls reachable.
- **Use custom fan curve** switch: on applies your saved curve; off restores the driver's default fan curve and Zero RPM setting. Your saved curve is kept.
- The custom-control choice is saved immediately and restored on the same GPU at app startup, including Windows login, after telemetry and driver checks pass. Turning it off cancels restoration; normal quit keeps the choice for the next launch. Older profiles remain off until explicitly enabled.
- Clearer button labels and hover explanations for saving, custom control, and driver-default restoration.
- The chart explains the dashed Zero RPM cutoff and shaded fan-stop region, including firmware discretion.
- Readable recovery messages explain unsupported Zero RPM changes (ADL -8), state whether driver defaults were restored, and keep diagnostics under **Technical details**.
- Failed commands refresh the confirmed control state. Restoration remains available when telemetry or curve capability is lost.
- Browser regression tests cover both themes, 390–1920 px widths, resizing, dragging/keyboard input, sensor selection, saving, control switching, and recovery. Rust tests cover sensor policy, native fallback ramps, skipped writes, missing sensors, external changes, rejected writes, and restore outcomes.

Downloads: Windows x64 MSI installer, NSIS setup executable, and portable GUI/CLI ZIP, with SHA-256 checksums. Installers handle WebView2; portable builds need it already installed. Alpha binaries are unsigned.

Hardware notes:

- Read-only capabilities and telemetry were observed on one RX 5600 XT. Fan writes and physical stop/restart still require the procedure in `docs/TESTING.md`. Other RDNA generations are candidates, not confirmed supported hardware.
- Zero RPM permission does not force physical 0 RPM. A readable capability does not guarantee the driver accepts changes. If ADL -8 appears, disable **Allow fans to stop**, save, and retry the custom curve. If restoration fails, keep the app open and retry **Use driver defaults**.
- Startup resumes a previously enabled custom curve only after fresh telemetry and the normal activation checks. Failed restoration is reported without repeated automatic writes. Closing hides to tray. Quit attempts driver-default restoration and stays open on failure, retaining the startup preference.
- Forced termination cannot clean up: the native curve and last Zero RPM flag may remain while the software gate stops updating. Firmware retains its own fan conditions. No independent watchdog exists.
- Linux/sysfs and legacy ADL control backends are not implemented.
