# Contributing to Funke

Thanks for your interest! A few things are unusual about this repository's current
phase — please read this before opening anything.

## ⚠️ License is not decided yet

Funke does not have a license yet (MIT/Apache-2.0 vs GPL is an open decision — see
`docs/PLAN.md` §6). Until one is chosen:

- **Code contributions cannot be merged.** Feel free to open issues and discuss, but
  PRs will wait for the license decision.
- **Never vendor external code** or paste license-bearing snippets (including
  copy-pasted SVG icon sets) into the repository.

If you want to build something *for* Funke today, write a **plugin** instead — plugins
are separate programs under your own license. See [docs/PLUGINS.md](docs/PLUGINS.md).

## Development setup

- Windows 10/11, Rust stable ≥ 1.85 (the Tauri tree uses edition-2024 crates).
- WebView2 (preinstalled on Windows 11).

```bash
cargo run -p funke-app     # build & run; quit via tray or typing "quit"
```

The UI (`crates/funke-app/ui/`) is static HTML/CSS/JS embedded at compile time — there
is deliberately no Node toolchain and no dev server; rebuild after editing it.

## The four gates

CI (and reviewers) require all of these clean before a change is considered done:

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
cargo build -p funke-app
```

After nontrivial `funke-app` changes, also smoke-run the binary: it must print the
tray line and stay alive. There is no headless way to exercise the overlay — test the
UI by hand (`Ctrl+Space`).

## Where things go

The workspace is one crate per concern with strictly downward dependencies
(`funke-app` → provider crates → `funke-core`). Read `.claude/CLAUDE.md` for the
architecture map and — more importantly — the **invariants** (core purity, the
never-recreated overlay window, opaque actions, graceful degradation, the focus
contract, cheap per-keystroke queries). Violating an invariant is a bug even if it
works. `docs/PLAN.md` is the document of record for architectural decisions; update it
when a decision changes or a milestone lands.

New feature providers get their own crate under `crates/`; first-party out-of-process
plugins live under `funke-plugins/`; only providers that act on the launcher itself
belong in `crates/funke-app/src/providers.rs`.

## Style

- rustfmt with `max_width = 120` (checked in `rustfmt.toml`), edition 2021, zero
  clippy warnings.
- The UI follows the design-system rules at the top of `crates/funke-app/ui/style.css`
  (tokens only, one accent, native glass — never CSS-faked).
- Comments explain *why*, not *what*.

## Security

Anything touching the vault provider must keep [SECURITY.md](SECURITY.md) accurate.
Report vulnerabilities privately — see SECURITY.md for the contact.
