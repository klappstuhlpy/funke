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
- **Out of scope: native passkey provision.** Supplying vault passkeys to the Windows
  passkey/WebAuthn prompt requires registering as a third-party passkey provider
  (Windows 11 plugin-authenticator API) and performing the FIDO2 ceremony ourselves —
  i.e. vault crypto outside the CLI, which the line above forbids. A passkey also
  can't be autotyped (it's a challenge–response, not text). Bitwarden's desktop app
  ships that provider; point users to Windows Settings → Accounts → Passkeys.

**Fast unlock (Windows Hello), opt-in:** a master-password unlock additionally runs
`bw unlock --raw` and persists the session key DPAPI-encrypted; later unlocks show a
Hello consent prompt and respawn `bw serve` with `BW_SESSION` set (pre-unlocked).
Locking kills the server process instead of `bw lock` so the stored key survives.
Tradeoff (Hello = presence gate, DPAPI = user-account encryption) documented in
SECURITY.md.

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
- **M3 — Utility providers + settings** ✅: calculator (`fasteval`, a
  dependency-free f64 evaluator — swapped from `meval`, which dragged in the unmaintained
  `nom 1.2.4`; result tops the list, Enter copies via arboard), web search (`g` prefix; engine configurable,
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
  on change). **Auto-updater** landed and **configured** (`tauri-plugin-updater`): a
  "Check for updates" button (Settings → General) calls the `check_update` command, which
  downloads + stages a newer GitHub release. The signing keypair is set up — the **public**
  key is in `tauri.conf.json` → `plugins.updater.pubkey`, and the **private** key +
  password are repo secrets (`TAURI_SIGNING_PRIVATE_KEY`, `TAURI_SIGNING_PRIVATE_KEY_PASSWORD`).
  The private key + password are backed up **outside the repo** at `~/funke-updater/`
  (`.key`, `.key.pub`, `password.txt`) — GitHub secrets are write-only, so that folder is the
  only recoverable copy; back it up (a password manager) and never commit it. Updates go live
  for installed clients from the **first signed release onward** (the release workflow emits the
  signed updater artifact + `latest.json` now that the key secret is present).
- **M4 — Bitwarden plugin** ✅: `funke-vault` crate: `bw serve` spawned
  windowless on a random loopback port (CLI presence probed first; killed on app exit
  via `RunEvent::Exit`), REST client for status/unlock/lock/sync/list/item. Provider
  (`v` prefix, `prefix_only` — entries never appear in global searches or recents)
  fuzzy-matches a cache of non-secret fields (name/username/URI host); secrets are
  fetched by id at action time only. Unlock happens in the overlay via a masked
  password prompt (`PromptVaultUnlock` → `vault-unlock` event → `vault_unlock`
  command; wrong password errors inline). Actions per entry: **autotype** (restore
  captured focus → `SendInput` unicode: username ⇥ password ↵ — hand-written FFI in
  `autotype.rs`), **copy password/username/TOTP** (TOTP computed by the CLI at action
  time, cached only as a has-it flag) with 30 s clipboard auto-clear. **Windows Hello
  unlock** (opt-in): DPAPI-persisted `bw` session key redeemed behind a Hello consent
  prompt, `bw serve` respawned pre-unlocked; kill-based lock keeps the key valid.
  **Website favicons** from the server's icon service (in-memory cache, toggleable);
  entries carry their organization label; usernames only match queries containing `@`.
  **Auto-lock** is configurable: idle timeout (`vault_idle_lock_minutes`, `0` = never)
  plus opt-in **lock-on-screen-lock** (`vault_lock_on_screen_lock`; the watchdog polls the
  input-desktop name — `lockscreen.rs` raw FFI — every 30 s and locks when Windows is
  locked). Rust-side password/credential buffers zeroized; posture + accepted limitations
  documented in `SECURITY.md` (incl. passkeys out of scope — see §4).

  **Focus context** (`context.rs`) closes the milestone. On every summon a background
  thread reads what the previously-focused window *is* — title, process image name
  (`focus.rs` raw FFI), and, for known browsers, the **URL from the address bar via UI
  Automation** (`funke-shell/uia.rs`: the window's Document element's ValuePattern, the
  address-bar Edit as fallback) — then emits `focus-context` so the overlay refreshes in
  place. It is off the hotkey path on purpose: the UIA tree walk costs tens of
  milliseconds and nothing may sit between the keypress and the window. Matching is
  deliberately conservative (registrable-domain equality, the process *being* the site,
  the title naming the entry — never a fuzzy near-miss), and it drives two things: the
  score boost on `v` searches, and — the point of it — **context suggestions in the empty
  overlay** (`vault_context_suggest`, on by default): summon Funke over Discord and the
  Discord credential is right there under a "For Discord" heading, ready to autotype into
  it; over a GitHub tab it's the GitHub one. A **locked** vault has no cache to match
  against, so it offers "Unlock vault to autofill Discord" instead — unlock and the
  credential appears in place. This is the one sanctioned exception to `prefix_only`
  (documented in SECURITY.md): only ever the credential for the window already in front
  of you, never persisted, and switchable off.

  **Per-entry autotype sequences** (`sequence.rs`) replace the hardcoded username ⇥
  password: a KeePass-style template (`{USERNAME}` `{PASSWORD}` `{TOTP}` `{TAB}` `{ENTER}`
  `{DELAY=500}`, unknown tokens typed literally) parsed into `Step`s that *name* the
  fields — no secret ever lives inside a parsed sequence; the app resolves them at
  `SendInput` time from freshly fetched, zeroized credentials. Precedence: the entry's
  own `autotype` custom field in Bitwarden → `vault_autotype_sequence` in settings → the
  built-in sequence (whose trailing Enter stays governed by `vault_autotype_enter`; an
  explicit template is typed exactly as written).
- **M5 — Public plugin API + 1.0** ✅: protocol v1 (JSON-RPC 2.0,
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
  landed (`release.yml`: a `v*` tag publishes a GitHub release with the **NSIS
  installer/uninstaller** and the portable launcher zip, plus one
  `funke-plugin-<id>-<tag>.zip` per `funke-plugins/*` member that **changed since the
  previous tag** — unchanged plugins like `template` no longer re-release). Version is a
  single source of truth: `tauri.conf.json` omits it, so it is inferred from
  `crates/funke-app/Cargo.toml` and shown live in the settings window. A **Python plugin
  template** (`funke-plugins/template-python`, `tpy` prefix) shows the protocol in
  dependency-free Python behind a `run.cmd` launcher; the release workflow packages script
  plugins (entry not built by cargo) by shipping their folder as-is.

  **Plugin lifecycle, live** (`funke-plugin/catalog.rs`): **Settings → Plugins → Refresh**
  (`reload_plugins`) picks up a dropped-in plugin via `PluginManager::reload` + a runtime
  `RwLock<Registry>`; **Browse** fetches the curated catalog and **Install** downloads it;
  **✕** uninstalls it (`PluginManager::remove` stops the child, `Registry::unregister` drops
  its provider, then the folder goes) — all three without a relaunch.

  **The catalog's trust story** is the reason it took until now. The index is `plugins.json`
  on the default branch — no server to run, its git history *is* the audit log, and an entry
  gets in only by a reviewed pull request. Each entry **pins the archive's SHA-256**, so a
  release asset cannot be swapped out from under a reviewed entry; the launcher refuses a
  mismatch before writing a byte. Archive paths are validated (no `..`, nothing outside the
  plugin's own folder), and the unpacked manifest must declare the id the catalog claimed.
  None of that sandboxes a plugin — it is a process with the user's full rights, which the
  pane says out loud. Sandboxing was rejected in §2 and that hasn't changed.

  **Distribution:** **winget manifests** (`packaging/winget/`) are rendered per tag by the
  release workflow with the installer's real hash and attached to the release; submitting
  them to microsoft/winget-pkgs stays a deliberate manual PR. **Code signing** is wired but
  dormant: it now happens *during* bundling via `bundle.windows.signCommand` (injected by the
  workflow only when the `AZURE_*` secrets exist), so one switch covers the portable exe, the
  copy inside the installer, and the installer itself — the old post-hoc step could not sign
  the inner exe. **Pending — an actual certificate** (Azure Trusted Signing account; budget
  for it). Until then binaries ship unsigned and SmartScreen warns.
- **M7 — clipboard history** ✅ (`c`, landed 2026-07). **Decision: in memory, never on disk.**
  A persisted history is what every other launcher ships, and it is the wrong default here:
  the clipboard catches passwords, tokens and 2FA codes by nature, so persisting it would
  create the single worst artifact this app could leave at rest — worse than the vault's,
  which at least stays encrypted. The history therefore lives in a capped in-process ring
  and dies with the process; "empty after a restart" is the accepted cost.

  Credentials are kept out in three layers, in descending order of trust: the **clipboard
  exclusion markers** (exact — set by Funke's own vault copies and by every other password
  manager, honoured on read), a **shape heuristic** for the unmarked accident (API keys,
  PATs, JWTs, PEM blocks), and the cap. Setting those markers on vault copies fixed a
  pre-existing hole in its own right: a password copied from Funke used to be recorded by
  Win+V and the cloud clipboard, which the 30 s auto-clear never touched.

  Enter pastes via **Ctrl+V, not synthesized keystrokes** — a clip is arbitrary text, and
  typing one containing newlines fires them as Enter, submitting a half-pasted message.
  This is the one place autotype's approach is deliberately *not* reused.

  Reaching the browse view needed a core change: a bare keyword and a space (`c `) now
  scopes to a provider with an *empty* query. Providers with nothing to browse answer it
  with nothing, exactly as before.
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
