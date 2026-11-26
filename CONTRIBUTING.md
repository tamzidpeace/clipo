# Contributing to Clipo

Thank you for contributing to **Clipo**! This guide covers local toolchain setup, project layout, verification commands, and release signing.

---

## Toolchain Prerequisites

- **Node.js 20+** and **npm**
- **Rust stable** via [`rustup`](https://rustup.rs/) with `clippy` and `rustfmt`:
  ```bash
  rustup component add clippy rustfmt
  ```
- **Platform system dependencies**:
  - **macOS**: Xcode Command Line Tools (`xcode-select --install`).
  - **Windows**: Microsoft C++ Build Tools (MSVC) and WebView2 Runtime.
  - **Linux (Debian/Ubuntu)**:
    ```bash
    sudo apt-get update
    sudo apt-get install -y \
      libwebkit2gtk-4.1-dev \
      build-essential \
      curl \
      wget \
      file \
      libxdo-dev \
      libssl-dev \
      libayatana-appindicator3-dev \
      librsvg2-dev \
      patchelf
    ```

---

## Project Architecture Map

```text
clipo/
├── popup.html / settings.html       # Multi-window Vite entry HTML pages
├── src/
│   ├── app.css                      # Tailwind CSS v4 theme tokens & scrollbars
│   ├── lib/
│   │   ├── highlight.ts             # Match highlighting, special symbols, hex swatch & clipoimg URL helpers
│   │   ├── hotkey.ts                # Accelerator normalization, formatting & KeyboardEvent matching
│   │   ├── ipc.ts                   # Typed Tauri invoke & event wrappers
│   │   ├── syntax.ts                # Lightweight code tokenizer & JSON pretty-printer
│   │   └── types.ts                 # TypeScript DTOs & defaults mirrored from Rust models
│   ├── popup/                       # Main clipboard popup window (Svelte 5 Runes)
│   │   ├── App.svelte               # Popup state, keyboard shortcuts, preview & modal orchestration
│   │   ├── SearchHeader.svelte      # Search input, search-mode badge, paused indicator
│   │   ├── FilterTabs.svelte        # All / Snippets / Images / Files filter strip
│   │   ├── ItemList.svelte          # Virtualized/scrollable result list
│   │   ├── ItemRow.svelte           # Row rendering, quick-pick badge, color swatch, pin indicator
│   │   ├── PreviewPane.svelte       # Attached sidecar preview (syntax, JSON, image, files, color)
│   │   ├── QuickSnippetModal.svelte # Quick-save history item or create blank snippet
│   │   └── FooterBar.svelte         # Status pill, Preview, New Snippet, Clear, Preferences
│   └── settings/                    # Preferences window (Svelte 5 Runes)
│       ├── App.svelte               # Tabbed navigation, live settings sync & unsaved-guard
│       ├── GeneralTab.svelte        # Launch at login, updater, shortcuts, search/paste, permissions
│       ├── StorageTab.svelte        # Retention toggles, max items, sort order, live DB/blob stats
│       ├── AppearanceTab.svelte     # Placement, preview delay, highlighting, footer, theme
│       ├── IgnoreTab.svelte         # Allowlist toggle + App / PasteboardType / Regex rules
│       ├── SnippetsTab.svelte       # Snippet CRUD, variable preview, JSON import/export
│       ├── AdvancedTab.svelte       # Capture pause/ignore-next, poll cadence, quit cleanup, CLI docs
│       └── HotkeyRecorder.svelte    # Interactive shortcut capture control
├── tests/frontend/                  # Vitest unit tests for frontend utilities & DTO parity
└── src-tauri/
    ├── tauri.conf.json              # Tauri v2 windows, CSP, asset protocol, bundle icons & updater
    ├── capabilities/default.json    # Desktop plugin permissions (autostart, global-shortcut, updater)
    ├── icons/                       # Vector source (icon.svg) + generated PNG / ICNS / ICO bundle
    ├── tests/                       # Rust integration tests (pipeline, IPC, search, snippets, storage, windows)
    └── src/
        ├── main.rs                  # Binary entry point & CLI flag dispatch (--pause/--resume/--ignore-next/--toggle)
        ├── lib.rs                   # AppState, Tauri setup, global shortcut, watcher & exit cleanup
        ├── commands.rs              # Tauri IPC commands exposed to popup & settings UIs
        ├── models.rs                # Shared domain structs, enums & default settings
        ├── tray.rs                  # System tray icon, menu & Alt/Alt+Shift click actions
        ├── windows_mgr.rs           # Popup positioning, sidecar resize, blur debounce & clipoimg:// resolver
        ├── clipboard/               # OS clipboard backends (macOS, Windows, Linux X11/Wayland) + ignore/privacy
        ├── paste/                   # Clipboard write-back, self-write suppression & OS paste simulation
        ├── search/                  # Fuzzy (nucleo-matcher), exact, regex & mixed search engine
        ├── snippets/                # Template expansion ({{date}}, {{uuid}}, {{cursor}}, ...) & JSON import/export
        ├── ipc/                     # Single-instance Unix domain socket / Windows named pipe server & client
        └── storage/                 # SQLite schema/migrations, binary blob store & JSON settings persistence
```

---

## Frontend Conventions & Testing

- **Svelte 5 Runes**: All Svelte components use Svelte 5 runes (`$state`, `$derived`, `$effect`, `$props`, `{@render ...}`). Keep components typed with strict TypeScript (`"strict": true`).
- **DTO Parity**: Changes to `src-tauri/src/models.rs` or `src-tauri/src/commands.rs` must stay aligned with `src/lib/types.ts` and `tests/frontend/types.test.ts`.
- **Testing**:
  - Frontend helpers and DTO contracts are tested with **Vitest** under `tests/frontend/`.
  - Backend modules have unit tests alongside source files and integration tests in `src-tauri/tests/`.
  - Note: `tauri::generate_context!()` validates that `dist/` exists at compile time, so run `npm run build` before running `cargo test` or `cargo clippy` on a clean checkout.

---

## Verification Suite

Run the full check sequence before opening a pull request (matching `.github/workflows/ci.yml`):

```bash
npm ci
./node_modules/.bin/tsc --noEmit
npx vitest run
npm run build
cargo test --manifest-path src-tauri/Cargo.toml
cargo clippy --manifest-path src-tauri/Cargo.toml --all-targets -- -D warnings
```

Check Rust formatting as well:

```bash
cargo fmt --manifest-path src-tauri/Cargo.toml --check
```

---

## Releasing & Tauri Updater Signing Setup

Releases are automated by `.github/workflows/release.yml` when pushing a version tag (`v*`):

1. Bump the version in `package.json`, `src-tauri/tauri.conf.json`, and `src-tauri/Cargo.toml`.
2. Commit the version bump and push a tag:
   ```bash
   git tag v0.1.0
   git push origin v0.1.0
   ```
3. The workflow builds draft release artifacts for macOS (`aarch64-apple-darwin` and `x86_64-apple-darwin`), Windows, and Linux.

### Enabling Signed Auto-Updates (`latest.json`)

By default, `.github/workflows/release.yml` sets `includeUpdaterJson: false` and `src-tauri/tauri.conf.json` leaves `plugins.updater.pubkey` empty so unsigned tag builds succeed out of the box (falling back to GitHub Releases links in **Preferences → General**).

To enable signed in-app updates:

1. Generate a Tauri updater keypair locally:
   ```bash
   npx tauri signer generate -w ~/.tauri/clipo.key
   ```
2. Add the **private key** contents and optional passphrase to your repository's GitHub Actions secrets:
   - `TAURI_SIGNING_PRIVATE_KEY`
   - `TAURI_SIGNING_PRIVATE_KEY_PASSWORD`
3. Update `src-tauri/tauri.conf.json` with your generated **public key** and release manifest endpoint:
   ```json
   "plugins": {
     "updater": {
       "pubkey": "<YOUR_GENERATED_PUBLIC_KEY>",
       "endpoints": [
         "https://github.com/tamzidpeace/clipo/releases/latest/download/latest.json"
       ]
     }
   }
   ```
4. Change `includeUpdaterJson: false` to `includeUpdaterJson: true` in `.github/workflows/release.yml`.
