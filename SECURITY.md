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
  (at launch, still locked — unlocking is always an explicit act) and talks to its REST
  API. Funke never sees your master key.
- **Secrets don't ride in the UI.** Search results carry item ids only; usernames and
  URI hosts are cached for matching, passwords are fetched by id at the moment you run
  an action and zeroized after use.
- **Prefix-only search.** Vault entries only appear for `v <query>` searches — account
  names never surface while typing an ordinary search, and vault items are excluded
  from the recents list on the empty overlay.
- **Context suggestions are the one exception** (Settings → Commands, on by default).
  With an unlocked vault, summoning Funke over an app offers *that app's* credential in
  the empty overlay — so a name can appear on screen without you typing it. It is only
  ever the credential for the window already in front of you, it is never persisted, and
  turning the setting off restores strict `v`-only behavior.
- **Reading the focused window is read-only and local.** To know which app you came
  from, Funke reads the foreground window's title and its process image name, and — in a
  browser — the URL from the address bar via **UI Automation** (the same public
  accessibility API screen readers use; asking a Chromium browser for it turns on its
  accessibility layer). Nothing is written, injected, or sent anywhere: the URL is used
  in-memory to pick a credential and is dropped on the next summon.
- **Auto-lock.** The vault locks itself after a configurable idle period (Settings →
  default 10 minutes; can be disabled) without vault use, and — opt-in, on by default —
  the moment you lock Windows (cache wiped; `POST /lock`, or with Windows Hello enabled
  the `bw serve` process is killed instead — see below). `bw serve` is also locked and
  killed when Funke exits.
- **Clipboard auto-clear, and exclusion from every clipboard monitor.** Copied secrets
  (passwords, usernames, TOTP codes) are wiped from the clipboard after 30 seconds unless
  you've since copied something else — and they are written with the standard exclusion
  markers (`ExcludeClipboardContentFromMonitorProcessing`, `CanIncludeInClipboardHistory=0`,
  `CanUploadToCloudClipboard=0`), so nothing records them in the first place: not the
  Windows Win+V history, not the cloud clipboard, not a third-party clipboard manager, and
  not Funke's own clipboard history. Auto-clear alone never covered this — whatever copied
  the password into its own store inside the 30 s window kept it there.
- **Clipboard history is memory-only** (`c`, Settings → Commands). What you copy is kept
  in a capped in-process ring and is **never written to disk** — no file survives the
  process, so there is no artifact to steal at rest, and restarting Funke empties it.
  Three filters keep credentials out of it: the exclusion markers above (exact — Funke's
  own vault copies and other password managers' copies never reach it), a shape heuristic
  for the accident nobody marked (API keys, PATs, JWTs, PEM blocks), and the cap. It is
  `prefix_only`, so clips never surface in an ordinary search, and clips are recorded into
  neither the recents file nor frecency.
- **Windows Hello unlock is opt-in** (Settings → Commands). When enabled, a successful
  master-password unlock also mints a `bw` session key and stores it DPAPI-encrypted
  (bound to your Windows account) under `%APPDATA%\funke`; later unlocks show a Windows
  Hello consent prompt and boot `bw serve` pre-unlocked with that key. Locking then
  kills the server process instead of `bw lock`, which would invalidate the stored key.
  Turning the setting off deletes the stored key.
- **Website icons are fetched from your server's icon service** (the Bitwarden cloud
  icon CDN, or your self-hosted server's `/icons` endpoint) and cached in memory only —
  nothing icon-related is written to disk. This tells that service which domains you
  search; disable it in Settings → Commands if that matters to you.
- **No telemetry.** Funke never phones home. It makes exactly four kinds of network
  request, all of them either yours or explicitly asked for: the optional vault favicon
  fetches above; loopback to `bw serve` (which syncs with your configured server); the
  update check and the plugin catalog, both only when you press the button.

## Plugins and the catalog

- **A plugin is a program, not a sandbox.** Plugins are separate processes speaking
  JSON-RPC over stdio; they run with your full user rights, exactly like anything else you
  double-click. Isolation buys crash-safety, not security. Install what you'd run.
- **The catalog pins bytes, not behavior.** Entries in `plugins.json` are added by
  reviewed pull request and pin the archive's **SHA-256**; Funke refuses any download whose
  hash doesn't match, so a release asset cannot be swapped out after review. Archive paths
  are validated before extraction (no `..`, nothing outside the plugin's own folder), and
  the unpacked manifest must declare the id the catalog claimed. That makes an installed
  plugin *the one that was reviewed* — it does not make it safe, and the Plugins pane says
  so before you install.
- Plugin downloads are HTTPS-only, size-capped, and staged: a failed or tampered install
  leaves nothing on disk.

## Known limitations (accepted, documented)

- `bw serve` has no request authentication: any local process running as your user can
  talk to it while it runs. This is inherent to the official CLI's serve mode and is
  the reason the port is random and the server's lifetime is bounded by Funke's.
- Autotype sends keystrokes to whatever window held focus before the overlay was
  summoned. Since 0.5.0 it refuses to type into a window that shows **no password field**
  (UI Automation decides; "Only autotype into login forms", on by default) — which is what
  keeps a credential out of a chat box, where a stray `{ENTER}` would *send* it. The guard
  is a check, not a proof: UI Automation cannot read every window (games, remote sessions,
  terminals), so a refusal is offered back as a confirmable "type it anyway", and a window
  that *does* expose a password field is not thereby trustworthy. Verify the target window
  before confirming — a focus change in the ~150 ms between dismiss and typing cannot be
  fully ruled out. A custom autotype sequence (Settings, or an entry's `autotype` field) is
  typed exactly as written: a sequence that types the password into the wrong field of the
  wrong form is the author's to get right.
- "Open website & autofill" types only once the browser's address bar names the entry's
  site (registrable-domain equality — the same conservative matcher the suggestions use)
  *and* a login form is up. It follows the **default browser**, and it will not follow a
  redirect to a domain the entry does not name: an SSO login hosted elsewhere times out
  unfilled rather than typing a password at a host the entry never claimed.
  When the saved URI is a homepage, Funke will **click the page's own sign-in link** (an
  exact name match — "Log in", "Sign in", "Anmelden" — inside the page, never the browser's
  own chrome, never "Sign in with Google"). It never *guesses* a login URL and never looks
  one up in a search engine: the target of an autofill must not be something SEO can choose.
  The click is a navigation, not a secret — the host and login-form checks still gate the
  typing on whatever page it lands on. To pin the page yourself, give the item a `loginurl`
  custom field; it wins over everything.
- Context matching can be wrong. It is deliberately conservative — a hit needs the URL's
  registrable domain, the process name, or (for a native app) the window title to *name*
  the entry — but a suggestion is only ever an offer: nothing is typed until you press
  Enter on it. **In a browser, only the address bar's host is trusted to identify the
  site**: a page's title is content the site controls, so it may confirm a host that
  already matched but can never produce a match of its own, and a browser window whose
  URL cannot be read suggests nothing. Path, query and fragment are discarded — a site
  cannot steer a suggestion by what it puts after the host.
- The clipboard history's secret filter is exact for *marked* content and guesswork for
  the rest. Anything carrying the exclusion markers is never recorded — that covers Funke's
  own vault copies and every password manager that sets them. Unmarked content is judged by
  shape, which catches API keys, tokens and PEM blocks but cannot recognize a short
  human-chosen password: `Sommer2024!` is a word with a number on the end. Copy a password
  out of a text file and it will sit in the history (in memory, until you clear it or quit).
- The master password crosses the webview IPC boundary as a string once per unlock.
  Rust-side copies are zeroized immediately; the webview side is cleared but JS string
  lifetime is ultimately up to the engine's garbage collector.
- With Windows Hello unlock enabled, the Hello prompt is a **user-presence gate, not
  an extra encryption layer**: the session key is protected by DPAPI, so other code
  running under your Windows account could decrypt it without a Hello prompt. This is
  the classic convenience/security tradeoff of biometric vault unlock — leave the
  setting off if your threat model includes malware running as your user.
- In-browser DOM autofill is **out of scope by design** — use the Bitwarden browser
  extension for that (see docs/DESIGN.md §5).
- Native passkey provision is **out of scope by design**: answering the Windows
  passkey (WebAuthn) prompt would mean registering as a system passkey provider and
  performing FIDO2 signing with key material outside the CLI. Bitwarden's desktop app
  ships exactly that — enable it under Windows Settings → Accounts → Passkeys.
