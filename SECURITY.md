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
- **The `bw` binary is pinned and its signature checked.** Your master password is handed
  to that executable in its environment, so *which* executable it is matters: Funke
  resolves `bw` once at startup — known install locations (winget, Program Files, scoop,
  npm) **before** `PATH`, so a planted `bw.exe` early in `PATH` cannot win — and then
  spawns that absolute path for the rest of the session rather than re-walking `PATH` on
  every spawn. It also asks Windows who signed it (Authenticode, *and* whether the
  certificate says Bitwarden — a signature by somebody else is not good enough).
  A CLI that doesn't verify is **used anyway, with the reason shown on the vault's unlock
  row**: an `npm -g install @bitwarden/cli` is an unsigned `.cmd` wrapper around a Node
  script and cannot ever be verified, and a launcher that bricked that install would only
  teach people to switch the check off. Settings → Vault → *Only run a Bitwarden-signed
  CLI* turns the warning into a refusal.
- **Secrets don't ride in the UI.** Search results carry item ids only; usernames and
  URI hosts are cached for matching, passwords are fetched by id at the moment you run
  an action and zeroized after use.
- **Prefix-only search.** Vault entries only appear for `v <query>` searches — account
  names never surface while typing an ordinary search, and vault items are excluded
  from the recents list on the empty overlay.
- **Context suggestions are the one exception** (Settings → Vault, on by default).
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
  the moment you walk away: locking Windows, the machine going to sleep or hibernating,
  or a remote-desktop session disconnecting (all delivered as session events, with a
  polling fallback; cache wiped; `POST /lock`, or with Windows Hello enabled the
  `bw serve` process is killed instead — see below). `bw serve` is also locked and
  killed when Funke exits.
- **Vault content is hidden from screen capture** (Settings → Vault, on by default).
  While the overlay shows the masked master-password prompt, vault search results, or a
  context suggestion, the window is excluded from screenshots, recordings, and screen
  shares (`WDA_EXCLUDEFROMCAPTURE`; on Windows builds too old for it, the window captures
  as a black box instead). Plain results stay capturable on purpose — a launcher you
  cannot screenshot reads as a bug — and the shield drops the moment the vault content
  leaves the screen.
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
- **Windows Hello unlock is opt-in** (Settings → Vault), and the Hello prompt is the
  lock itself. When enabled, a successful master-password unlock mints a `bw` session key
  and seals it under a key that **only a Hello prompt can reproduce**: `KeyCredentialManager`
  holds a key pair whose private half lives in the TPM and signs only after Hello verifies
  you, and the session file is `AES-256-GCM` under `HKDF-SHA256(that signature)`, DPAPI-
  wrapped on top. Later unlocks show a Hello prompt — the signature *is* the prompt — and
  boot `bw serve` pre-unlocked. No Hello, no signature; no signature, no key.
  The two layers answer different attackers: DPAPI binds the file to your Windows account
  (useless on another machine), the Hello layer binds it to your presence (useless to code
  running *as* you). Locking kills the server process instead of `bw lock`, which would
  invalidate the stored key. Turning the setting off deletes both the stored session and
  the Hello key.
  Sessions saved by Funke **before 0.6.0** used the older scheme (DPAPI only, with Hello as
  a mere consent check) and are **discarded, not migrated** — the first Hello unlock after
  updating asks for your master password once, and that unlock re-seals the session properly.
- **Website icons are fetched from your server's icon service** (the Bitwarden cloud
  icon CDN, or your self-hosted server's `/icons` endpoint) and cached in memory only —
  nothing icon-related is written to disk. This tells that service which domains you
  search; disable it in Settings → Vault if that matters to you.
- **No telemetry.** Funke never phones home: nothing you type, search, or open is sent
  anywhere, and there is no account, no analytics, and no identifier. It makes exactly four
  kinds of network request: the optional vault favicon fetches above; loopback to
  `bw serve` (which syncs with your configured server); the plugin catalog, only when you
  press Browse; and the update check.
- **The update check is the one request Funke makes without being asked** — so it is a
  setting (General → *Tell me about new versions*, on by default), and this is what it
  does. Shortly after startup it fetches the release manifest from GitHub and compares
  version numbers. If a newer release exists you get one Windows notification, once: the
  version it announced is written to `%APPDATA%\funke\update.seen`, so the same release
  never notifies twice however often Funke restarts. It **never installs anything** —
  downloading and installing is a separate button you press in Settings, after you have
  seen the version and its release notes. The request carries nothing but an HTTP GET; it
  tells GitHub only what any download would. Turn the setting off and Funke checks only
  when you press *Check for updates*.

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
  the reason the port is random and the server's lifetime is bounded by Funke's — and
  that bound is enforced by the kernel, not just by a clean exit: every serve process is
  assigned to a kill-on-close job object, so even a crashed or force-killed Funke cannot
  leave an unlocked server listening. (On the rare system where the job cannot be
  created, Funke logs a warning and falls back to the exit-time kill.)
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
- Zeroizing bounds a secret's lifetime, not its location. Two additional mitigations
  narrow the location story: Funke excludes itself from Windows Error Reporting at
  startup (a crash dump would carry whatever secret was in flight; an admin-configured
  LocalDumps policy or an attached debugger can still dump), and the decrypted Windows
  Hello session key is held in page-locked (`VirtualLock`) memory that cannot be swapped
  to the pagefile while it waits for `bw serve` to boot. Secrets that merely pass
  through (a fetched password between REST parse and keystrokes) are zeroized but not
  page-locked — the parse makes transient copies no wrapper can catch, and pretending
  otherwise would be theater.
- Windows Hello unlock still **widens the attack surface, it just no longer hands the
  session key over for free**. Since 0.6.0 the key is sealed to a TPM signature Hello
  gates (above), so code running as you cannot simply decrypt it — but it *can still ask*.
  Nothing stops a program running under your account from raising a Hello prompt of its
  own and hoping you approve it out of habit, and once the vault is unlocked the session
  lives in Funke's memory, where it is page-locked and zeroized but not hidden from a
  debugger attached as you. The prompt is a wall an attacker must get you to open, not one
  they can walk around; if your threat model includes malware running as your user, the
  strongest posture is still to leave the setting off and type your master password.
- In-browser DOM autofill is **out of scope by design** — use the Bitwarden browser
  extension for that (see docs/DESIGN.md §5).
- Native passkey provision is **out of scope by design**: answering the Windows
  passkey (WebAuthn) prompt would mean registering as a system passkey provider and
  performing FIDO2 signing with key material outside the CLI. Bitwarden's desktop app
  ships exactly that — enable it under Windows Settings → Accounts → Passkeys.
