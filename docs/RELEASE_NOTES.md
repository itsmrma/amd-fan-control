Windows x64 alpha, built around the AMD Radeon RX 5600 XT.

- English interface, tray messages, diagnostics, and documentation.
- Responsive layout, persistent Settings panel, and bundled local logo.
- Resizable SVG curve, explicit temperature/fan labels, keyboard controls, and dragging without text selection or grab-offset jumps.
- Five-point native Overdrive8 writes with readback verification.
- Optional Zero RPM permission below a configurable curve-speed threshold, initially 25%, with a 3 °C re-arm band and hotter-sensor policy.
- Edge/hotspot temperatures, RPM, and driver-reported speed percentage.
- Fan-only restoration, tray, autostart, and atomic JSON configuration.
- MSI, NSIS, and portable GUI/CLI packages with SHA-256 checksums.

Read-only capabilities and telemetry were observed on one RX 5600 XT. Hardware writes and physical stop/restart still require TESTING.md. Other RDNA generations are candidates, not confirmed supported hardware.

Startup is monitoring only. Closing hides to tray. Quit attempts restoration and stays open on failure. Forced termination cannot clean up: the native curve and last Zero RPM flag may remain, and the software gate stops updating. Firmware retains its own fan conditions. No independent watchdog exists.

Zero RPM requires native driver support; permission does not force physical 0 RPM. Linux/sysfs and legacy ADL control backends are not implemented. Portable builds need WebView2 installed. Alpha binaries are unsigned.
