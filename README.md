# Clipo

[![License: MIT](https://img.shields.io/badge/License-MIT-blue.svg)](LICENSE)

Clipo is a fast, lightweight, cross-platform clipboard history and snippet manager built with **Tauri v2**, **Rust**, **Svelte 5**, **TypeScript**, **Tailwind CSS v4**, and **SQLite**.

Inspired by [Maccy](https://maccy.app/), Clipo lives unobtrusively in your system tray, pops up at your cursor or active window with a keyboard-first workflow, and keeps your clipboard history and reusable snippets local on your machine.

---

## Features

- **Unified History & Snippets Search**: Instant filtering across clipboard history and reusable snippets with four search modes (`Fuzzy`, `Exact`, `Regex`, and `Mixed`) plus filter tabs (`All`, `Snippets`, `Images`, `Files`).
- **Quick Pick (`1..9`)**: Jump straight to any of the top 9 results with `Cmd+1..9` (macOS) or `Ctrl+1..9` (Windows/Linux), or assign custom hotkeys to pinned items and snippets.
- **Rich & Plain Copy / Paste**: Preserves plain text, RTF, HTML, PNG images, and file lists; supports automatic paste, copy-only, or paste-without-formatting.
- **Attached Preview Sidecar**: Expandable preview pane with syntax highlighting, pretty-printable JSON, image dimensions & size, file paths, metadata (source app, copy count, timestamps), and live HEX/RGB/HSL color swatches.
- **Reusable Snippets & Templates**: Save history items or create snippets with tags, language hints, pin-to-top, optional shortcuts, versioned JSON import/export (`version: 1`), and built-in template variables:
  - `{{date}}` — current local date (`YYYY-MM-DD`)
  - `{{date:<fmt>}}` — custom `strftime` date format (e.g. `{{date:%Y/%m/%d}}`)
  - `{{time}}` — current local time (`HH:MM:SS`)
  - `{{datetime}}` — current local date and time (`YYYY-MM-DD HH:MM:SS`)
  - `{{clipboard}}` — latest clipboard plain text
  - `{{uuid}}` — freshly generated UUID v4
  - `{{cursor}}` — optional caret position marker (rewinds cursor after paste when enabled in **Advanced** settings)
- **Privacy & Ignore Rules**: Automatically skips concealed/transient password-manager pasteboard types, and supports custom **Application**, **Pasteboard type**, and **Regex** ignore rules plus an **Ignore all applications except listed** allowlist mode.
- **System Tray Controls**: Toggle popup, pause/resume capture, arm single-shot ignore-next-copy, clear unpinned history, open Preferences, or check GitHub releases for updates.
- **Single-Instance CLI IPC**: Control the running Clipo daemon from terminal scripts, Raycast/Alfred workflows, or window manager bindings over a local Unix socket / Windows named pipe.

---

## Default Keyboard & Tray Shortcuts

All global and in-popup shortcuts can be customized in **Preferences → General**.

| Action | macOS Default | Windows / Linux Default |
| --- | --- | --- |
| Toggle popup (global) | `⌘⇧C` (`CommandOrControl+Shift+C`) | `Ctrl+Shift+C` (`CommandOrControl+Shift+C`) |
| Paste selected item (or copy if auto-paste is off) | `Enter` | `Enter` |
| Copy selected item only | `⌥Enter` (`Alt+Enter`) | `Alt+Enter` |
| Copy & paste with formatting | `⌘Enter` (`CommandOrControl+Enter`) | `Ctrl+Enter` (`CommandOrControl+Enter`) |
| Copy & paste without formatting (plain text) | `⌘⇧Enter` (`CommandOrControl+Shift+Enter`) | `Ctrl+Shift+Enter` (`CommandOrControl+Shift+Enter`) |
| Quick pick rows 1–9 | `⌘1` .. `⌘9` (`CommandOrControl+1..9`) | `Ctrl+1` .. `Ctrl+9` (`CommandOrControl+1..9`) |
| Pin / Unpin selected item | `⌥P` (`Alt+P`) | `Alt+P` |
| Save selected item as snippet | `⌥S` (`Alt+S`) | `Alt+S` |
| Delete selected item | `⌥⌫` (`Alt+Backspace`) | `Alt+Backspace` |
| Clear all unpinned history | `⌥⇧⌫` (`Alt+Shift+Backspace`) | `Alt+Shift+Backspace` |
| Toggle preview sidecar pane | `⌃Space` (`Control+Space`) | `Ctrl+Space` (`Control+Space`) |
| Open Preferences | `⌘,` (`CommandOrControl+,`) | `Ctrl+,` (`CommandOrControl+,`) |
| Cycle filter tabs (`All` / `Snippets` / `Images` / `Files`) | `Tab` / `⇧Tab` | `Tab` / `Shift+Tab` |
| Close modal / preview pane / popup | `Esc` | `Esc` |
| Tray: Toggle popup | Left-click tray icon | Left-click tray icon |
| Tray: Pause / Resume capture | `⌥` + Left-click (`Alt+Click`) | `Alt` + Left-click |
| Tray: Ignore next copy | `⌥⇧` + Left-click (`Alt+Shift+Click`) | `Alt+Shift` + Left-click |

> **Note:** On Linux, tray modifier-click detection (`Alt+Click` / `Alt+Shift+Click`) is not supported by all desktop trays; use the tray context menu or CLI flags instead.

---

## CLI Flags

When Clipo is already running, invoke the binary with a single control flag to send an IPC command to the active instance:

```text
clipo --pause        Pause clipboard monitoring
clipo --resume       Resume clipboard monitoring
clipo --ignore-next  Ignore the next clipboard change
clipo --toggle       Toggle the Clipo popup window
clipo --help         Print usage information (-h, --help)
```

---

## Platform Notes

### macOS
- **Accessibility Permission**: To automatically paste items into the previously active application (`CGEvent` `Cmd+V`), macOS requires Accessibility permission for Clipo. Open **Preferences → General** and click **Open Accessibility Settings** to grant access in **System Settings → Privacy & Security → Accessibility**. Without Accessibility permission, Clipo falls back to copying selected items to the clipboard so you can paste manually with `⌘V`.

### Linux (Wayland vs. X11)
- **X11**: Uses `x11rb` (XFixes / XTest) for clipboard monitoring, active-window placement, and synthetic `Ctrl+V` auto-paste (`xdotool` is also supported as a fallback).
- **Wayland**: Uses `wl-clipboard-rs` (`ext-data-control` / `wlr-data-control` when exposed by your compositor) for clipboard reads and writes. Because Wayland isolates synthetic keyboard input from background apps, OS-level auto-paste requires `wtype` to be installed; otherwise Clipo copies the selected item cleanly to the clipboard so you can paste manually with `Ctrl+V`.

---

## Quick Start (Build from Source)

### Prerequisites
- **Node.js 20+** and **npm**
- **Rust stable** (`rustup`)
- **Linux only** (Ubuntu/Debian Tauri v2 dependencies):
  ```bash
  sudo apt-get update && sudo apt-get install -y \
    libwebkit2gtk-4.1-dev build-essential curl wget file libxdo-dev \
    libssl-dev libayatana-appindicator3-dev librsvg2-dev patchelf
  ```

### Run & Package

```bash
# Install JavaScript dependencies
npm install

# Run Clipo in development mode (Vite + Tauri)
npm run tauri dev

# Build production installer / app bundle for your OS
npm run tauri build
```

---

## Credits & License

- Built by [Tamzid Peace](https://github.com/tamzidpeace/clipo)
- Licensed under the [MIT License](LICENSE)
