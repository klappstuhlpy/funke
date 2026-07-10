<div align="center">
  <img src="crates/funke-app/icons/icon.png" width="110" alt="Funke logo">

  # Funke

  **A fast, extensible Spotlight-style launcher for Windows.**

  Summon a search bar with a global hotkey, then launch apps, find files, switch windows,
  run actions, and search your Bitwarden / Vaultwarden vault with autotype — all from the keyboard.

  <br>

  [![CI](https://img.shields.io/github/actions/workflow/status/klappstuhlpy/funke/ci.yml?branch=main&style=flat-square&label=CI)](https://github.com/klappstuhlpy/funke/actions/workflows/ci.yml)
  ![Rust](https://img.shields.io/badge/Rust-1.85%2B-d97757?style=flat-square&logo=rust&logoColor=white)
  ![Tauri](https://img.shields.io/badge/Tauri-v2-262624?style=flat-square&logo=tauri&logoColor=ffc131)
  ![Platform](https://img.shields.io/badge/Platform-Windows%2010%2F11-262624?style=flat-square)
  ![Plugins](https://img.shields.io/badge/Plugins-JSON--RPC%20over%20stdio-d97757?style=flat-square)
  ![Status](https://img.shields.io/badge/Status-pre--release-8a8478?style=flat-square)
  [![License](https://img.shields.io/badge/License-MIT-d97757?style=flat-square)](LICENSE)

</div>

## Features

`Ctrl+Space` toggles a native-glass overlay (acrylic backdrop, DWM shadow, Win11 rounded
corners, sized to its content) in a warm Anthropic-inspired theme. Type to search — results
arrive in labeled sections, frequently picked ones bubble up (frecency).

- **Applications** — fuzzy-search installed apps (Start Menu, Store/UWP, PATH) with real icons.
- **Files** (`f`) — background filename index of your chosen folders, watcher-refreshed.
- **Windows** (`w`) — switch to any open window (Enter focuses, restores minimized) or end its process.
- **Vault** (`v`) — Bitwarden/Vaultwarden via the official `bw` CLI: unlock in the overlay,
  autotype into the previous window, copy with 30 s clipboard auto-clear, idle auto-lock.
  Prefix-only for privacy — see [SECURITY.md](SECURITY.md).
- **Web search** (`g`) — configurable engine, wearing your default browser's icon.
- **Calculator** — `2+2*3` inline; Enter copies the result.
- **System commands** — lock, sleep, shut down, restart, empty recycle bin; destructive ones ask to confirm.
- **Plugins** — separate executables in any language speaking JSON-RPC over stdio: drop a
  folder into `%APPDATA%\funke\plugins`, toggle in Settings → Plugins. Write your own with
  [docs/PLUGINS.md](docs/PLUGINS.md), starting from `funke-plugins/template`.
- **Actions menu** — Enter runs the default action, **Tab lists every action** of a result
  (open / reveal in Explorer / copy path, …).
- **Overview** — the empty overlay shows recent picks, a greeting/date/uptime line, and first-run tips.
- **Settings window** (tray → Settings, or search "settings") — summon hotkey, accent color,
  overlay width, web engine, provider toggles, file-index folders, plugins, launch-at-startup —
  all applied live.

**Status:** M3 (minus auto-update) + core M4 + the M5 plugin foundation — the full
roadmap lives in [docs/PLAN.md](docs/PLAN.md).

## Development

Requires Rust ≥ 1.85 and Windows 10/11 (WebView2 is preinstalled on Windows 11).

```bash
cargo run -p funke-app        # build & run (first build takes a few minutes)
cargo test --workspace        # unit tests
cargo fmt --all && cargo clippy --workspace --all-targets -- -D warnings
```

Note: the UI in `crates/funke-app/ui/` is embedded at compile time (no Node toolchain,
no dev server) — rebuild after editing it.

Contributions: read [CONTRIBUTING.md](CONTRIBUTING.md) first — or write a
[plugin](docs/PLUGINS.md), which lives in its own repository under any license you like.

## Layout

```text
crates/
├── funke-core/      # UI-free core: SearchProvider trait, Registry (+ prefix scoping), fuzzy, frecency
├── funke-shell/     # Windows shell helpers shared by providers (COM icon extraction)
├── funke-apps/      # installed-apps provider: Get-StartApps (AUMIDs) + PATH executables
├── funke-files/     # filename index of chosen roots: walkdir + notify refresh, `f` prefix
├── funke-utils/     # utility providers: calculator, web search (`g`, engine from settings), system commands
├── funke-windows/   # window switcher (`w`): switch to or kill open top-level windows
├── funke-vault/     # Bitwarden/Vaultwarden (`v`): bw serve client, autotype, prefix-only privacy
├── funke-plugin/    # plugin protocol (JSON-RPC/stdio): author SDK + launcher-side host
└── funke-app/       # Tauri shell: tray, hotkey, overlay + settings windows, IPC commands, built-in providers
    └── ui/          # static frontend (HTML/CSS/JS, embedded via frontendDist)
funke-plugins/       # first-party out-of-process plugins (template/ is the authoring starting point)
docs/PLAN.md         # full project plan: architecture, indexing, Bitwarden, roadmap
docs/PLUGINS.md      # how to write a plugin (manifest, protocol, distribution)
```

## License

[MIT](LICENSE).
