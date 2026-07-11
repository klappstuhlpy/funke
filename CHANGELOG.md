# Changelog

All notable changes to Funke are documented here. The format is based on
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and the project aims to follow
[Semantic Versioning](https://semver.org/spec/v2.0.0.html).

The launcher version is the single source of truth in `crates/funke-app/Cargo.toml`
(`tauri.conf.json` omits it and inherits from there); keep the git tag in step with it.

## [Unreleased]

## [0.4.0] - 2026-07-11

### Security
- **A web page's title could conjure a credential suggestion for a different site.** The
  focus-context scorer let the window title carry a match on its own, worth exactly the
  threshold a suggestion needs. In a browser that title is the *page's* text — the site
  writes it, path text and all — so visiting `github.com/discord/discord-api-docs` floated
  the **Discord** credential with Enter wired to autotype it into a **github.com** tab, and
  any site could have titled itself to bait whichever password it wanted. In a browser the
  address bar's host is now the only thing that may identify the site: the title can
  confirm a host that already matched but never produce a match, a browser whose URL can't
  be read suggests nothing, and it no longer offers to unlock "for Chrome". Native apps are
  unchanged — their title comes from the app itself, not from a page.
- **Copied vault secrets were recorded by Windows' own clipboard history.** The 30 s
  auto-clear only ever wiped the *clipboard*; anything that recorded the password within
  that window — Win+V, the cloud clipboard, any third-party clipboard manager — kept its
  copy afterwards. Secrets are now written with the clipboard-exclusion markers
  (`ExcludeClipboardContentFromMonitorProcessing`, `CanIncludeInClipboardHistory=0`,
  `CanUploadToCloudClipboard=0`), which every clipboard monitor honours, so they are
  invisible to all of them — Funke's own new history included.

### Fixed
- **A successful vault unlock reported itself as a `ReferenceError`.** `exitVaultPrompt`
  still referenced a constant the string catalogue had removed (`SEARCH_PLACEHOLDER`), so it
  threw halfway through — and at the call site that matters, inside the unlock's own `try`,
  that throw was caught by the `catch` meant for a *wrong password*: the vault had in fact
  unlocked, and the overlay answered by re-rendering the masked prompt with
  "ReferenceError: SEARCH_PLACEHOLDER is not defined" where the error message goes. Escaping
  out of the prompt hit the same throw and left the input wearing the password placeholder,
  with the query you had typed gone. The query is restored and re-run on both paths again.
- **Two strings the overlay writes itself stayed English in a German UI** — the vault
  prompt's "Enter unlocks the vault" line and the ✕ tooltip on a recent. Both are in the
  catalogue now, which is where invariant 0 says they belong.
- **"Open settings" from the overlay hung the launcher's windows.** Picking it built the
  settings window from the command handler — which runs on the main thread, and the main
  thread *is* the event loop. `WebviewWindowBuilder::build()` creates the window there and
  then waits for the webview, so it was waiting on the loop it had just blocked: the HWND
  appeared, the call never returned, and the window stayed invisible forever. Worse, the
  wedged creation took every later window operation with it, so the tray's Settings item
  stopped responding too and only a restart brought it back — the reason it looked like the
  tray worked "until you touched the overlay". The window is now built off the main thread
  (the seam Windows Hello unlock already uses), leaving the loop free to finish the job.
  Present since the first commit; the tray's item only ever worked because nothing had hung
  the loop yet.
- **A crash while the settings pane booted left a window that never appeared.** It is
  created hidden and reveals itself once the UI has painted, so anything thrown on the way
  there stranded it invisible — the same "nothing happens" symptom with a different cause.
  It now reveals itself either way and says what went wrong in the error bar.
- **The clipboard recorder could silently drop a clip.** Reading returned a bare
  `Option<String>`, which conflated "somebody's excluded secret", "not text", and **"another
  process had the clipboard open"** — so losing the race for a lock that *every* clipboard
  monitor grabs the instant a copy happens meant the clip was dropped and the history got a
  hole in it, for no reason. The read now says which of the three it was, the recorder waits
  and comes back for a busy clipboard instead of giving up on it, and the retry budget is
  long enough to sit out ordinary contention. (Found because the round-trip test started
  failing the moment it ran with a Funke instance up — its listener is exactly such a
  competitor.)

### Added
- **An About pane in settings** — what Funke is, which version is running, and one click to
  everything around it: the source, the issue tracker, releases, the changelog, the design
  record, the plugin guide, the security policy, the license. Links open in your browser, not
  inside the settings window (a new `open_url` command, which refuses anything that isn't
  `https://` — a command is callable by anything in the webview, and the shell would happily
  launch a local executable).
- **The Hotkey pane lists the keys that work *inside* the overlay** too — navigate, open, run
  the second action, list all actions, run the nth, dismiss. "What do I press" now has one
  answer in one place, instead of being folded into a footer legend you only see while the
  overlay is open.

- **German, and a seam for the next language.** Everything Funke writes — result titles and
  subtitles, action labels, section headers, the tray menu, both windows — comes from a
  string catalogue with an English and a German half (`funke_core::i18n` for what providers
  produce, `ui/i18n.js` for what the UI writes itself). Settings → General → *Sprache* picks
  one; the default follows Windows, and a change repaints both windows at once — no restart,
  no re-index.

  Two rules keep localization from quietly breaking the launcher, and both are tested:
  - **A result's id is never translated.** Ids key frecency and recents, which outlive a
    language change — build one out of a title and switching to German silently orphans
    everything you have ever launched. Ids come from stable keys (`system:lock`); only the
    text is looked up.
  - **The English word keeps working.** A German UI still answers to `settings`, because the
    matcher scores the localized title *and* the English one and keeps the better. Muscle
    memory is not a language.

  Untranslated keys render as the key itself rather than as a blank, so a hole in the
  catalogue is visible the first time it renders instead of being silently swallowed.
- **Everything integration** — if voidtools' [Everything](https://www.voidtools.com/) is
  running, file search asks *it* instead of walking the disk: no index to build at startup,
  none held in memory, and no minute-long wait before a file you just saved can be found.
  Detected, never required — close Everything and the built-in index takes over again, with
  no setting to find and nothing to configure. Settings → Commands says which one is
  answering.

  It changes **how** files are indexed, not **which** files are searched: the query is scoped
  to the same index folders as before (your home folder by default). Searching every drive is
  deliberately not the default — Everything caps a reply and fills it in its own order, so on
  a whole-disk query a common word like "report" (4,366 matches here) spends the entire
  budget on `C:\Windows\WinSxS` before reaching anything of yours. Add `C:\` as a folder if
  you want it anyway.

  One difference is worth knowing: Everything matches **substrings**, where the built-in
  index matches fuzzy subsequences — `rprt` finds `report.txt` in the built-in index and
  nothing in Everything. Ranking stays ours either way.

  Spoken over Everything's `WM_COPYDATA` IPC directly, so there is no `Everything64.dll` to
  vendor and no third-party license in the tree.
- **Snippets** (`s`) — text you paste often (a signature, an address, a block of
  boilerplate), created in Settings → Snippets and pasted into the window you came from.
  Found by name or abbreviation from an ordinary search; the *body* is only searched behind
  the `s` prefix, so a global query can't surface your address because you typed a street
  name. Placeholders resolve at paste time, not save time: `{DATE}` `{TIME}` `{DATETIME}`
  (with your own format, `{DATE:%d.%m.%Y}`), `{CLIPBOARD}` for what you last copied,
  `{CURSOR}` for where the caret should land, `{NEWLINE}` `{TAB}` — and, as in vault
  autotype sequences, an unknown token is typed exactly as written, so
  `fn main() { … }` survives intact. Snippets live in `settings.json`, so they need no
  store of their own and travel with the rest of your preferences.
- **Providers can tell a keyword-scoped query from a global one** (`Query::scoped`) — the
  seam that lets snippets be forthcoming when asked for and discreet when merely overhearing.
- **Clipboard history** (`c`) — an in-memory ring of the last 100 things you copied. `c `
  browses it newest-first, `c foo` fuzzy-matches the text. Enter pastes the clip straight
  back into the window you came from (Ctrl+V, not keystrokes — typing a multi-line clip
  would fire its newlines as Enter and send the half-pasted message), Shift+Enter copies it,
  Ctrl+3 forgets it, and a confirmed row at the bottom clears the lot.

  **Nothing is ever written to disk** — a file of everything you ever copied is the worst
  artifact this app could leave behind, so the history lives in the process and dies with
  it. Three filters stand in front of it: the clipboard-exclusion markers (exact — Funke's
  own vault copies and other password managers' copies never arrive at all), a shape
  heuristic for the unmarked accident (API keys, PATs, JWTs, PEM blocks), and the cap.
  Clips are `prefix_only` like the vault, and they enter neither `recents.json` (which
  would put their text on disk) nor frecency (whose ids outlive the clips they name).
- **A bare prefix and a space is a provider's browse view.** `c ` hands the clipboard an
  empty query, which is how it lists everything. Previously a keyword needed text after it
  to scope at all; providers with nothing to browse answer an empty query the way they
  always did, with nothing.
- **Screenshots in the README** — a hero shot of the overlay mid-search, a three-up gallery
  (overview, vault search, actions menu) and the four settings pages behind a collapsed
  `<details>`, so the page shows the app without turning into a scroll. Images live in
  `assets/` under descriptive names.

### Changed
- **Shortcuts are drawn as keys, not as strings.** `⇧↵` was one box with two glyphs crammed
  into it, which reads as a symbol rather than as two fingers. Shift+Enter is now two caps
  side by side, the way the keyboard has it — in the result rows, in the actions menu, in the
  footer legend, in the new shortcut list, and on the hotkey recorder, which now builds the
  combination out of caps as you hold the modifiers down. One shared component
  (`ui/keys.css` + `ui/keys.js`), and one spelling of a chord (`"Ctrl+Shift+Enter"` — the same
  string the settings file and the shortcut registration already use), so a key is drawn one
  way, in one place.
- **The settings panes are grouped into categories.** Commands was a flat wall of switches,
  seven of which had to begin with the word "Vault:" to say what they were even about. That
  prefix is now the heading above the card — *Providers*, *Web search*, *File search*,
  *Vault · unlocking*, *Vault · autotype*, *Vault · suggestions* — so no row has to repeat its
  own subject, and every other pane is grouped the same way.
- **The settings window is a fixed size.** It is frameless (there was no grip to drag anyway),
  its panes are laid out for one width, and nothing in it rewards being made bigger — the
  content column scrolls instead.
- **`docs/PLAN.md` is now `docs/DESIGN.md`**: a record of what is built and *why* — including
  what was deliberately not built and the reason it wasn't — rather than a roadmap whose
  milestones have all landed. The milestone list survives as one condensed section of it, and
  the open ground is stated as open ground rather than as a schedule.

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

[Unreleased]: https://github.com/klappstuhlpy/funke/compare/v0.4.0...HEAD
[0.4.0]: https://github.com/klappstuhlpy/funke/compare/v0.3.1...v0.4.0
[0.3.1]: https://github.com/klappstuhlpy/funke/compare/v0.3.0...v0.3.1
[0.3.0]: https://github.com/klappstuhlpy/funke/compare/v0.2.0...v0.3.0
[0.2.0]: https://github.com/klappstuhlpy/funke/compare/v0.1.1...v0.2.0
[0.1.1]: https://github.com/klappstuhlpy/funke/compare/v0.1.0...v0.1.1
[0.1.0]: https://github.com/klappstuhlpy/funke/releases/tag/v0.1.0
