# Contributing to Funke

Thanks for your interest! Please read this before opening anything.

## License

Funke is [MIT-licensed](LICENSE). By submitting a contribution you agree that it is
your own work and that you license it under the same terms.

- **Never vendor external code** or paste snippets carrying another license (including
  copy-pasted SVG icon sets) into the repository — everything here must be
  MIT-compatible and attributable.

If you'd rather build something *for* Funke than change Funke itself, write a
**plugin** — plugins are separate programs under whatever license you choose. See
[docs/PLUGINS.md](docs/PLUGINS.md).

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
