# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

**Funke** (working name — rename before anything goes public) is a Spotlight/Raycast-style launcher for Windows: a resident tray app whose global hotkey (`Ctrl+Space`) summons a frameless, always-on-top overlay for searching apps, files, and actions. Rust + Tauri v2; the UI is static HTML/CSS/JS embedded at compile time — there is deliberately **no Node toolchain and no dev server**; rebuild after editing `crates/funke-app/ui/`.

## Commands

```bash
cargo run -p funke-app                                # build & run (binary: funke; quit via tray or typing "quit")
cargo test --workspace                                # all tests
cargo test -p funke-core frecency                     # single test / module by name filter
cargo fmt --all
cargo clippy --workspace --all-targets -- -D warnings
```

- Requires Rust stable ≥ 1.85 (the Tauri dependency tree uses edition-2024 crates); Windows 10/11.
- CI (`.github/workflows/ci.yml`, windows-latest) gates on exactly those four checks: fmt `--check`, clippy `-D warnings`, test, build. Keep all four clean before considering a change done.
- There is no way to exercise the overlay headlessly — after nontrivial app-crate changes, smoke-run the binary (it must print the tray line and stay alive) in addition to the test suite.

## Documents of record

- `docs/PLAN.md` — the full project plan: stack decision and rejected alternatives, plugin strategy, file-indexing phases, Bitwarden/Vaultwarden approach, autotype scope (in-browser DOM autofill is explicitly **out of scope**), security posture, and the M0–M6 roadmap. Consult it before architectural decisions; update it when a decision changes or a milestone lands.
- `README.md` — current status and the roadmap table; keep the table in sync with PLAN.md.

## Architecture

Cargo workspace, one crate per concern; dependencies flow strictly downward (`funke-app` → provider crates → `funke-core`):

- **`crates/funke-core`** — the UI-free heart: `SearchProvider` trait, `Query`/`ResultItem`/`Action`/`NamedAction` types (a `ResultItem` carries `actions: Vec<NamedAction>` — label + action + `confirm` flag; index 0 runs on Enter, 1 on Shift+Enter, Tab lists all), `Registry` (fans a query out to all providers, merges best-score-first, caps at `MAX_RESULTS`; a leading provider keyword like `f report` scopes to that provider with the keyword stripped; `search_enabled` applies the settings toggles), `FuzzyMatcher` (nucleo-based; parse the pattern once per keystroke, score once per candidate), `FrecencyStore` (JSON-persisted selection history; timestamps are passed in by callers so tests stay deterministic).
- **`crates/funke-shell`** — Windows shell helpers shared by providers, currently COM icon extraction (`IShellItemImageFactory::GetImage` — one shell API covers exe paths *and* `shell:AppsFolder\<AUMID>`) returning `data:image/png;base64` URLs; COM is initialized lazily per calling thread. Anything COM-flavored belongs here, not in a provider.
- **`crates/funke-apps`** — installed-application provider: indexes Start Menu/Store apps via PowerShell `Get-StartApps` plus PATH executables, on a background thread at startup (queries return nothing until the first index completes; that is by design, not a bug). The index is published twice: names first, then again with icons.
- **`crates/funke-files`** — filename search (`f` prefix): background walk of the settings-chosen roots (`index_roots`, empty = home dir; nested roots pruned; the loop re-reads settings every 2 s and re-indexes + re-watches on change), dot-dir + junk denylist, capped; `notify` watcher flips a dirty flag → wholesale rebuild at most once per minute. Queries run a byte-subsequence prefilter before nucleo scores survivors; file icons come from a lazy per-extension cache (per-file for exe/lnk/ico).
- **`crates/funke-utils`** — small utility providers: calculator (`meval`-based, digits+operators heuristic gate, Enter copies the result), web search (`g` prefix, low-scored fallback row otherwise; engine from settings; the row wears the default browser's icon, resolved once on a background thread), system commands (`RunCommand` entries with inline SVG glyph icons; destructive ones carry `confirm`).
- **`crates/funke-windows`** — window switcher (`w` prefix): `EnumWindows` per keystroke (visible + titled + not tool-window + not cloaked), icons cached per exe; Enter focuses (`FocusWindow`, restores minimized), secondary confirmed action kills the process (`KillProcess`).
- **`crates/funke-vault`** — Bitwarden/Vaultwarden (`v` prefix, **`prefix_only`** — never in global results or recents): spawns `bw serve` on a random loopback port (lazily, on first `v` query; killed on exit via `RunEvent::Exit`) and talks REST to it — all vault crypto stays in the CLI. Caches only non-secret fields (name/username/host) for fuzzy search; secrets fetched by id at action time (`VaultCopy`/`VaultAutotype`) and zeroized. Idle auto-lock (10 min). Posture and accepted limitations live in `SECURITY.md` — keep it in sync with behavior changes here.
- **`crates/funke-plugin`** — the out-of-process plugin system (`proto`/`sdk`/`host` modules; authoring guide: `docs/PLUGINS.md`): plugins are separate executables speaking line-delimited JSON-RPC 2.0 over stdio, discovered once at startup from `%APPDATA%/funke/plugins/*/plugin.json`, spawned lazily on first query. Each gets a worker thread owning its stdio; queries time out at 300 ms (a slow plugin loses its rows, never blocks the keystroke). Actions route back opaquely via `Action::PluginInvoke`; the plugin executes them itself. Manifest strings are deliberately `Box::leak`ed for `ProviderMeta`'s `&'static str`s. First-party plugins live in **`funke-plugins/`** (top-level dir, workspace members; `template/` is the authoring starting point and must stay minimal). Deep integrations (vault, files, windows) stay compiled-in on purpose — they need host seams (focus capture, masked prompts) plugins don't get.
- **`crates/funke-app`** — the Tauri shell: window/tray/hotkey lifecycle, the IPC commands, native FFI (`focus.rs`, `native.rs`), launcher-self providers (`providers.rs`), and the **settings window** (`settings.html/css/js`; frameless, created on demand, destroyed on close — unlike the overlay). New feature providers get their own crate; only providers that act on the launcher itself belong in `providers.rs`.

The IPC surface lives in `main.rs` — overlay: `search(text)`, `run_action(item, action_index)`, `hide_overlay`, `resize_overlay(height)`, `overview()`; settings: `get_settings`, `save_settings` (re-registers the hotkey live; rejected bindings revert and error back), `list_providers`, `list_engines`, `pick_index_root` (native folder dialog), `settings_ready`, `close_settings`; vault: `vault_unlock` (the `PromptVaultUnlock` action emits `vault-unlock` to the overlay, which switches into a masked password prompt and invokes this); plugins: `list_plugins`, `open_plugins_folder` — called from the frontend via `window.__TAURI__` (`withGlobalTauri` is on). User preferences are a `funke_core::Settings` (JSON at `%APPDATA%/funke/settings.json`, corrupt → defaults) shared as `Arc<RwLock<_>>` between `AppState` and providers that read it per query; on save the app emits `settings-changed` and the overlay re-themes (accent token family) without reloading. Frecency boost is applied app-side in `search` (record on `run_action`, boost + re-sort after `Registry::search`); `run_action` also records the full item into `RecentsStore` (skipping `AppControl`/`CopyText`/`FocusWindow` primaries), which feeds the empty-input overview (recents + uptime; greeting/date are computed in JS).

### Invariants (violating these is a bug even if it works)

1. **Core purity:** `funke-core` must never import tauri, webview, or Win32 APIs. Everything in it is unit-testable without a GUI. Provider crates may touch the system but not the UI.
2. **The overlay window is created once** (hidden, at startup) and only ever shown/hidden — never recreated. That is what makes summoning instant. Close requests (Alt+F4) are converted to hide via `prevent_close`.
3. **Actions are opaque to the frontend.** The UI renders `ResultItem`s (action *labels* and the `confirm` flag are data, not semantics) and sends the whole item plus the chosen action index back to `run_action` — it never interprets what an action does. Confirmation for `confirm` actions happens UI-side (armed row, second Enter) before the invoke. New behavior = new `Action` variant in core + a `run_action` arm; the match is exhaustive on purpose, so the compiler forces the new arm.
4. **Graceful degradation over hard failure:** hotkey registration failure logs a warning and the app keeps running (registered via `GlobalShortcutExt` in `setup`, deliberately *not* `Builder::with_shortcuts`, which would abort startup on conflict — e.g. PowerToys Run also uses Ctrl+Space). Same spirit everywhere: a corrupt frecency file loads as empty, a failed app index yields an empty provider.
5. **Focus contract:** the foreground HWND is captured into `AppState.prev_focus` *before* showing the overlay; it is restored when dismissing without an action (Esc) and **not** restored after launching something (the launched app should keep focus). Vault autotype consumes this seam: it *takes* `prev_focus`, refocuses it, then `SendInput`s the credentials. `focus.rs`/`native.rs`/`autotype.rs` are deliberately hand-written FFI — don't pull the `windows` crate into `funke-app` for them (`funke-apps` uses it where COM is unavoidable).
6. **Search must stay cancellation-friendly:** provider `query()` implementations must be cheap per keystroke (index in the background, query against memory). Anything slow blocks the whole result list until the M1+ async orchestrator lands.

## Design system (the UI's styling rules)

The rule header at the top of `crates/funke-app/ui/style.css` is normative; in short:

1. **Tokens only** — every color/radius/spacing comes from the `:root` variables; component rules never hard-code raw values.
2. **Warm, Anthropic-inspired palette** — ivory text ramp over warm charcoal glass with a single terracotta accent (`#d97757`), used sparingly (selection, caret, key hints). The app icon uses the same palette.
3. **Glass is native, never faked in CSS** — acrylic backdrop, DWM shadow, and Win11 rounded corners are applied by the shell (`setup` in `main.rs` + `native.rs`, `"shadow": true` in tauri.conf.json). CSS only tints; no CSS box-shadows or blur on the panel.
4. **The window height follows content** — after every render the UI calls `resize_overlay` with the panel's measured height (clamped in Rust); `#panel` must always be exactly as tall as its content. The overlay is positioned Spotlight-style (~24% down the screen) on every show.
5. Scrollbars are the themed `::-webkit-scrollbar` overlay pills — never the default chrome.

## Conventions

- rustfmt `max_width = 120` (rustfmt.toml), edition 2021, zero clippy warnings.
- Commits are authored solely by the repo owner — no AI co-author trailers.
- License is MIT (`LICENSE`) — never vendor external code or snippets carrying an incompatible license; anything brought in must be MIT-compatible and attributed.
