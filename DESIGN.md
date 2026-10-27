---
name: AMD Fan Control
description: A compact, minimal desktop interface for GPU telemetry and fan-curve control.
colors:
  light-background: "#faf8f6"
  light-foreground: "#242224"
  light-panel: "#fff"
  light-line: "#d9d2d0"
  light-muted: "#655d5d"
  light-accent: "#a82326"
  light-focus: "#a82326"
  light-chart-grid: "#e6dfdc"
  light-soft: "#f4e9e7"
  light-error: "#842124"
  dark-background: "#18191c"
  dark-foreground: "#f4f0ed"
  dark-panel: "#202125"
  dark-line: "#494246"
  dark-muted: "#c0b6b8"
  dark-accent: "#ffb3ac"
  dark-focus: "#ffb3ac"
  dark-chart-grid: "#3c383b"
  dark-soft: "#573034"
  dark-error: "#ffb4ab"
  button-fill: "#a82326"
  button-text: "#fff"
typography:
  headline:
    fontFamily: '"Segoe UI", system-ui, sans-serif'
    fontSize: "24px"
    fontWeight: 650
  title:
    fontFamily: '"Segoe UI", system-ui, sans-serif'
    fontSize: "20px"
  body:
    fontFamily: '"Segoe UI", system-ui, sans-serif'
  supporting:
    fontFamily: '"Segoe UI", system-ui, sans-serif'
    fontSize: "13px"
    lineHeight: 1.55
  telemetry:
    fontFamily: '"Segoe UI", system-ui, sans-serif'
    fontSize: "28px"
    fontWeight: 600
  telemetry-wide:
    fontFamily: '"Segoe UI", system-ui, sans-serif'
    fontSize: "38px"
    fontWeight: 600
  error:
    fontSize: "14px"
  settings-group:
    fontSize: "15px"
  compact-version:
    fontSize: "11px"
  button:
    fontFamily: '"Segoe UI", system-ui, sans-serif'
    fontSize: "13px"
    fontWeight: 600
  label:
    fontFamily: '"Segoe UI", system-ui, sans-serif'
    fontSize: "12px"
rounded:
  field: "8px"
  button: "9px"
  error: "10px"
  panel: "12px"
  status: "20px"
spacing:
  tight: "4px"
  compact: "8px"
  control-gap: "10px"
  small: "12px"
  field-gap: "14px"
  medium: "16px"
  section: "20px"
  panel: "22px"
  large: "24px"
  header: "28px"
  page-inline: "32px"
components:
  button-primary:
    backgroundColor: "{colors.button-fill}"
    textColor: "{colors.button-text}"
    typography: "{typography.button}"
    rounded: "{rounded.button}"
    padding: "12px 16px"
  button-secondary-light:
    backgroundColor: "{colors.light-panel}"
    textColor: "{colors.light-accent}"
    typography: "{typography.button}"
    rounded: "{rounded.button}"
    padding: "12px 16px"
  button-secondary-dark:
    backgroundColor: "{colors.dark-panel}"
    textColor: "{colors.dark-accent}"
    typography: "{typography.button}"
    rounded: "{rounded.button}"
    padding: "12px 16px"
  curve-panel-light:
    backgroundColor: "{colors.light-panel}"
    rounded: "{rounded.panel}"
    padding: "22px"
  curve-panel-dark:
    backgroundColor: "{colors.dark-panel}"
    rounded: "{rounded.panel}"
    padding: "22px"
---

# Design System: AMD Fan Control

## Overview

**Creative North Star: "Clean, minimal Material-inspired desktop UI"**

The interface is a compact Windows Tauri utility with English labels, restrained color, familiar native typography, and a direct hierarchy from telemetry to curve editing. System light and dark appearance comes from the operating-system preference. An original SVG three-blade fan in `assets/icon.svg` supplies the product identity without additional imagery or downloaded fonts.

The implemented **Operate** surface keeps four readings, a five-point fan curve, numeric editing, explicit commands, and persistent settings in one window. Low resource use is a binding direction: vanilla TypeScript, native SVG, local assets, and no decorative animation. This document records `index.html`, `src/style.css`, and `src/main.ts`; visual review evidence covers browser captures with mocked IPC in both system themes at compact and wide sizes. The README preview uses unavailable telemetry. These captures validate layout and interaction, not native WebView2 or physical GPU behavior.

**Key Characteristics:**

- Compact desktop hierarchy with telemetry and curve immediately available.
- System appearance, local Segoe UI typography, and one restrained red accent family.
- Flat bordered surfaces, clear focus, and explicit save/apply feedback.
- A lightweight SVG chart paired with keyboard-editable numeric fields.

## Colors

The palette combines warm light neutrals or charcoal dark neutrals with red controls and a softer salmon accent in dark appearance. The frontmatter records exact source values; theme-prefixed keys represent the two `prefers-color-scheme` sets.

### Primary

- **Red accent** (`light-accent`): the light chart, mode text, secondary button text, selection, and focus ring.
- **Soft salmon accent** (`dark-accent`): the corresponding dark-theme chart, text, selection, and focus ring.
- **Filled action red** (`button-fill`): the primary button remains red in both themes, with `button-text` white text.
- **Soft accent surfaces** (`light-soft`, `dark-soft`): the compact mode badge background.
- **Error red** (`light-error`, `dark-error`): the persistent error banner's text and border.

### Neutral

- **Page backgrounds** (`light-background`, `dark-background`): the overall window canvas.
- **Panel surfaces** (`light-panel`, `dark-panel`): curve panel, form fields, secondary buttons, and chart-handle centers.
- **Foregrounds** (`light-foreground`, `dark-foreground`): primary text and inherited field text.
- **Muted text** (`light-muted`, `dark-muted`): telemetry labels, units, chart labels, hints, version, feedback, and footer.
- **Lines** (`light-line`, `dark-line`): panel and field borders, secondary button outlines, and scrollbar color.
- **Chart grid** (`light-chart-grid`, `dark-chart-grid`): thin reference lines behind the curve.

## Typography

Use the local **Segoe UI, system-ui, sans-serif** stack throughout. There is no separate display or monospaced face. The root does not override the browser's base font size or line height; use explicit component roles where the stylesheet defines them.

- **Application heading:** `headline`, reducing to the title size at the narrow breakpoint; weight 650.
- **Section heading:** `title`, with the browser's bold heading weight.
- **Telemetry:** fluid sizing from `telemetry` (28px) to `telemetry-wide` (38px), with tabular numerals. Units use supporting size, regular weight, and muted color.
- **Supporting copy:** `supporting` for header subtitle, hints, and footer line height. Telemetry labels, version, feedback, settings labels, and buttons use the supporting size, with their own inherited line height unless specified.
- **Small labels:** `label` for chart axes, curve-point labels, and mode badge. Chart axes also use tabular numerals.
- **Settings group legend:** 15px, weight 600. Error copy is 14px. The narrow version marker is 11px; the footer is 12px.

## Layout

The Operate surface fills the window with fluid 16–32px page padding and no maximum content width, with tighter vertical spacing on shorter desktops. The header pairs a 44px local fan mark with the title, selected GPU, and alpha version. Four telemetry columns precede the workspace. At 900px and above, the curve shares the workspace with a 300px Settings panel (340px above 1600px). Narrower windows stack the panels. Settings stay visible without a disclosure arrow; GPU and refresh controls share a row.

The chart grows horizontally and vertically with available space. Its SVG is positioned inside an independently sized flex container to avoid intrinsic SVG aspect-ratio feedback during resize. ResizeObserver updates the viewBox to CSS-pixel dimensions, retaining legible labels and 48px pointer targets. Five fieldsets identify the curve points, each with distinct **Temp (°C)** and **Fan (%)** labels. Point values use tabular numerals.

At 700px or less, telemetry becomes two columns and point groups use three columns with stacked fields. Below 420px, point groups use two columns. Settings and footer stack as needed. The page scrolls vertically on short windows; horizontal page overflow is avoided. Browser review covers 680, 920, 1366, and 1920px desktop widths plus a 390px stress check in both system themes across the captures.

## Elevation & Depth

There are no box shadows, gradients, or blur effects. Depth comes from the page/panel color difference and single-pixel borders. Status uses a soft accent fill. Hover darkens buttons with `brightness(.92)` without moving or elevating them. A three-pixel focus outline with a three-pixel offset identifies keyboard focus. Only the hover filter transitions, for 100ms with `ease-out`, when reduced motion is not requested.

## Shapes

Use gently rounded outlined fields, slightly rounder buttons, and the larger panel radius from the frontmatter. The error banner has its own intermediate radius. The mode badge is pill-like through the status radius. Chart points are circles with a 6px radius, panel-colored fill, and a 3px accent stroke. The curve uses rounded line caps and joins.

## Components

### Buttons

The filled primary button saves the curve and settings. Secondary outlined buttons restore driver defaults and save all changes from Settings. All buttons include native hover help that explains their effects. Both variants use button typography, a 44px minimum height, button radius, and frontmatter padding. Secondary borders use the theme line color. Disabled buttons have 0.45 opacity, a default cursor, and no hover filter; keyboard focus uses the shared outline.

A labeled **Use custom fan curve** switch applies the saved curve when enabled and restores driver defaults when disabled. Nearby text explains both outcomes. The switch follows confirmed worker state and remains available for restoration when telemetry or curve capability is lost. Unsaved changes block activation; saving alone does not control the fans. The startup mode reads **Custom control off**, since monitoring does not confirm existing driver settings are defaults.

### Curve panel and chart

The panel uses a flat bordered surface. The SVG has temperature references from 20 to 100 °C and speed references from 0 to 100%. A 3px accent line shows the draft native curve. Each point has a 6px visible handle and a transparent 24px-radius hit target. The chart uses a dynamic viewBox, not a fixed intrinsic aspect ratio.

Dragging captures the pointer on the stable SVG root, retains the initial grab offset, updates existing point nodes, and suppresses text selection. Pointer release, cancellation, lost capture, or window blur ends the gesture. Numeric inputs remain text-selectable. Chart points are keyboard-focusable: left/right adjusts temperature, up/down adjusts speed, and Shift uses steps of five. Numeric fields provide another keyboard alternative.

The optional Zero RPM region uses the soft accent fill and a dashed cutoff line. A visible legend names the cutoff temperature, shaded fan-stop region, firmware discretion, and restriction to custom control. It represents permission to stop below a derived temperature, not a promise of physical 0 RPM. The plotted line remains the requested native curve. The Settings panel describes the cutoff and re-arm band.

Draft changes are locked while a command is pending or custom control is active. Save-before-apply and unsaved state are explicit. No separate saved/applied curve is overlaid.

### Telemetry and status

Readings use large tabular values with muted units and labels. Unavailable or stale readings use an em dash. The mode badge distinguishes inactive custom control from an active custom curve through text. Error messages remain in a bordered `role="alert"` banner, with technical driver diagnostics inside a disclosure. Recovery messages state whether driver defaults were restored and keep failed restoration explicit. Command and settings outcomes have separate status regions. Preserve meaningful text alongside color.

### Fields and settings

Fields use inherited typography, theme panel backgrounds, single-pixel borders, and 44px minimum height. Checkboxes are 18px with the accent color. Settings include Zero RPM permission and its percentage threshold (initially 25%), GPU selection, refresh interval, Windows autostart, and start in tray. Unsupported Zero RPM remains unavailable, but an already enabled draft can always be switched off for recovery.

**Save all changes** and **Save curve & settings** save the whole draft, with unsaved/saving/saved/error feedback. Autostart is an immediate OS command with pending state and rollback on failure; the other settings require saving. Existing profiles without a Zero RPM value preserve disabled behavior.

## Do's and Don'ts

### Do:

- **Do** retain system light/dark behavior and the original local SVG fan mark.
- **Do** keep the curve and telemetry readable within the compact Operate surface.
- **Do** provide numeric keyboard alternatives for pointer editing and visible keyboard focus.
- **Do** distinguish unsaved drafts, saved configuration, active manual control, and unavailable telemetry with explicit text.
- **Do** preserve the dedicated settings save action and its nearby outcome feedback.

### Don't:

- **Don't** add downloaded fonts, decorative imagery, or continuous decorative motion to this utility.
- **Don't** replace the implemented flat borders with unsupported shadows or gradients.
- **Don't** imply that the chart displays a separate saved curve when it displays only the draft.
- **Don't** describe source-defined light or narrow styling as visually captured validation.
