# Security

Funke is pre-1.0 software under active development. It touches credentials (the
Bitwarden/Vaultwarden provider), so its security posture is documented from day one.

## Reporting a vulnerability

Please report vulnerabilities privately to **bigbenwashere@gmail.com** — do not open a
public issue. You'll get an acknowledgement within a few days. (Once the repository is
hosted publicly, GitHub private vulnerability reporting will be enabled and preferred.)

## Vault posture (what the Bitwarden provider does and doesn't do)

- **No vault crypto is reimplemented.** All encryption/decryption happens inside the
  official Bitwarden CLI: Funke spawns `bw serve` bound to `127.0.0.1` on a random port
  and talks to its REST API. Funke never sees your master key.
- **Secrets don't ride in the UI.** Search results carry item ids only; usernames and
  URI hosts are cached for matching, passwords are fetched by id at the moment you run
  an action and zeroized after use.
- **Prefix-only search.** Vault entries only appear for `v <query>` searches — account
  names never surface while typing an ordinary search, and vault items are excluded
  from the recents list on the empty overlay.
- **Auto-lock.** The vault locks itself (`POST /lock`, cache wiped) after 10 minutes
  without vault use, and `bw serve` is locked and killed when Funke exits.
- **Clipboard auto-clear.** Copied secrets are wiped from the clipboard after 30
  seconds unless you've since copied something else.
- **No telemetry.** Funke makes no network requests of its own; the only vault traffic
  is loopback to `bw serve` (which syncs with your configured server).

## Known limitations (accepted, documented)

- `bw serve` has no request authentication: any local process running as your user can
  talk to it while it runs. This is inherent to the official CLI's serve mode and is
  the reason the port is random and the server's lifetime is bounded by Funke's.
- Autotype sends keystrokes to whatever window held focus before the overlay was
  summoned. Verify the target window before confirming — a focus change in the ~150 ms
  between dismiss and typing cannot be fully ruled out.
- The master password crosses the webview IPC boundary as a string once per unlock.
  Rust-side copies are zeroized immediately; the webview side is cleared but JS string
  lifetime is ultimately up to the engine's garbage collector.
- In-browser DOM autofill is **out of scope by design** — use the Bitwarden browser
  extension for that (see docs/PLAN.md §4).
