# Funke — Design & Decisions

What Funke is, how it is built, and why each load-bearing choice was made that way.

This is a **record, not a roadmap**. Everything below either exists in the tree or is written
down as a deliberate decision *not* to build it — and the second kind matters as much as the
first: most of the interesting choices here were choices to leave something out. The document
began (2026-07) as a plan with a milestone list; when the list ran out, it was rewritten into
what it had actually become. §9 keeps the milestone record, condensed, because "when did this
land, and what shipped with it" stays worth being able to answer. §10 is the ground that is
still open.

Update this document when a decision changes — not when code moves.

---

## 1. Stack

**Rust core + Tauri v2 shell; the UI is plain HTML/CSS/JS in a WebView2 webview.**

- Logic lives in Rust: indexing, fuzzy matching over 100k+ entries, low resident RAM, and
  `zeroize`-able buffers for vault secrets.
- The UI lives in a webview, because theming and polish are cheap there and expensive
  everywhere else. **No Node toolchain and no dev server**: the assets are static and embedded
  at compile time, so a UI change is a rebuild, not a pipeline. Vite/TS stays an option if
  vanilla JS ever stops carrying it; it hasn't.
- Tauri v2's official plugins cover the ground layer: global-shortcut, single-instance, tray,
  autostart, updater, dialog.

**Rejected.** Electron (resident RAM and startup weight — the two things a launcher cannot
spend). Python (packaging, startup time, global hooks). Pure-Rust GUI toolkits (a time sink in
UI plumbing, for a UI that has to look *good* rather than merely exist). C#/.NET + WPF was the
standing fallback — it is what PowerToys Run and Flow Launcher use, and the reference
codebases are large — but Tauri never fought back hard enough to take it.

## 2. Architecture

One resident process, a tray icon, and a global hotkey that summons a **pre-created hidden
overlay window**. The window is never recreated, only shown and hidden; that is what makes
summoning feel instant, and it is an invariant rather than an optimization.

A Cargo workspace, one crate per concern, dependencies flowing strictly downward
(`funke-app` → provider crates → `funke-core`). The abstraction everything hangs off is the
provider:

```rust
trait SearchProvider {
    fn metadata(&self) -> ProviderMeta;              // id, display name, keyword prefix
    fn query(&self, q: &Query) -> Vec<ResultItem>;   // cheap, per keystroke
}
```

`Registry` fans a query out to every enabled provider, merges best-score-first, and caps the
list. A leading keyword (`f report`) scopes the query to one provider with the keyword
stripped; a provider marked `prefix_only` (vault, clipboard) answers *only* behind its keyword
and never appears in a global search. `Query::scoped` lets a provider tell "you asked for me"
from "I overheard the query" — snippets match their bodies only in the first case.

**Ranking** is fuzzy score (nucleo; the pattern is parsed once per keystroke, each candidate
scored once) plus **frecency**: a persisted history of what the user actually picked for what
they actually typed. That boost is the difference between a search box and a launcher that
feels like it read your mind.

**Providers must be cheap per keystroke.** Index in the background, query against memory.
There is no async orchestrator yet — one slow provider blocks the list — so this is a rule,
not a preference.

**Core purity:** `funke-core` imports neither Tauri nor Win32. It is unit-testable without a
GUI, and every ranking, parsing and settings decision in it is tested that way.

The crate-by-crate map lives in the crate-level doc comments, which are the truth for anyone
reading the code.

## 3. Two-stage plugin strategy

1. **Ground layer:** providers are Rust crates in the workspace behind the trait. Deep OS
   integrations — vault autotype and its unlock prompt, the file index, the window switcher —
   stay compiled in **permanently**. They need host seams (focus capture, masked input,
   `SendInput`) that a plugin does not get and should not get. They are individually
   deactivatable in settings, which is what "modular" has to mean here.
2. **Public API:** out-of-process plugins speaking **JSON-RPC 2.0, line-delimited over
   stdio**, declared by a `plugin.json` manifest, discovered from
   `%APPDATA%/funke/plugins/*/plugin.json` and spawned lazily on first query. Each gets a
   worker thread owning its stdio; a query times out at 300 ms, so a slow plugin loses its
   rows and never blocks a keystroke. Language-agnostic on purpose — that is how launcher
   ecosystems actually grow (the Flow Launcher/Wox precedent) — with Rust and Python templates
   in `funke-plugins/`.

   **Rejected:** Rust `dylib` plugins (no stable ABI), and WASM — sandboxing defeats the point
   of most plugins, which is to *do* something on the machine.

**The catalog's trust story.** `plugins.json` at the repo root is the curated index, fetched
from raw.githubusercontent by Settings → Plugins → Browse. There is no server to run, its git
history *is* the audit log, and an entry gets in only through a reviewed pull request. Every
entry **pins the archive's SHA-256**: the download is verified before a byte is written, so a
release asset cannot be swapped out from under a reviewed entry. Archive paths are validated
(no `..`, nothing outside the plugin's own folder), the unpacked manifest must declare the id
the catalog claimed, and an install lands via a staging folder so a failure leaves nothing
behind.

None of that is a sandbox, and the pane says so out loud: **a plugin is a process with the
user's full rights.** The hash pin is what makes "reviewed" mean something; it is not a
substitute for trust.

## 4. File search

**What runs today (Phase A):** a background walk of the user's chosen roots (empty = home) into
an in-memory filename index — dot-dir and junk denylist, capped — with a byte-subsequence
prefilter ahead of nucleo scoring. A `notify` watcher marks the index dirty and it rebuilds
wholesale, at most once a minute. Known cost: a string per entry keeps a six-figure file count
around ~200 MB RSS.

**Everything, when it is running.** If voidtools' Everything is up, the query goes to *it*
instead and the walk does not run at all — `funke-files` idles and drops its index, then picks
it up again by itself if Everything goes away. Everything chooses the candidates; Funke still
ranks them, so `f` feels the same either way. This is a **backend of `funke-files`, not a
provider**: there is no second `f` in the results and nothing to configure. It buys the
*result* of a whole-volume index that is current to the second, for the users who already have
one, without the elevated service that building one ourselves would need.

Three things about it are deliberate:

- **Scope stays `index_roots`, not the whole disk.** Everything caps its reply and fills it in
  path order, so a common word ("report" — 4,366 whole-disk matches on this machine) would
  spend the entire candidate budget inside `C:\Windows\WinSxS` and surface none of the user's
  own files. Home by default; `C:\` is one root away, as an opt-in.
- **Substring, not fuzzy.** Everything decides candidates by substring. `rprt` → `report.txt`
  works only on the built-in index. Accepted, and documented in the crate.
- **No DLL.** The `WM_COPYDATA` protocol is spoken directly, so `Everything64.dll` is neither
  vendored nor shipped, and no foreign license enters the tree.

**Not built — Phase B:** USN Journal monitoring plus MFT enumeration (the way Everything does
it). It needs elevation, therefore an optional Windows service. Everything's IPC took the
urgency out of it without cancelling it — that only helps people who already installed
Everything.

**Not built — Phase C:** content search. If it ever happens it will *query the existing Windows
Search index*. Building a content indexer is not a thing this project will do.

## 5. Vault (Bitwarden / Vaultwarden)

Funke talks to the vault through **`bw serve`** — the official CLI's localhost REST API,
spawned on a random loopback port and killed on exit. It works against Vaultwarden, and it
means **zero crypto in this codebase**. The rule behind it: *never reimplement the client
protocol or the vault crypto by hand.* Only non-secret fields (name, username, host, has-TOTP
flag, org label) are cached for fuzzy search; secrets are fetched by id at action time and
zeroized after use.

**Autofill scope, stated precisely — this is the part everyone asks about:**

- **In scope: KeePass-style autotype.** The foreground HWND is captured *before* the overlay
  shows; picking a credential restores that focus and `SendInput`s the sequence. Sequences are
  templates that *name* fields (`{USERNAME}` `{PASSWORD}` `{TOTP}` `{TAB}` `{ENTER}`
  `{DELAY=n}`), parsed into steps — **no secret ever lives inside a parsed sequence**; the app
  resolves them at `SendInput` time from freshly fetched buffers.
- **Out of scope: in-browser DOM autofill.** That is a browser extension's job, and the
  Bitwarden extension already does it. Written down to preempt the #1 feature request.
- **Out of scope: native passkey provision.** Supplying vault passkeys to the Windows WebAuthn
  prompt means registering as a third-party passkey provider and performing the FIDO2 ceremony
  ourselves — i.e. vault crypto outside the CLI, which the rule above forbids. A passkey also
  cannot be autotyped: it is a challenge–response, not text. Bitwarden's own desktop app ships
  that provider.

**Focus context** is what makes the vault feel like it is paying attention. On every summon a
background thread reads what the previously-focused window *is* — title, process image name,
and, for browsers, the **URL from the address bar via UI Automation**. It is off the hotkey
path on purpose: the UIA tree walk costs tens of milliseconds, and nothing may sit between the
keypress and the window.

Matching is **deliberately not fuzzy** — registrable-domain equality, the process *being* the
site, the title naming the entry. A wrong hit here means typing a password into the wrong
window, so a near-miss must lose. (A later security fix tightened it further: in a browser the
address bar's host is the only thing that may *identify* a site. A page's title is written by
the page, so any site could otherwise have titled itself to bait whichever credential it
wanted.)

That context drives the `v`-search boost and **suggestions** — the one sanctioned exception to
`prefix_only`: summon Funke over Discord and the Discord credential is right there, ready to
autotype into it. A locked vault has no cache to match against, so it offers "Unlock vault to
autofill Discord" instead.

**Windows Hello unlock** (opt-in): a master-password unlock also runs `bw unlock --raw` and
persists the session key DPAPI-encrypted; later unlocks are a Hello consent prompt and a
`bw serve` respawned with `BW_SESSION` already set. Locking **kills** the server process
rather than calling `bw lock`, which would invalidate the stored key. The tradeoff (Hello is a
presence gate, DPAPI is user-account encryption) is documented in `SECURITY.md` rather than
hidden.

**Security posture, from day one**, because this is a public app that touches passwords:
secrets in zeroized buffers, never logged; clipboard copies auto-clear after ~30 s **and carry
the clipboard-exclusion markers**, so Win+V, the cloud clipboard and every third-party manager
never record them; auto-lock on idle and (opt-in) on screen lock; no telemetry; a `SECURITY.md`
with a disclosure contact. It is kept in sync with behavior, and it lists the accepted
limitations instead of pretending there are none.

## 6. Clipboard and snippets

**Clipboard history lives in memory and never on disk.** Every other launcher persists it;
here that would be the wrong default, and the decision *is* the feature. The clipboard catches
passwords, tokens and 2FA codes by its nature, so a persisted history would be the single worst
artifact this app could leave at rest — worse than the vault's, which is at least encrypted. It
is a capped in-process ring that dies with the process, and "empty after a restart" is the
accepted cost.

Credentials are kept out in three layers, in descending order of trust: the **clipboard
exclusion markers** (exact — set by Funke's own vault copies and by every other password
manager, honoured on read), a **shape heuristic** for the unmarked accident (API keys, PATs,
JWTs, PEM blocks — guesswork, documented as such), and the cap.

**Snippets** are saved text whose placeholders resolve **at paste time**, never at query time:
`{DATE}` means the day you use it and `{CLIPBOARD}` means what you copied, and resolving either
while the user is still typing the search would bake in the wrong thing. Unknown tokens are
typed literally — a snippet is the user's text, and the expander must never eat part of it on a
guess. They live in `Settings` rather than a store of their own, because they *are* preferences:
they should travel with the rest. Names and abbreviations match globally; **bodies only when
scoped**, so a global query cannot surface a home address because it happened to contain a
street name.

Both paste with **Ctrl+V, not synthesized keystrokes** — the one place autotype's approach is
deliberately not reused. A clip or a snippet is arbitrary text, and typing one that contains
newlines fires them as Enter, sending the half-pasted message.

**Not built: system-wide abbreviation expansion** (type `;sig` in any app and have it expand).
It needs a low-level keyboard hook (`WH_KEYBOARD_LL`) reading every keystroke in every
application — a keylogger's exact shape, in an app that also holds vault secrets. Whether that
trade is worth making is a decision for the maintainer, not a detail of the snippets feature.

## 7. The interface

The overlay is **native glass**: acrylic backdrop, DWM shadow, Win11 rounded corners, all
applied by the shell. CSS only tints — it never fakes any of it. The window height follows its
content (the UI measures the panel and tells Rust), and it is positioned Spotlight-style about
a quarter down the screen on every show.

The palette is warm: an ivory text ramp over warm charcoal with a single terracotta accent,
used sparingly (selection, caret, key hints). Every color, radius and spacing is a token; no
component rule hard-codes a value. Shortcuts are drawn as **keycaps**, one per key — Shift+Enter
is two keys, not a glyph.

**Actions are opaque to the frontend.** The UI renders items and sends the item plus a chosen
action index back; it never interprets what an action *does*. Action labels and the `confirm`
flag are data. New behavior is a new `Action` variant in core plus one arm in `run_action` —
the match is exhaustive on purpose, so the compiler asks for the arm.

**Nothing the user reads is a literal.** Every visible string comes from a catalogue —
`funke_core::i18n` for what providers produce, `ui/i18n.js` for what the UI writes itself —
with English and German halves kept at parity by a test. Two rules protect the launcher from
its own translations: **ids are never localized** (they key frecency and recents, which outlive
a language change), and **the English word keeps matching** (a German UI still answers to
`settings`, because the matcher scores both and keeps the better — muscle memory is not a
language).

**Graceful degradation over hard failure**, everywhere: a hotkey that fails to register logs a
warning and the app keeps running (PowerToys Run also wants Ctrl+Space); a corrupt frecency
file loads as empty; a failed app index yields an empty provider.

## 8. Distribution

- **License: MIT**, decided 2026-07, for maximum plugin-ecosystem adoption (Flow Launcher's
  precedent). GPL was considered and rejected: preventing closed forks matters less here than a
  frictionless ecosystem. Nothing under an incompatible license may be vendored into the tree.
- **Releases:** a `v*` tag builds the NSIS installer, the portable zip, rendered winget
  manifests, and one zip per first-party plugin that changed since the previous tag. The version
  has a single source of truth (`crates/funke-app/Cargo.toml`; `tauri.conf.json` omits it and
  inherits).
- **Auto-update** is live: `tauri-plugin-updater` against GitHub releases, signed with a keypair
  whose public half sits in `tauri.conf.json` and whose private half is a repo secret, backed up
  outside the repo (GitHub secrets are write-only — that backup is the only recoverable copy).
- **Code signing** is wired but dormant: it happens *during* bundling via
  `bundle.windows.signCommand`, injected by the workflow only when the `AZURE_*` secrets exist,
  so one switch covers the portable exe, the copy inside the installer, and the installer
  itself. **Pending an actual certificate** (Azure Trusted Signing). Until then binaries ship
  unsigned and SmartScreen warns.
- **Name.** "Funke" is a working name — Funke Mediengruppe exists. GitHub, crates.io, winget and
  a domain all want checking before this is called finished.

**Prior art worth studying:** Flow Launcher and PowerToys Command Palette (C#, closest — steal
their UX decisions), ueli (a webview launcher, proof the approach works), Keypirinha
(keyboard-first UX), Everything (indexing behavior).

## 9. What's built

The milestone record, condensed.

|        |                         |                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                 |
|--------|-------------------------|-----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------|
| **M0** | Skeleton                | Workspace, Tauri shell, tray, single-instance, hotkey ↔ overlay, focus capture/restore, provider pipeline, CI.                                                                                                                                                                                                                                                                                                                                                                                                  |
| **M1** | App launcher            | Start Menu and Store apps (PowerShell `Get-StartApps`, so classic *and* UWP via `shell:AppsFolder`) plus PATH executables; nucleo matching; frecency; real icons via `IShellItemImageFactory`. The design foundation landed here too: native glass, content-driven height, the tokenized warm theme. *Dogfooding started here and never stopped.* Deviation from plan: frecency persists as JSON, not SQLite.                                                                                                   |
| **M2** | File search             | The Phase-A index, prefilter ahead of nucleo, `notify` rebuilds, and keyword routing (`f`) in the core registry.                                                                                                                                                                                                                                                                                                                                                                                                |
| **M3** | Utilities + settings    | Calculator (`fasteval` — swapped from `meval`, which dragged in an unmaintained `nom 1.2.4`), web search, system commands, the empty-state overview (recents, greeting, tips), sectioned results, and the settings window. Live-applied: hotkey, accent, width, engine, provider toggles, autostart. The **multi-action UI** (Enter / Shift+Enter / Tab menu / Ctrl+n, `confirm` on destructive ones) landed here and unlocked copy-path and the **window switcher** (`w`). Auto-updater configured and signed. |
| **M4** | Vault                   | `funke-vault`: `bw serve`, the `v` provider, autotype, copy password/username/TOTP with auto-clear, Windows Hello unlock, website favicons, idle and screen-lock auto-lock, **focus context** and its suggestions, per-entry autotype sequences.                                                                                                                                                                                                                                                                |
| **M5** | Public plugin API + 1.0 | Protocol v1, `funke-plugin` (proto + SDK + host), discovery, the Plugins pane, `docs/PLUGINS.md`, Rust and Python templates. The repo went **public** (MIT), the release pipeline landed, and plugin lifecycle went live in both directions: Refresh adds, Browse/Install fetches from the checksum-pinned catalog, ✕ uninstalls — none of it needing a relaunch.                                                                                                                                               |
| **M7** | Clipboard history       | `c`, in memory only, exclusion markers plus the shape heuristic, Ctrl+V paste. It needed one core change: a bare keyword and a space (`c `) now scopes a provider with an *empty* query, so there is something to browse.                                                                                                                                                                                                                                                                                       |
| **M8** | Snippets                | `s`, stored in `Settings`, placeholders resolved at paste time, bodies searched only when scoped (`Query::scoped`).                                                                                                                                                                                                                                                                                                                                                                                             |
|        | Localization            | English and German, across both windows and everything the providers produce. The locale follows Windows unless set, and a change repaints without a restart.                                                                                                                                                                                                                                                                                                                                                   |

## 10. Open ground

Nothing here is scheduled. It is what is knowingly missing, with the reason it is missing.

- **Phase B file indexing** (USN Journal + MFT, elevated service). Wanted for the ~200 MB RSS
  and the minute-wide staleness window of the built-in index. Deprioritized, not cancelled:
  Everything's IPC delivers the same result for the people who have it.
- **Async, cancellable search orchestration.** Today a slow provider blocks the whole list; the
  mitigation is the "providers must be cheap" rule, which holds only as long as every provider
  obeys it.
- **Content search**, by querying the Windows Search index — never by building an indexer.
- **A certificate**, so releases stop being greeted by SmartScreen.
- **The name.**
- Decided against, and unlikely to change: in-browser DOM autofill (§5), native passkey
  provision (§5), system-wide abbreviation expansion (§6), plugin sandboxing (§3), and a
  persisted clipboard history (§6).
