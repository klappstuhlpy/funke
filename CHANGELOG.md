# Changelog

All notable changes to Funke are documented here. The format is based on
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and the project aims to follow
[Semantic Versioning](https://semver.org/spec/v2.0.0.html).

The launcher version is the single source of truth in `crates/funke-app/Cargo.toml`
(`tauri.conf.json` omits it and inherits from there); keep the git tag in step with it.

## [Unreleased]

### Added
- **NSIS installer & uninstaller.** Releases now ship `funke-<tag>-windows-x86_64-setup.exe`
  (registered in Add/Remove Programs) alongside the portable zip. The release workflow drives
  the Tauri bundler (`cargo tauri build`) instead of only zipping the raw binary.
- **Live version in Settings.** The Settings window reads the app version at runtime via
  `getVersion()` instead of a hard-coded string, so a Cargo bump is reflected everywhere.
- **Dormant code-signing hook.** The release workflow has a gated Azure Trusted Signing step
  that is a no-op until the `AZURE_*` repo secrets are set (binaries still ship unsigned for now).
- **Configurable vault auto-lock.** New settings for the idle-lock timeout
  (`vault_idle_lock_minutes`, `0` = never), an opt-in **lock-on-screen-lock**
  (`vault_lock_on_screen_lock`, on by default — the watchdog locks the vault when Windows
  locks), and a toggle for autotype's trailing Enter (`vault_autotype_enter`).
- **Hot plugin re-discovery.** Settings → Plugins → **Refresh** (`reload_plugins`) loads
  newly installed plugins live via `PluginManager::reload` + a runtime `RwLock<Registry>`,
  no restart needed (additive — removing a plugin still needs a relaunch).
- **Python plugin template** (`funke-plugins/template-python`, `tpy` prefix): the same demo
  in dependency-free Python behind a `run.cmd` launcher. The release workflow now packages
  script plugins (whose entry isn't built by cargo) by shipping their folder as-is.
- **Auto-updater.** `tauri-plugin-updater` wired with a "Check for updates" button
  (Settings → General) and a `check_update` command, checking GitHub Releases. The signing
  keypair is configured (public key in `tauri.conf.json`, private key + password in repo
  secrets); the release workflow emits the signed updater artifact + `latest.json`, so
  updates go live for installed clients from the first signed release onward.

### Changed
- **Single source of truth for the version.** `tauri.conf.json` no longer pins `version`; it
  is inferred from `crates/funke-app/Cargo.toml`, fixing the drift where the config said `0.1.0`
  while the crate was `0.1.1`.
- **Plugins only re-release when they change.** The release workflow diffs each
  `funke-plugins/*` directory against the previous tag and skips unchanged plugins (e.g.
  `template`), so tagging a launcher-only release no longer re-publishes untouched plugin zips.
- **Calculator no longer depends on the unmaintained `nom 1.2.4`.** Swapped `meval` for the
  dependency-free `fasteval` (same f64 semantics), clearing the future-incompatibility warning.

## [0.1.1] - 2026-07-10

### Added
- **Windows Hello vault unlock** (opt-in `vault_hello`): master-password unlock also persists a
  DPAPI-encrypted `bw` session key, and `VaultHelloUnlock` re-unlocks via `UserConsentVerifier`
  (parented to the overlay) without retyping the master password. Toggling the setting off
  deletes the stored session.
- **Website favicons in vault results** (`vault_icons`), fetched from the server's icon service
  with an in-memory per-host cache, wiped on lock and re-rendered in place via a listener.
- **Brand image in Settings** replacing the previous text/spark treatment.

### Changed
- `focus.rs` gained `force_foreground` (AttachThreadInput dance) to reclaim the overlay's
  foreground after the Windows Hello system dialog closes.
- When a Hello session is persisted, locking **kills** the `bw serve` process instead of
  `POST /lock` (which would invalidate the session key).
- `SECURITY.md` and `docs/PLAN.md` updated to match the new unlock/favicon behavior.

## [0.1.0] - 2026-07-10

Initial public release (github.com/klappstuhlpy/funke, MIT) — the launcher through the M5
plugin foundation.

### Added
- **Resident launcher overlay:** frameless, always-on-top, native-glass panel summoned by a
  global hotkey (`Ctrl+Space`), created once and shown/hidden for instant summoning; tray icon
  and lifecycle.
- **Search core** (`funke-core`): `SearchProvider` trait, `Registry` (keyword-scoped, best-score
  merge, capped), nucleo `FuzzyMatcher`, JSON-persisted `FrecencyStore` and `RecentsStore`, and a
  `Settings` struct.
- **Providers:** installed apps (`funke-apps`), filename search (`funke-files`, `f`), calculator +
  web search (`g`) + system commands (`funke-utils`), window switcher (`funke-windows`, `w`), and
  Bitwarden/Vaultwarden (`funke-vault`, `v`) talking REST to a spawned `bw serve`.
- **Out-of-process plugin system** (`funke-plugin`): line-delimited JSON-RPC 2.0 over stdio,
  discovered from `%APPDATA%/funke/plugins/*/plugin.json`, lazy-spawned with a 300 ms query
  timeout and crash isolation; `template` first-party plugin (`tp`) and authoring guide.
- **Settings window:** frameless pane UI over the `Settings` struct — hotkey rebinding, provider
  toggles, index roots, search engine, accent theming, and the Plugins pane.
- **Release pipeline** (`release.yml`): a `v*` tag publishes a GitHub release with the portable
  launcher zip and one zip per `funke-plugins/*` plugin.
- Repo went public with `LICENSE` (MIT), `README.md`, `SECURITY.md`, `CONTRIBUTING.md`, and
  `CODE_OF_CONDUCT.md`.

[Unreleased]: https://github.com/klappstuhlpy/funke/compare/v0.1.1...HEAD
[0.1.1]: https://github.com/klappstuhlpy/funke/compare/v0.1.0...v0.1.1
[0.1.0]: https://github.com/klappstuhlpy/funke/releases/tag/v0.1.0
