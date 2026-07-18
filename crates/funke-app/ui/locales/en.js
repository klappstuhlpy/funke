// English — the source of truth. Every other locale file is checked against these keys by a
// test in funke-app (`ui_locales_stay_in_step`), so a key added here without a counterpart
// elsewhere fails the build rather than showing up untranslated at a user.
//
// This is the UI's half of the catalogue: everything the settings window and the overlay write
// themselves. The other half — everything that arrives inside a ResultItem — lives in
// crates/funke-core/locales/. See docs/TRANSLATING.md.
//
// A few strings carry inline <code>/<kbd> markup and are rendered with data-i18n-html. They are
// ours, never user input; keep the markup, translate around it.

window.FUNKE_STRINGS = window.FUNKE_STRINGS || {};
window.FUNKE_STRINGS.en = {
  "overlay.placeholder": "Search…",
  "overlay.master_password": "Master password…",
  "overlay.keys.navigate": "navigate",
  "overlay.keys.open": "open",
  "overlay.keys.actions": "actions",
  "overlay.keys.dismiss": "dismiss",
  "overlay.actions": "Actions",
  "overlay.no_results": "No results",
  "overlay.forget": "Remove from recents",
  "overlay.confirm": "Press Enter again to confirm — Esc cancels",
  "overlay.vault.prompt": "Enter unlocks the vault — Esc cancels",
  "overlay.vault.unlocking": "Unlocking…",
  "overlay.result": "1 result",
  "overlay.results": "{count} results",
  "overlay.vault": "Bitwarden vault",
  "overlay.recent": "Recent",
  "overlay.suggested": "Suggested",
  "overlay.suggested_for": "For {app}",
  "overlay.greeting.night": "Good night",
  "overlay.greeting.morning": "Good morning",
  "overlay.greeting.afternoon": "Good afternoon",
  "overlay.greeting.evening": "Good evening",
  "overlay.uptime.hours": "up {hours} h {minutes} min",
  "overlay.uptime.minutes": "up {minutes} min",
  "overlay.tip.search": "Type to search apps, files, and windows",
  "overlay.tip.prefixes": "f <query> searches files only, w windows, g the web, v your vault",
  "overlay.tip.clipboard": "c shows what you copied — Enter pastes it back into the window you came from",
  "overlay.tip.actions": "Tab lists every action of the selected result",
  "overlay.tip.content": "ff <query> looks inside your files — the words in the document, not its name",

  "settings.title": "Settings",
  "settings.close": "Close (Esc)",
  "settings.nav.general": "General",
  "settings.nav.appearance": "Appearance",
  "settings.nav.hotkey": "Hotkey",
  "settings.nav.commands": "Commands",
  "settings.nav.vault": "Vault",
  "settings.nav.snippets": "Snippets",
  "settings.nav.quicklinks": "Quicklinks",
  "settings.nav.plugins": "Plugins",
  "settings.nav.about": "About",
  "settings.load_failed": "Settings didn't load fully: {error}",

  // Category headings, and the one line under each that says what the whole group is for.
  // A heading names the subject so no row has to repeat it; the note says why the group
  // exists, so the rows underneath can get straight to what they do.
  "settings.section.startup": "Startup",
  "settings.section.updates": "Updates",
  "settings.section.overlay": "Overlay",
  "settings.section.summon": "Summon",
  "settings.section.scopes": "Scoped shortcuts",
  "settings.section.scopes.note":
    "A shortcut that opens Funke already inside one source — straight to your clipboard history, or your vault, without passing through a general search first.",
  "settings.section.inside": "Inside the overlay",
  "settings.section.providers": "Sources",
  "settings.section.providers.note":
    "Everything Funke can answer with. Turn one off and it goes quiet — its results disappear, and so does its keyword.",
  "settings.providers.keyword": "Type {prefix} and a space to search only here",
  "settings.section.web": "Web search",
  "settings.section.files": "File search",
  "settings.section.vault_unlock": "Unlocking",
  "settings.section.vault_unlock.note": "How you open the vault — and when it closes itself again.",
  "settings.section.vault_autotype": "Autotype",
  "settings.section.vault_autotype.note":
    "What gets typed into the window you came from, and the guard that decides whether anything is typed at all.",
  "settings.section.vault_suggest": "Suggestions",
  "settings.section.vault_suggest.note": "The one time a vault entry appears without you typing v.",
  "settings.section.vault_privacy": "Privacy & trust",
  "settings.section.vault_privacy.note":
    "Who else can see the vault on your screen, and which bw Funke is willing to hand your master password to.",
  "settings.section.your_snippets": "Your snippets",
  "settings.section.placeholders": "Placeholders",
  "settings.section.your_quicklinks": "Your quicklinks",
  "settings.section.quicklink_argument": "Arguments",
  "settings.section.installed": "Installed",
  "settings.section.catalog": "Catalog",
  "settings.section.links": "Links",

  "settings.general.heading": "General",
  "settings.general.lead": "How Funke lives on this machine.",
  "settings.autostart": "Launch at startup",
  "settings.autostart.desc": "Start Funke in the tray when you sign in to Windows.",
  "settings.language": "Language",
  "settings.language.desc": "Follows Windows unless you choose one. Results and menus change with it.",
  "settings.language.auto": "Follow Windows",
  "settings.updates": "Updates",
  "settings.updates.desc": "Check GitHub for a newer release.",
  "settings.updates.check": "Check for updates",
  "settings.updates.checking": "Checking…",
  "settings.updates.none": "You're on the latest version.",
  "settings.updates.available": "Version {version} is available — here's what changed.",
  "settings.updates.install": "Install and restart",
  "settings.updates.installing": "Downloading… Funke will restart when it's done.",
  "settings.updates.auto": "Tell me about new versions",
  "settings.updates.auto.desc":
    "Look for a new release shortly after startup and show a Windows notification the first time there is one — once per version, never again for the same one. It only ever tells you; installing stays a button you press.",

  "settings.appearance.heading": "Appearance",
  "settings.appearance.lead": "The overlay stays native glass — these tune its character.",
  "settings.accent": "Accent",
  "settings.accent.desc": "Used for the selection, caret, and key hints.",
  "settings.width": "Overlay width",
  "settings.width.desc": "How wide the search bar is, in any mode.",

  "settings.hotkey.lead": "The global shortcut that summons the overlay.",
  "settings.hotkey.label": "Summon Funke",
  "settings.hotkey.desc": "Click, then press the new combination. Applied immediately.",
  "settings.hotkey.fineprint":
    "If another app owns the combination (PowerToys Run also likes <kbd>Ctrl</kbd>+<kbd>Space</kbd>), binding fails and the previous hotkey stays active.",
  "settings.hotkey.recording": "Press keys…",
  "settings.hotkey.needs_modifier": "Add a modifier…",
  "settings.scopes.manage": "Your scoped shortcuts",
  "settings.scopes.manage.desc": "Each one summons Funke with a source's keyword already typed.",
  "settings.scopes.new": "Add a shortcut",
  "settings.scopes.opens": "Opens",
  "settings.scopes.opens.desc": "The same thing typing {prefix} and a space does — done for you.",
  "settings.scopes.unbound": "Set keys…",
  "settings.scopes.delete": "Remove shortcut",
  "settings.shortcuts.navigate": "Move through the results",
  "settings.shortcuts.open": "Run the selected result",
  "settings.shortcuts.alt": "Run its second action",
  "settings.shortcuts.actions": "List every action of the selected result",
  "settings.shortcuts.nth": "Run the nth action straight away",
  "settings.shortcuts.dismiss": "Close the overlay, or cancel a confirmation",

  "settings.commands.heading": "Commands",
  "settings.commands.lead": "Where Funke looks, and how each source behaves.",
  "settings.engine": "Web search engine",
  "settings.engine.desc": "Where “Search the web for …” sends you.",

  "settings.vault.lead":
    "Your Bitwarden or Vaultwarden vault, behind <kbd>v</kbd>. Funke decrypts nothing itself — the official <code>bw</code> CLI does that, and Funke only talks to it.",
  "settings.vault.hello": "Unlock with Windows Hello",
  "settings.vault.hello.desc":
    "Unlock with your master password once; after that a Hello prompt is enough. The saved session is sealed with a key your TPM hands over only once Hello has recognised you, so there is no way to it that goes around the prompt. What that does and doesn't buy you is spelled out in SECURITY.md.",
  "settings.vault.icons": "Website icons",
  "settings.vault.icons.desc":
    "Show site favicons on vault entries. They come from your Bitwarden server's icon service, which therefore learns which sites you search for.",
  "settings.vault.context": "Suggest for the focused app",
  "settings.vault.context.desc":
    "When you summon Funke over an app (or a website in your browser), offer that credential in the empty overlay — matched by process, window title, and the browser's address bar. Off means vault entries only ever appear behind <code>v</code>.",
  "settings.vault.guard": "Only autotype into login forms",
  "settings.vault.guard.desc":
    "Type a password only into a window that shows a password field. It's what stops a credential — and the Enter behind it — from landing in a chat box, a search bar, or the desktop. A blocked attempt comes back with the entry, the reason, and a <b>Type it anyway</b> you can confirm, so windows Funke can't read (games, remote sessions, terminals) still work — they just ask first.",
  "settings.vault.autotype_enter": "Autotype presses Enter",
  "settings.vault.autotype_enter.desc":
    "After typing the password, submit the form automatically. Turn off to type the credentials and stop, leaving Enter to you. Applies to the built-in sequence only — a custom one below is typed exactly as written.",
  "settings.vault.sequence": "Autotype sequence",
  "settings.vault.sequence.desc":
    "What autotype types, if not the usual username ⇥ password. Tokens: <code>{USERNAME}</code> <code>{PASSWORD}</code> <code>{TOTP}</code> <code>{TAB}</code> <code>{ENTER}</code> <code>{DELAY=500}</code>. A single entry can override this with an <code>autotype</code> custom field in Bitwarden.",
  "settings.vault.lock_screen": "Lock when you step away",
  "settings.vault.lock_screen.desc":
    "Lock the vault the moment Windows locks (Win+L), the machine goes to sleep, or a remote session disconnects.",
  "settings.vault.capture_shield": "Hide vault content from screen capture",
  "settings.vault.capture_shield.desc":
    "Screenshots, recordings and screen shares can't see the overlay while it shows the master-password prompt or vault entries. Everything else stays capturable.",
  "settings.vault.signed_cli": "Only run a Bitwarden-signed CLI",
  "settings.vault.signed_cli.desc":
    "Funke hands your master password to the bw executable, so it pins the one it found at startup and checks that Bitwarden signed it. An unverified bw normally still runs, with the reason shown on the vault row — an npm install is an unsigned script wrapper, and that is a perfectly good way to install it. Turn this on to refuse it instead.",
  "settings.vault.idle": "Auto-lock when idle",
  "settings.vault.idle.desc": "Lock the vault again after this long without using it.",
  "settings.idle.minutes": "{count} minutes",
  "settings.idle.minute": "1 minute",
  "settings.idle.hour": "1 hour",
  "settings.idle.never": "Never",

  "settings.roots": "File index folders",
  "settings.roots.desc":
    "Where both file searches look — f by name, ff by what is written inside. Empty means your home folder; changes re-index within seconds.",
  "settings.roots.add": "Add folder",
  "settings.roots.default": "Searching your home folder (default).",
  "settings.roots.remove": "Remove folder",
  "settings.index_hidden": "Include hidden folders",
  "settings.index_hidden.desc":
    "Also search inside hidden and system folders. Off by default — results stay cleaner without them.",
  "settings.everything": "Everything is doing the indexing",
  "settings.everything.detected": "detected",
  "settings.everything.desc":
    "Everything keeps a live index of your drives, so Funke asks it instead of walking the disk itself — results are current to the second and nothing is indexed twice. The folders above still scope the search; add <code>C:\\</code> to search every drive. Close Everything and Funke goes back to its own index.",

  "settings.snippets.lead":
    "Text you paste often — a signature, an address, a block of boilerplate. Find one with <kbd>s</kbd>, and Enter types it into the window you came from.",
  "settings.snippets.manage": "Your snippets",
  "settings.snippets.manage.desc": "Everything you have saved. Edit one, or write another.",
  "settings.snippets.placeholders": "What you can put in one",
  "settings.snippets.placeholders.desc":
    "Resolved when the snippet is pasted, not when it is saved: <code>{DATE}</code>, <code>{TIME}</code>, <code>{DATETIME}</code> (or your own format, <code>{DATE:%d.%m.%Y}</code>), <code>{CLIPBOARD}</code> for what you last copied, and <code>{CURSOR}</code> for where the caret should land. Anything else in braces is typed exactly as written.",
  "settings.snippets.new": "New snippet",
  "settings.snippets.name": "Name",
  "settings.snippets.abbr": "Abbreviation",
  "settings.snippets.optional": "optional",
  "settings.snippets.content": "Content",
  "settings.snippets.save": "Save changes",
  "settings.snippets.create": "Create snippet",
  "settings.snippets.cancel": "Cancel",
  "settings.snippets.edit": "Edit",
  "settings.snippets.delete": "Delete snippet",
  "settings.snippets.incomplete": "A snippet needs a name and some content.",
  "settings.snippets.empty":
    "No snippets yet. Create one above, then reach it with <kbd>s</kbd> — or by the name you gave it, straight from the search.",

  "settings.quicklinks.lead":
    "Pages you open often, by the name you know them by. Give one an argument slot and it becomes a search: <kbd>yt lofi beats</kbd> goes straight to the results.",
  "settings.quicklinks.manage": "Your quicklinks",
  "settings.quicklinks.manage.desc": "Everything you have saved. Edit one, or add another.",
  "settings.quicklinks.argument": "The argument slot",
  "settings.quicklinks.argument.desc":
    "Put <code>{query}</code> anywhere in the URL and whatever you type after the abbreviation lands there — <code>yt lofi beats</code> in, <code>?search_query=lofi%20beats</code> out. Leave it out and the link simply opens. A URL without an abbreviation is found by its name, like everything else.",
  "settings.quicklinks.new": "New quicklink",
  "settings.quicklinks.name": "Name",
  "settings.quicklinks.abbr": "Abbreviation",
  "settings.quicklinks.optional": "optional",
  "settings.quicklinks.url": "URL",
  "settings.quicklinks.save": "Save changes",
  "settings.quicklinks.create": "Create quicklink",
  "settings.quicklinks.cancel": "Cancel",
  "settings.quicklinks.edit": "Edit",
  "settings.quicklinks.delete": "Delete quicklink",
  "settings.quicklinks.incomplete": "A quicklink needs a name and a URL.",
  "settings.quicklinks.bad_url": "A quicklink has to start with http:// or https://.",
  "settings.quicklinks.empty":
    "No quicklinks yet. Create one above, then reach it by its name — or by its abbreviation, with whatever you want to look up behind it.",

  "settings.plugins.lead":
    "Out-of-process providers in any language — drop a folder with a <code>plugin.json</code> into the plugins directory, then Refresh.",
  "settings.plugins.folder": "Plugins folder",
  "settings.plugins.folder.desc":
    "Drop a plugin in, then Refresh to load it without a restart. See docs/PLUGINS.md to write one.",
  "settings.plugins.refresh": "Refresh",
  "settings.plugins.open": "Open folder",
  "settings.plugins.empty":
    "No plugins installed yet. Browse the catalog below, or write your own with <code>docs/PLUGINS.md</code> — the repository ships templates (<code>funke-plugins/template</code> in Rust, <code>funke-plugins/template-python</code> in Python): build one next to its <code>plugin.json</code> into the folder above, hit Refresh, then try <kbd>tp hello</kbd>.",
  "settings.plugins.suggested": "Suggested plugins",
  "settings.plugins.suggested.desc":
    "A curated catalog from the Funke repository. Every entry is pinned to a checksum, so what installs is what was reviewed — but a plugin is a program that runs with your rights, not a sandbox. Install only what you trust.",
  "settings.plugins.browse": "Browse",
  "settings.plugins.loading": "Loading…",
  "settings.plugins.install": "Install",
  "settings.plugins.installing": "Installing…",
  "settings.plugins.update": "Update to {version}",
  "settings.plugins.updating": "Updating…",
  "settings.plugins.installed": "Installed",
  "settings.plugins.uninstall": "Uninstall {name}",
  "settings.plugins.remove_confirm": "Remove?",
  "settings.plugins.removing": "Removing…",
  "settings.plugins.catalog_empty": "Nothing here yet",
  "settings.plugins.catalog_empty.desc": "The catalog is empty — write the first one!",

  "settings.about.lead": "What this is, and where the rest of it lives. :)",
  "settings.about.tagline":
    "A keyboard launcher for Windows: apps, files, windows, snippets, the clipboard, and your vault — one hotkey away.",
  "settings.about.built": "Free software, developed for Windows.",
  "settings.about.built.desc":
    "Written in Rust on Tauri, MIT-licensed. No account, no telemetry, no analytics: nothing you type is sent anywhere, and the only things Funke fetches are the plugin catalog and update checks — both only when you ask.",
  "settings.about.fineprint": "Funke the project title. Issues and pull requests welcome.",
  "settings.about.source": "Source code",
  "settings.about.issues": "Report an issue",
  "settings.about.releases": "Releases",
  "settings.about.changelog": "Changelog",
  "settings.about.design": "Design & decisions",
  "settings.about.plugins": "Writing a plugin",
  "settings.about.security": "Security policy",
  "settings.about.license": "License (MIT)",
};
