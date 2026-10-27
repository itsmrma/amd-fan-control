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

The interface is a compact Windows Tauri utility with Italian labels, restrained color, familiar native typography, and a direct hierarchy from telemetry to curve editing. System light and dark appearance comes from the operating-system preference. An original SVG three-blade fan in `assets/icon.svg` supplies the product identity without additional imagery or downloaded fonts.

The implemented **Operate** surface keeps four readings, a five-point fan curve, numeric editing, explicit commands, and expandable settings in one window. Low resource use is a binding direction: vanilla TypeScript, native SVG, local assets, and no decorative animation. This document records `index.html`, `src/style.css`, and `src/main.ts`; visual review evidence covers the captured Windows dark desktop alpha. Light appearance and the narrow layout below are implemented CSS behavior, not claims of captured validation.

**Key Characteristics:**

- Compact desktop hierarchy with telemetry and curve immediately available.
- System appearance, local Segoe UI typography, and one restrained red accent family.
- Flat bordered surfaces, clear focus, and explicit save/apply feedback.
- A lightweight SVG chart paired with keyboard-editable numeric fields.

## Colors

The palette combines warm light neutrals or charcoal dark neutrals with red controls and a softer salmon accent in dark appearance. The frontmatter records exact source values; theme-prefixed keys represent the two `prefers-color-scheme` sets.

### Primary

- **Red accent** (`light-accent`): the light chart, mode text, secondary button text, caret, selection, and focus ring.
- **Soft salmon accent** (`dark-accent`): the corresponding dark-theme chart, text, caret, selection, and focus ring.
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
- **Telemetry:** `telemetry`, with tabular numerals. Units use supporting size, regular weight, and muted color.
- **Supporting copy:** `supporting` for header subtitle, hints, and footer line height. Telemetry labels, version, feedback, settings labels, and buttons use the supporting size, with their own inherited line height unless specified.
- **Small labels:** `label` for chart axes, curve-point labels, and mode badge. Chart axes also use tabular numerals.
- **Settings disclosure:** 15px, weight 600. Error copy is 14px. The narrow version marker is 11px; the footer is 12px.

## Layout

The Operate surface uses a centered main container with a maximum width of 1040px and page padding of 28px vertically and 32px horizontally. The header aligns a 44px identity mark, title/subtitle, and alpha version marker. Four equal telemetry columns sit above one curve panel. The panel has 22px padding; its heading and mode badge share a row.

Five equal point groups follow the full-width chart. Each group contains adjacent temperature and percentage fields. Commands wrap with a 10px gap. Settings use a native disclosure, a two-column form with a 16px gap, a dedicated save button, and an adjacent status message. The footer separates its two instructions across a row.

The implementation uses several small spacing steps rather than a strict 8px grid; the frontmatter records reusable observed values. At widths of 700px or less, page padding becomes 20px, telemetry becomes two columns, panel padding becomes 16px, numeric pairs stack within their five-column group, settings become one column, and footer instructions stack. This behavior is source-defined; minimum-size visual capture remains outside the documented review evidence.

## Elevation & Depth

There are no box shadows, gradients, or blur effects. Depth comes from the page/panel color difference and single-pixel borders. Status uses a soft accent fill. Hover darkens buttons with `brightness(.92)` without moving or elevating them. A three-pixel focus outline with a three-pixel offset identifies keyboard focus. Only the hover filter transitions, for 100ms with `ease-out`, when reduced motion is not requested.

## Shapes

Use gently rounded outlined fields, slightly rounder buttons, and the larger panel radius from the frontmatter. The error banner has its own intermediate radius. The mode badge is pill-like through the status radius. Chart points are circles with a 6px radius, panel-colored fill, and a 3px accent stroke. The curve uses rounded line caps and joins.

## Components

### Buttons

The filled primary button applies the manual curve. Secondary outlined buttons save the curve, restore automatic/default control, and save settings. Both variants use button typography, a 44px minimum height, button radius, and frontmatter padding. Secondary borders use the theme line color. Disabled buttons have 0.45 opacity, a default cursor, and no hover filter; keyboard focus uses the shared outline.

### Curve panel and chart

The curve panel uses the theme panel surface and a single-pixel line border. The native SVG has a `760 252` viewBox, with temperature reference labels from 20 to 100 °C and speed labels from 0 to 100%. A 3px accent polyline displays the current draft and extends horizontally to the graph bounds. Each of its five visible handles has a transparent 22px-radius pointer target in SVG coordinates; the target scales with the SVG.

Dragging updates the draft line and numeric fields. Each point also has separately named number inputs for temperature and percentage, providing the keyboard alternative. Chart handles themselves do not have keyboard interaction. Controls stop editing during a pending command or active manual mode. Unsaved status and the save-before-apply requirement are conveyed through text and disabled controls; the chart does not overlay a separate saved/applied curve.

### Telemetry and status

Readings use large tabular values with muted units and labels. Unavailable or stale readings use an em dash. The mode badge distinguishes monitoring from an active custom curve through text. Error messages remain in a bordered `role="alert"` banner; command and settings outcomes have separate status regions. Preserve meaningful text alongside color.

### Fields and settings

Fields use inherited typography and foreground, the theme panel background, a single-pixel line border, field radius, and 10px padding. Curve number inputs use tighter 8px vertical/5px horizontal padding. Checkboxes use the accent and 18px dimensions. The native settings disclosure reveals GPU, polling interval, Windows autostart, and start-in-tray controls.

**Salva impostazioni** shares the same draft-save operation as **Salva curva**, with unsaved, saving, saved, and failed feedback near settings. It becomes available for a dirty draft and is disabled while pending or manual control is active. Autostart is an immediate checkbox command, with disabled pending state and rollback on error. Preserve the distinction in future UI changes.

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
