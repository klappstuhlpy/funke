# Funke — Project Plan

A Spotlight/Raycast-style launcher for Windows. This document records the stack decision,
the architecture, the two hard subsystems, and the roadmap. Written at project start
(2026-07); revise as milestones land.

## 1. Stack decision

**Rust core + Tauri v2 shell, UI in plain HTML/CSS/JS (webview via WebView2).**

- Logic lives in Rust (performance-critical: indexing, fuzzy matching over 100k+ entries,
  low resident RAM, `zeroize`-able secret handling for the vault plugin).
- UI lives in a webview: effortless theming/polish, and it matches existing web-frontend
  skills. No Node toolchain — static assets embedded at compile time; add Vite/TS later
  only if the UI outgrows vanilla JS.
- Tauri v2 official plugins cover the ground layer: global-shortcut, single-instance,
  tray, autostart, updater.

**Fallback:** C# / .NET + WPF — what PowerToys Run/Command Palette and Flow Launcher use;
large reference codebases exist. Switch only if Tauri fights back hard.

**Rejected:** Electron (resident RAM + startup weight), Python (packaging/startup/global
hooks), pure-Rust GUI toolkits (time sink on UI plumbing).

## 2. Architecture

One resident process, tray icon, hotkey summons a pre-created hidden overlay window
(never recreated — that's what makes it instant).

Core abstraction — the provider (in `funke-core`):

```rust
trait SearchProvider {
    fn metadata(&self) -> ProviderMeta;              // id, name, keyword prefix
    fn query(&self, q: &Query) -> Vec<ResultItem>;   // async + cancellation from M1
}
```

The orchestrator debounces keystrokes (~30–50 ms), fans out to enabled providers
concurrently, **cancels in-flight queries when typing continues** (the single most
important thing for perceived speed), merges results, and ranks with **frecency**
(SQLite DB of past selections per query — this is what makes launchers feel telepathic).

**Plugin strategy in two stages:**

1. Ground layer: providers are Rust crates in the workspace behind the trait. No dynamic
   loading while the trait design settles. Deep OS integrations (vault autotype/unlock
   prompt, file index, window switcher) stay compiled-in permanently — they need host
   seams plugins don't get — but are individually deactivatable in settings.
2. Public API (M5): out-of-process plugins speaking **JSON-RPC over stdio**, declared by a
   `plugin.json` manifest. Language-agnostic (Python/Node plugins — that's how launcher
   ecosystems actually grow; Flow Launcher/Wox model), crash-isolated. No Rust `dylib`
   plugins (no stable ABI); WASM rejected because sandboxing defeats most plugin purposes.
   *Protocol v1 landed early* (see `docs/PLUGINS.md` and M5 below): `crates/funke-plugin`
   holds the wire types, a Rust author SDK, and the host; first-party plugins live under
   `funke-plugins/` (starting with the template) and are meant to ship as separately
   compiled artifacts on GitHub releases, installable manually today and via an
   in-settings suggested-plugins catalog later.

## 3. File indexing

- **Phase A (M2):** parallel directory walk of user-chosen roots → in-memory filename
  index; fuzzy matching via `nucleo` (handles hundreds of thousands of entries in
  single-digit ms). Persist to disk; incremental refresh via `notify` watchers.
- **Phase B (M6):** **USN Journal** monitoring for real-time updates + **MFT enumeration**
  for full-volume indexing (the Everything approach). Needs elevation → optional small
  Windows service; the launcher works without it at Phase-A quality.
- **Phase C (optional):** content search by *querying* the existing Windows Search index
  (never build your own content indexer).
- Cheap win: detect an installed Everything instance and offer its IPC as a provider.

## 4. Bitwarden / Vaultwarden plugin (M4)

Talk to the vault via **`bw serve`** (the official CLI's localhost REST API — works
against Vaultwarden, zero crypto to write). The plugin manages the `bw` process, prompts
for unlock in the overlay, and holds the session key in memory only. Evaluate the official
Bitwarden Rust SDK later; **never** reimplement the client protocol/vault crypto by hand.

**Autofill scope — be precise:**

- **In scope: KeePass-style autotype.** Capture foreground HWND before showing the overlay
  → user picks a credential → restore focus → `SendInput` types `username {TAB} password
  {ENTER}` (configurable per entry). For browsers, read the address-bar URL via UI
  Automation to auto-match credentials.
- **Out of scope: in-browser DOM autofill.** That's a browser extension's job; the
  Bitwarden extension already exists. Document this to preempt the #1 feature request.

**Security posture from day one** (public app touching passwords): secrets in `zeroize`d
buffers, never logged; clipboard copies auto-clear (~30 s); vault auto-locks on
idle/lock-screen; no telemetry; `SECURITY.md` with a disclosure contact.

## 5. Roadmap

- **M0 — Skeleton** ✅: workspace, Tauri shell, tray, single-instance, hotkey ↔ overlay,
  focus capture/restore, provider pipeline stub, CI.
- **M1 — App launcher** ✅ (first daily-usable build): apps indexed via PowerShell
  `Get-StartApps` (covers classic *and* UWP apps as AUMIDs, launched through
  `shell:AppsFolder`) plus PATH executables; nucleo fuzzy match; frecency; real app
  icons via `IShellItemImageFactory` (one shell API for exe paths and AUMIDs alike).
  *Start dogfooding here and never stop.* Deviation: frecency persists as a JSON file
  rather than SQLite (revisit if it grows).
  The design foundation also landed here: native glass (acrylic + DWM shadow + Win11
  rounded corners), content-driven window height, Spotlight positioning, and a tokenized
  warm Anthropic-inspired theme (rules in `ui/style.css` and `.claude/CLAUDE.md`).
- **M2 — File search** ✅: Phase-A index of the home directory (walkdir, dot-dir + junk
  denylist, 400k-entry cap) with a byte-subsequence prefilter ahead of nucleo scoring;
  `notify` watcher marks the index dirty and it rebuilds wholesale (≥60 s apart) — full
  per-event surgery deferred to Phase B. Keyword prefix routing landed in the core
  `Registry` (`f query`). Actions: Enter opens, Shift+Enter reveals in Explorer
  (`alt_action` on `ResultItem`); copy-path waits for a proper multi-action UI (M3).
  Icons come from a lazy per-extension cache (per-file for exe/lnk/ico). Known cost:
  the string-per-entry index keeps a six-figure file count around ~200 MB RSS — the
  motivating driver for Phase B's compact (parent-pointer) index.
- **M3 — Utility providers + settings** ◐: **landed** — calculator (`meval`; result tops
  the list, Enter copies via arboard; note: meval pulls the ancient `nom 1.2.4`, swap for
  a maintained expression crate before 1.0), web search (`g` prefix; engine configurable,
  row wears the default browser's icon), system commands (lock/sleep/shutdown/restart/
  empty-bin as `RunCommand`, console-window-free, inline SVG glyph icons; destructive
  entries say "immediately" in the subtitle — a confirm step comes with the multi-action
  UI), the **overview** empty state (recent picks stored as full `ResultItem`s in
  `RecentsStore`, greeting/date/uptime info line, first-run tips), sectioned results
  (grouped by provider display name, ordered by best-ranked item), and the **settings
  window**: a second, on-demand frameless webview (`settings.html`, sidebar navigation
  in the same design system) over a `Settings` struct in core (JSON-persisted, corrupt →
  defaults). Live-applied: summon hotkey (re-registered on save, rejected bindings revert
  and error inline), accent color + overlay width (overlay re-themes via the
  `settings-changed` event), web engine, per-provider enable toggles
  (`Registry::search_enabled`), autostart (`tauri-plugin-autostart`); a Plugins pane
  placeholder points at M5. Reachable via tray → Settings and the "Open Settings" result.
  The **multi-action UI** also landed: `ResultItem` carries a `Vec<NamedAction>`
  (label + action + `confirm` flag; index 0 = Enter, 1 = Shift+Enter), Tab opens an
  actions menu listing them all, and destructive actions (shutdown, restart, empty bin,
  kill) demand a second Enter, rendered in a danger tint. That unlocked **copy-path** on
  files and the **window switcher** (`funke-windows`, `w` prefix): open top-level windows
  fuzzy-matched by title/process, Enter focuses (restoring minimized windows), secondary
  action force-kills the process. **File-index roots** are configurable too (Commands →
  File index folders, native picker via `tauri-plugin-dialog`; empty = home; the index
  thread re-reads roots every 2 s tick, prunes nested roots, and re-indexes + re-watches
  on change). **Pending** — auto-updater only (blocked on update-endpoint + signing
  decisions: needs a public release channel, e.g. GitHub Releases, and a
  `tauri-plugin-updater` signing keypair).
- **M4 — Bitwarden plugin** ◐: **landed** — `funke-vault` crate: `bw serve` spawned
  windowless on a random loopback port (CLI presence probed first; killed on app exit
  via `RunEvent::Exit`), REST client for status/unlock/lock/sync/list/item. Provider
  (`v` prefix, `prefix_only` — entries never appear in global searches or recents)
  fuzzy-matches a cache of non-secret fields (name/username/URI host); secrets are
  fetched by id at action time only. Unlock happens in the overlay via a masked
  password prompt (`PromptVaultUnlock` → `vault-unlock` event → `vault_unlock`
  command; wrong password errors inline). Actions per entry: **autotype** (restore
  captured focus → `SendInput` unicode: username ⇥ password ↵ — hand-written FFI in
  `autotype.rs`), **copy password/username** with 30 s clipboard auto-clear. Idle
  auto-lock after 10 min; Rust-side password/credential buffers zeroized; posture +
  accepted limitations documented in `SECURITY.md`. **Pending** — browser URL matching
  via UI Automation, TOTP copy, per-entry autotype sequences, lock-on-lock-screen,
  vault settings (idle timeout, autotype enter toggle).
- **M5 — Public plugin API + 1.0** ◐: **landed** — protocol v1 (JSON-RPC 2.0,
  line-delimited over stdio: `initialize` handshake with version check, `query`,
  `invoke`, `shutdown`), `crates/funke-plugin` (proto + Rust SDK + host: per-plugin
  worker thread owning the child's stdio, lazy spawn on first query, 300 ms query
  timeout so a slow plugin can't block a keystroke, crash isolation, children killed on
  exit), discovery from `%APPDATA%/funke/plugins/*/plugin.json`, `PluginProvider`
  adapter (actions route back opaquely via `Action::PluginInvoke`; plugin items skip
  recents), settings → Plugins pane (installed list + enable toggles + open-folder),
  authoring guide `docs/PLUGINS.md`, template plugin `funke-plugins/template` (`tp`
  prefix), plus `CONTRIBUTING.md`/`CODE_OF_CONDUCT.md`/`SECURITY.md`. The repo went
  **public** (github.com/klappstuhlpy/funke, MIT), and the **release pipeline**
  landed (`release.yml`: a `v*` tag publishes a GitHub release with the portable
  launcher zip and one `funke-plugin-<id>-<tag>.zip` per `funke-plugins/*` member).
  **Pending** — in-settings suggested-plugins catalog (needs hosted index + trust
  story), hot re-discovery without restart, Python plugin template, MSI/NSIS
  installer, **winget manifest**, **code signing** (unsigned binaries get
  SmartScreen-blocked — budget for a cert or Azure Trusted Signing).
- **M6 — USN/MFT service, content search, ecosystem.**

## 6. Going public

- **License:** decided (2026-07) — **MIT** (`LICENSE`), for maximum plugin-ecosystem
  adoption (Flow Launcher's precedent). GPL was considered and rejected: preventing
  closed forks matters less than a frictionless ecosystem.
- **Prior art to study:** Flow Launcher & PowerToys Command Palette (C#, closest — steal
  UX decisions), ueli (webview-based launcher, proof the approach works), Keypirinha
  (keyboard-first UX), Everything (indexing behavior).
- **Name check early:** GitHub, crates.io, winget, domain. "Funke" is a working name
  (note: Funke Mediengruppe exists).
