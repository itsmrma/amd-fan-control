# Product

<!-- impeccable:product-schema 1 -->

## Platform

web

Desktop Tauri application with a local web interface. Windows 10/11 first;
hardware abstraction ready for Linux. macOS GPU control is outside the scope.

## Stack

Rust + Tauri v2; vanilla TypeScript and SVG (small bundle, choice delegated by user).

## Users

Owner of an AMD Radeon RX 5600 XT who needs reliable fan curves without the
overhead and profile issues experienced with MSI Afterburner.

## Product Purpose

Edit a temperature-to-fan-speed curve, monitor actual GPU telemetry, apply the
curve explicitly, and recover the driver's default fan settings.

## Capabilities and Constraints

Use runtime ADL capabilities. User prioritizes reliability over the control
mechanism and accepts an always-running app. Native OD8 curve is the initial
Windows implementation. Never claim a process destructor runs after a forced kill.
Tray, login autostart, local JSON, system dark mode, and compact Material styling.
UI and all user-facing documentation in English. Alpha packaging requested as a GitHub prerelease.

## Evidence on Hand

RX 5600 XT detected locally with OD8 curve capability. Read-only native probe
confirmed fan curve limits and temperature/ventilation telemetry on 2026-10-27.
Fan writes have not yet been validated on hardware.

