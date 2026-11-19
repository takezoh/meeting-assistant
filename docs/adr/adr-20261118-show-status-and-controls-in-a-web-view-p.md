---
id: adr-20261118-show-status-and-controls-in-a-web-view-p
kind: adr
title: Show status and controls in a web-view panel opened from the tray icon
status: proposed
created: '2026-11-18'
decision_makers:
- assistant
tags: []
owners: []
relations: []
source_paths:
- crates/app/src/panel/view.rs
- crates/app/src/panel/window.rs
- crates/app/src/panel/panel.html
- crates/app/src/tray.rs
summary: The tray icon opens a web-view panel listing detected meetings and the recording,
  with a control for each.
---

# Show status and controls in a web-view panel opened from the tray icon

## Context

The user asked for the recording state and the detected meetings to be visible from the operating system's tray, for each meeting to be recorded and stopped by hand, and for a modern interface. The tray had a coloured dot, a tooltip and a native menu whose first line named one meeting. A native menu cannot show a list with a button per row, a running timer or a switch, and it looks different on each platform.

Options considered:

- **Native menu only.** No new dependency, but rows are plain text items, there is no running time, and the menu closes after each click.
- **Native panels per platform** (AppKit popover, a WinUI flyout). Best fidelity, but the same screen would be written twice in two toolkits, against the rule that procedures live on the shared side.
- **An immediate-mode toolkit drawn by the app** (egui and similar). One implementation, but it brings its own renderer and event loop next to `tao`, and does not look like either platform.
- **The system web view through `wry`**, in a borderless `tao` window next to the icon. One HTML page for both platforms, drawn by WKWebView on macOS and WebView2 on Windows; `wry` uses the same `tao`, `windows` and `raw-window-handle` versions as the app.

## Decision

A left click on the tray icon opens a panel drawn by the system web view. The page is a single self-contained file embedded in the executable; it loads nothing from elsewhere and may not navigate away. What it shows and what its messages mean are decided by a pure module (`panel::view`) and tested there; the page only renders the JSON it is given and posts the user's clicks back. Window styling that differs by platform is a small operation in `platform`: a transparent window over the system material with rounded corners (the popover material on macOS, the acrylic of transient windows on Windows 11, no taskbar button on Windows). The page follows each platform's design with the same markup and script: the platform marks it macOS or Windows and passes the user's accent color where the design uses one, and only design tokens and a few control styles differ.

The native menu stays, with the same actions, and opens with a right click, or with a left click when the panel cannot be created.

## Consequences

- Positive: one screen for both platforms, with a list of meetings, a control for each, a running timer and a switch for automatic recording.
- Negative: the app now depends on the system web view. On Windows that is the WebView2 runtime, which ships with Windows 11 but may be missing on older systems; then the menu is the only interface.
- Negative: the page is not exercised by `cargo test`. Its messages and the content it is given are tested on the Rust side; its rendering was only checked in a browser with sample content.
- Neutral: the macOS translucent material uses a private `WKWebView` key through `wry` and `window-vibrancy`; if it fails the page draws an opaque background.
- Neutral: on Windows the material shows only on a window without the frame that gives a borderless window its shadow; with that frame the page stayed white (observed 2026-11-18). The window is therefore created without it, and the frame is put back where Windows refuses the material.

## Confirmation

`crates/app/src/panel/view.rs` tests the content, the message parsing and the panel's position. `crates/app/src/architecture.rs` keeps `panel/view.rs` free of effects and keeps `window_vibrancy` and the `tao` platform extensions inside `platform`.
