# Changelog

All notable changes to Funke are documented here. The format is based on
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and the project aims to follow
[Semantic Versioning](https://semver.org/spec/v2.0.0.html).

The launcher version is the single source of truth in `crates/funke-app/Cargo.toml`
(`tauri.conf.json` omits it and inherits from there); keep the git tag in step with it.

## [Unreleased]

### Added
- **Screenshots in the README** — a hero shot of the overlay mid-search, a three-up gallery
  (overview, vault search, actions menu) and the four settings pages behind a collapsed
  `<details>`, so the page shows the app without turning into a scroll. Images live in
  `assets/` under descriptive names.

## [0.3.1] - 2026-07-11

### Fixed
- **"Unable to uninstall" when the installer's reinstall page offered to remove an older
  version.** NSIS uses the `publisher` as the installation's registry identity — the install
  directory is recorded under `Software\<publisher>\<product>`, and the reinstall page reads
  it back to tell the *old* uninstaller where it lives (`uninstall.exe _?=<dir>`). 0.3.0
  introduced a `publisher`, which orphaned the key every earlier release had written, so the
  lookup came back empty and the uninstaller was handed a `_?=` with nothing after it. The
  installer now rebuilds that key from the Add/Remove Programs entry (which is named after
  the product, so it survives a publisher change) before any page is shown.
- **The ✕ on a recent didn't remove the row until the overlay was reopened.** `remove_recent`
  returns `()`, which Tauri resolves as `null`, and it was being passed straight into
  `loadOverview` as its options object — `= {}` only defaults away `undefined`, so
  destructuring `null` threw and the re-render never ran. The entry was deleted correctly all
  along; only the repaint was lost.

## [0.3.0] - 2026-07-11

### Added
- **A plugin catalog in Settings** (Plugins → **Browse**). A curated index in the repository
  (`plugins.json`, fetched from the default branch) lists installable plugins; **Install**
  downloads, verifies and loads one live, and **✕** uninstalls it. The trust story is the
  point (`funke-plugin/catalog.rs`): entries get in by pull request, each one **pins the
  archive's SHA-256** so a release asset cannot be swapped out after review, archive paths
  are validated before extraction (no `..`, nothing outside the plugin's own folder), and the
  unpacked `plugin.json` must declare the id the catalog claimed — a failed install leaves
  nothing behind. What it does not do is sandbox a plugin, and the pane says so.
- **Installer: "Start Funke when I sign in".** A checkbox on the installer's welcome page,
  ticked by default on a first install (and off when settings.json already exists, so a
  reinstall can't silently undo a "no"). It leaves a marker file rather than writing the Run
  key itself — funke consumes it on first launch and enables autostart through the plugin, so
  the Settings toggle and the registry can never disagree.
- **Installer: branding and the paperwork.** Sidebar and header images in the app's palette,
  the app icon on the installer/uninstaller, an MIT license page, and real bundle metadata
  (publisher, copyright, homepage, description) — so Add/Remove Programs shows a publisher
  instead of a blank, and the exe carries a copyright string. Uninstalling now also removes
  the autostart entries it may have left behind.
- **winget manifests** (`packaging/winget/`), rendered for each tag by the release workflow
  with the installer's real hash and attached to the release — ready to paste into a
  microsoft/winget-pkgs PR. Submitting stays a deliberate, manual act.
- **Credential suggestions for the app you're in** (`vault_context_suggest`, on by default —
  Settings → Commands). Summon Funke over Discord and the empty overlay offers the Discord
  credential under a “For Discord” heading, ready to autotype straight back into it; over a
  browser tab it offers the credential for *that site*. On every summon a background thread
  reads the previously-focused window's title, its process image name, and — for known
  browsers — the **URL in the address bar via UI Automation** (`funke-shell/uia.rs`), then
  emits `focus-context` so the overlay refreshes in place. It never sits between the hotkey
  and the window. Matching (`funke-vault/context.rs`) is deliberately conservative:
  registrable-domain equality, the process *being* the site, or the window title naming the
  entry — never a fuzzy near-miss. The same scorer now drives the boost on `v` searches,
  replacing the old title-substring heuristic.
- **“Unlock vault to autofill …” in the overview.** A locked vault has no entry cache to
  match against, so it can't know whether a Discord credential exists — it offers the unlock
  instead (Windows Hello when set up), and the credential appears in place once it's open.
- **Per-entry autotype sequences** (`funke-vault/sequence.rs`). A KeePass-style template —
  `{USERNAME}` `{PASSWORD}` `{TOTP}` `{TAB}` `{ENTER}` `{DELAY=500}`, unknown tokens typed
  literally — parsed into steps that *name* the fields, so no secret ever lives inside a
  parsed sequence. Precedence: an entry's `autotype` custom field in Bitwarden → the new
  `vault_autotype_sequence` setting → the built-in username ⇥ password. This unlocks
  password-first, TOTP-in-sequence, and two-page (`{USERNAME}{ENTER}{DELAY=800}{PASSWORD}`)
  logins.

### Changed
- **The overlay's empty state is sectioned.** Credential suggestions come first (“For
  Discord”), recents follow under “Recent”. With nothing to suggest it looks exactly as
  before — a lone header over the only group would be noise.
- **M4 (Bitwarden) and M3 (utilities + settings) are complete** in `docs/PLAN.md`; the two
  pending M4 items — browser URL matching and per-entry autotype sequences — landed here.
- **M5's remaining items are done** apart from the signing certificate itself: the catalog,
  the winget manifests, and the move to `signCommand` all landed. Signing now happens
  *during* bundling, so one switch covers the portable exe, the copy inside the installer,
  and the installer itself — the old post-hoc step could not sign the inner exe. It stays
  dormant until the `AZURE_*` secrets exist.
- **Uninstalling a plugin no longer needs a restart.** `PluginManager::remove` stops the
  child process and `Registry::unregister` drops its provider, so a removed plugin stops
  answering queries at once (installing live already worked).
- **One icon in git, not two.** The settings window's brand mark was a byte-identical copy of
  `icons/icon.png` committed under `ui/`. A webview can't reach outside `frontendDist`, so
  `build.rs` now stages it into `ui/` at build time (only when the bytes differ, so it can't
  churn the mtime and force a rebuild) and the copy is gitignored.

### Fixed
- **Settings → Plugins: the "Suggested plugins" card sat flush against the installed list.**
  The (usually hidden) empty-state placeholder sits between them in the DOM, and a hidden
  element still breaks `+` adjacency — so `.card + .card` never matched and the gap vanished.

## [0.2.0] - 2026-07-11

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

[Unreleased]: https://github.com/klappstuhlpy/funke/compare/v0.3.1...HEAD
[0.3.1]: https://github.com/klappstuhlpy/funke/compare/v0.3.0...v0.3.1
[0.3.0]: https://github.com/klappstuhlpy/funke/compare/v0.2.0...v0.3.0
[0.2.0]: https://github.com/klappstuhlpy/funke/compare/v0.1.1...v0.2.0
[0.1.1]: https://github.com/klappstuhlpy/funke/compare/v0.1.0...v0.1.1
[0.1.0]: https://github.com/klappstuhlpy/funke/releases/tag/v0.1.0
