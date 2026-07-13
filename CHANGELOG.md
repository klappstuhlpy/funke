# Changelog

All notable changes to Funke are documented here. The format is based on
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and the project aims to follow
[Semantic Versioning](https://semver.org/spec/v2.0.0.html).

The launcher version is the single source of truth in `crates/funke-app/Cargo.toml`
(`tauri.conf.json` omits it and inherits from there); keep the git tag in step with it.

## [0.6.0] - 2026-07-13

The vault's Windows Hello prompt stops being a formality, Funke learns which `bw` it is
talking to, search stops being as slow as its slowest source, and updates ask before they
install. Also carries the hardening that was staged for 0.5.1 and never tagged.

### Changed
- **"Check for updates" no longer installs the update.** It used to do both in one click:
  press check, and the new version was downloaded and staged — you never saw which version,
  and there was nothing to say no to. Now the check tells you what is available and what
  changed in it, and installing is a second button you press once you've read that. An update
  is a new program on your machine; that is a decision, not a side effect of asking a
  question.
- **No terminal window at sign-in.** Funke is now a windowed binary in every build, not only
  release ones, so nothing can flash a console at startup — and the tray line that used to
  appear in it is gone from view. Run Funke from a shell and its output still lands in *that*
  shell, redirects and pipes included; started any other way, it opens no window and writes
  nowhere.
- **The vault has its own settings page.** Its eight switches used to live at the bottom of
  Commands, which made that page half launcher and half password manager and buried the ones
  that matter. They now sit under Vault, in four groups that each say what the group is for:
  unlocking, autotype, suggestions, and privacy & trust. Descriptions throughout the settings
  read as sentences rather than labels, and the German ones were rewritten where they still
  read like translations.
- **Secrets stay out of crash dumps and (where they rest) out of the pagefile.** Funke
  excludes itself from Windows Error Reporting at startup — a WER dump of a crashed
  launcher would have carried whatever secret was in flight — and the decrypted Windows
  Hello session key now lives in page-locked memory (`VirtualLock`) from DPAPI decrypt to
  `bw serve` boot, so it cannot be swapped to disk while it waits. Both are hardening
  layers with documented limits (SECURITY.md), not vaults.
- **Translation Update.** Updated and fixed some translations for German and English in `i18n.js` for better 
  readability and consistency.

### Added
- **The Windows Hello prompt is now the lock, not a doorbell beside it.** Unlocking with
  Hello used to work like this: Funke showed a Hello dialog, Windows said "yes, that was
  them", and Funke then decrypted the stored vault session — which was protected by DPAPI,
  meaning any program running as you could decrypt it too, without ever raising a prompt.
  The dialog was a formality; the lock it appeared to guard wasn't there. It is now. The
  session key is sealed with a key derived from a signature that only your TPM can produce
  and only after Hello verifies you, so there is no path to the vault that skips the prompt
  — not for Funke, not for anything else running under your account. DPAPI stays as a second
  wrapper, because the two answer different attackers: one binds the file to your Windows
  account, the other to your presence.
  **Sessions saved before this release are discarded rather than upgraded** — they are the
  weak shape being retired, and reading one to convert it would mean keeping the old path
  alive to do exactly what we stopped trusting it to do. Your first Hello unlock after
  updating asks for your master password once, and that unlock seals the new session
  properly. If your device has no Hello at all, nothing weaker is stored in its place: you
  simply keep using your master password, as you did before enabling it.
- **A slow source can no longer freeze your typing.** Until now every provider answered in
  turn, so the result list was only ever as fast as the slowest one — which is why "answer
  from memory" had to be a rule rather than a preference, and why a search of file *contents*
  was unbuildable. The registry now fans a query out across all its sources at once and waits
  120 ms; whatever has answered by then is what you see, and anything slower arrives a moment
  later and drops into place, re-ranked, without disturbing the row you had selected. Type on
  and the results of the query you abandoned are discarded rather than fighting the ones you
  want. Nothing about the launcher feels different today — every current source already
  answers in single-digit milliseconds. It is what makes the ones that can't possible.
- **Funke now knows which `bw` it is talking to.** Every vault command used to be spawned
  as plain `bw`, which asks Windows to walk `PATH` again on each spawn — and one of those
  commands hands over your master password. Anything that dropped a `bw.exe` into a
  directory earlier in `PATH` than the real one would have received it. Funke now resolves
  the CLI once at startup, checking the real install locations *before* `PATH`, and spawns
  that exact file from then on. It also checks Bitwarden's Authenticode signature — not
  merely that *somebody* signed it, which anyone with a certificate can arrange. An
  unverified CLI still runs, and says so on the vault's unlock row, because an npm install
  is an unsigned script wrapper and that is a perfectly good way to install it;
  `Vault → Only run a Bitwarden-signed CLI` turns the warning into a refusal.
- **Screen capture can't see vault content anymore.** While the overlay shows the masked
  master-password prompt, vault rows, or a context suggestion, the window is excluded
  from screenshots, recordings, and screen shares (`SetWindowDisplayAffinity` with
  `WDA_EXCLUDEFROMCAPTURE`; older Windows falls back to capturing a black box). The
  shield is scoped, not blanket: plain results stay capturable, so demos and bug-report
  screenshots keep working, and the exclusion drops as soon as the vault content leaves
  the screen. `Vault → Hide vault content from screen capture`, on by default.
- **The vault now locks when the machine sleeps or a remote session disconnects** — not
  only on screen lock. All three are the same event ("the user walked away"), but only
  the lock case was covered, and even that by a 30-second poll. Session and power events
  are now delivered as window messages the moment they happen; suspend is caught on both
  edges (a fast sleep can swallow the suspend message, so the wake always locks too).
  The poll stays as fallback, and the existing "Lock when you step away" setting governs
  all of it.
- **`bw serve` can no longer outlive a crashed Funke.** Until now the vault server was
  killed on exit — a *graceful* path. If Funke crashed or was force-killed, an unlocked
  `bw serve`, whose loopback REST API has no request authentication, kept listening
  indefinitely. Every serve process is now assigned to a kill-on-close Windows job
  object, so the kernel terminates it the moment Funke's process ends, however it ends.
  Where the job can't be created the old behavior remains, with a logged warning.
- **Funke tells you when there's a new version — once.** Shortly after startup it checks
  GitHub for a newer release and, the first time it sees one, raises a Windows notification.
  Only the first time: the announced version is remembered, so the same release never knocks
  again however often you restart. A newer one will. It only ever tells you — nothing is
  downloaded or installed by a notification. Turn it off under General → *Tell me about new
  versions*; the check is then only ever the button.

## [0.5.0] - 2026-07-12

### Added
- **Autotype refuses to type a password into a window that has no login form.** Autotype is
  `SendInput`: the keystrokes go wherever focus happens to be, and the window has no say in it.
  In a chat box that is not a small mistake — a password typed into Discord's message bar, with
  the sequence's trailing Enter behind it, is a password *posted to a channel*. So before a
  secret is typed, the target window is asked through UI Automation (the same public
  accessibility surface the address-bar reader already uses) whether it shows a password field
  at all. A chat window, an editor, a game, the desktop: no field, no typing.
  The second half of the same question is *where* in the window. A browser parked on a login
  page with focus on the page body would otherwise swallow the username and fire the Enter at
  whatever listened, so when the caret isn't in a field, the form's own username field is
  focused first. If only the password field can be reached (a password-only page), the sequence
  is typed **from `{PASSWORD}` on** — typing `{USERNAME}{TAB}` into a password box is how a
  username ends up in a password field and a password wherever the Tab landed.
  A refusal is never silent and never final: the overlay comes back with the credential, the
  reason nothing was typed, and a **Type it anyway** that arms and takes a confirming Enter (the
  copies stay one key away underneath — pasting by hand is usually the right answer to "that is
  not a login form", and always the safer one). UI Automation cannot read every window — games,
  remote sessions, terminals — and a guard with no way past it is a guard the user switches off.
  It is exactly that: a switch, `Vault · autotype → Only autotype into login forms`, on by
  default.
- **Open website & autofill** (⇧Enter on any vault entry with a web URI). For the credential
  whose window isn't open yet: it opens the entry's site in the default browser, waits for the
  browser to actually *be on that site* with a login form up, then fills it in. The waiting is
  what makes it safe — the address bar is matched by the same conservative scorer the context
  suggestions use (registrable-domain equality, deliberately no fuzzy matching), so a page it
  cannot identify is never typed into. A site that never shows a form, or an SSO redirect to a
  domain the entry doesn't name, ends in the same warning row rather than a guess. App-only
  entries (`androidapp://…`) have no site to open and don't offer the action.
  Most saved URIs are **homepages** (`discord.com`, `github.com`), which show no password field
  at all, so the login page is found in three steps — and none of them invents a URL. First the
  item's **`loginurl` custom field**, if it has one (the same escape hatch `autotype` is). Then
  the most login-shaped URI the item already carries: an entry holding both `github.com` and
  `github.com/login` opens the second. Failing both, Funke asks the *page* for its **own sign-in
  link** and clicks it, then goes on waiting for the form. That click is fenced hard: inside the
  page document only (a browser's own chrome has a "Sign in" button — Edge's profile, Chrome's
  sync), and on an **exact** name match only — `"Sign in with Google"` contains "sign in" and
  would hand the session to an identity provider your entry never named. It clicks; it does not
  type, and every check above still has to pass on the page it lands on.
  **Deliberately not built: searching the web for the login page.** It would make the target of
  an autofill something an attacker can pick through SEO or an ad, which is the exact thing the
  guard above exists to prevent. If the page can't be identified from your own vault data or the
  site's own markup, the warning row is the right answer. (DESIGN.md §5 records why.)

### Changed
- Both autofill flows now run **on a worker thread** rather than inside the sync IPC command.
  They inspect the target window through UI Automation, wait for pages, and sleep between
  keystrokes; the main thread is the event loop *and* an STA, where a UIA call can deadlock —
  the same seam `VaultHelloUnlock` already hops off the main thread for.

## [0.4.2] - 2026-07-12

### Fixed
- **Codex rows wore a blank page instead of a logo** (agent-sessions 0.1.1). The icon was taken
  from the tool's binary, which is right for `claude.exe` — it embeds its own mark — but
  `codex.exe` carries no icon resource at all, so the shell answered with Windows' *generic
  console icon*: the blank page on every Codex row. There is nothing there to extract, so the
  Codex mark is now drawn in the plugin, in the same dim ivory the house glyphs use. Claude Code
  still wears its real extracted icon, because a real icon beats a copy of one.

### Changed
- **A plugin is versioned by its own manifest, not by the launcher's tag.** The release packaged
  `funke-plugin-<id>-<tag>.zip`, so agent-sessions 0.1.0 shipped as `…-v0.4.1.zip` — a plugin
  wearing the launcher's version number, which it has no relationship to. Plugins release *on* a
  funke tag; they are not *part of* funke. The archive is now `funke-plugin-<id>-v<its own
  version>.zip`.
  The release also **refuses to package a plugin that changed without its version moving**. That
  is not tidiness: the catalog pins a plugin by version and SHA-256, so a filename is a promise
  about its bytes — publishing different contents under a name a previous release already used
  would quietly make that promise false.

## [0.4.1] - 2026-07-12

### Added
- **An Agent Sessions plugin** (`funke-plugins/agent-sessions`, keyword `cc`). `cc ` lists
  your Claude Code *and* Codex conversations newest-first; `cc <text>` searches them by name,
  by the prompt you opened them with, or by project and branch. Enter resumes one in a
  terminal in the directory it ran in, Shift+Enter opens that directory, and the third action
  copies the resume command for a terminal you already have open. It needs the `claude` and/or
  `codex` CLI on `PATH` — a tool that isn't there refuses to resume and says which one is
  missing, but its sessions still list, because a transcript on disk is proof it once ran.
  **Two sources, one provider**, the shape `funke-files` uses for its walk and Everything: both
  tools answer the same question ("resume what I was working on"), so they share the row, the
  ranking and the actions, and each row wears its tool's *real* icon — extracted from the
  installed binary through the same shell API `funke-apps` uses, rather than a hand-drawn
  imitation of somebody's logo. A tool that has never run here contributes nothing and costs
  one `stat`, so having only one of the two is free.
  Each row is titled with the name Claude Code gives a conversation (`ai-title`), falling back
  to the opening prompt where there is none — Codex does not name its sessions, so it is always
  titled by its prompt. Both formats are the tools' private ones, so both readers treat every
  field as optional and a session they cannot parse is simply not listed: a format change costs
  rows, never a crash.
  It is a **plugin rather than a compiled-in provider** on purpose: it needs none of the host
  seams the built-ins exist for (no focus capture, no masked prompt), it reads files and spawns
  a process, and shipping it out-of-process means the transcript formats can be chased without
  cutting a launcher release. It is `prefix_only` for the reason snippets keep their bodies out
  of global results: an opening prompt is whatever you happened to be typing that day.

### Changed
- **The plugins pane's "Remove?" confirmation had no room to breathe.** The ✕ is a 26px square
  sized for a single glyph, and arming it only widened the box — so the word it turns into was
  squeezed edge to edge, as was the "Removing…" that follows. The armed state is now a class
  rather than an inline width: it gets real padding, and it wears the danger colour of what the
  next click commits to.
- **A plugin can have a browse view.** A scoped query with nothing typed after the keyword
  (`cc `) reaches its provider as an *empty* query — that is how `c ` opens the clipboard's
  history — but the plugin adapter was dropping it, so plugins alone had no way to answer
  "show me everything". They can now. It is the difference between `cc ` listing your last
  sessions and having to guess a letter of one.

### Fixed
- **A credential suggestion wore an empty "Recent" heading.** The overview's standing
  "Recent" strip is the heading for the *unlabelled* case — the one group it could possibly
  be about. As soon as a suggestion arrives the groups grow their own headings ("For
  github.com", then "Recent"), and the strip became a second heading stacked above the first:
  it said "Recent" while sitting over a vault credential, and read as an empty label when
  there were no recents underneath at all. It now appears only when the groups are unlabelled.

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

[Unreleased]: https://github.com/klappstuhlpy/funke/compare/v0.6.0...HEAD
[0.6.0]: https://github.com/klappstuhlpy/funke/compare/v0.5.0...v0.6.0
[0.5.0]: https://github.com/klappstuhlpy/funke/compare/v0.4.2...v0.5.0
[0.4.2]: https://github.com/klappstuhlpy/funke/compare/v0.4.1...v0.4.2
[0.4.1]: https://github.com/klappstuhlpy/funke/compare/v0.4.0...v0.4.1
[0.4.0]: https://github.com/klappstuhlpy/funke/compare/v0.3.1...v0.4.0
[0.3.1]: https://github.com/klappstuhlpy/funke/compare/v0.3.0...v0.3.1
[0.3.0]: https://github.com/klappstuhlpy/funke/compare/v0.2.0...v0.3.0
[0.2.0]: https://github.com/klappstuhlpy/funke/compare/v0.1.1...v0.2.0
[0.1.1]: https://github.com/klappstuhlpy/funke/compare/v0.1.0...v0.1.1
[0.1.0]: https://github.com/klappstuhlpy/funke/releases/tag/v0.1.0
