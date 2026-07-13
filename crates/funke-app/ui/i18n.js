// The frontend's half of the string catalogue. The Rust half (funke-core's `i18n`) owns
// everything that arrives inside a ResultItem — titles, subtitles, action labels — and this
// owns everything the UI writes itself: the key hints, the settings pane, the greeting.
//
// Loaded before main.js/settings.js, which call `setLocale()` with what `invoke("locale")`
// reports. Rust has already resolved `auto` against Windows by then, so the UI never guesses.
//
// Static text lives in the markup behind a `data-i18n` key and is filled in by
// `applyTranslations` — a translator sees it in context, and the HTML stays the layout it is.
// Text assembled at runtime calls `t(key, args)`. A few strings carry inline <code>/<kbd>
// markup and use `data-i18n-html`; they are ours, never user input.

const STRINGS = {
  en: {
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

    "settings.title": "Settings",
    "settings.close": "Close (Esc)",
    "settings.nav.general": "General",
    "settings.nav.appearance": "Appearance",
    "settings.nav.hotkey": "Hotkey",
    "settings.nav.commands": "Commands",
    "settings.nav.vault": "Vault",
    "settings.nav.snippets": "Snippets",
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
    "settings.section.inside": "Inside the overlay",
    "settings.section.providers": "Sources",
    "settings.section.providers.note":
      "Everything Funke can answer with. Turn one off and it goes quiet — its results disappear, and so does its keyword.",
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
    "settings.roots.desc": "What file search covers. Empty means your home folder; changes re-index within seconds.",
    "settings.roots.add": "Add folder",
    "settings.roots.default": "Searching your home folder (default).",
    "settings.roots.remove": "Remove folder",
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
  },

  // German. Written, not translated: du-form, short sentences, and the words a German user
  // actually says — Snippet, Hotkey, Plugin, Overlay, Update, Autotype stay as they are.
  // "Textbaustein" for Snippet or "Zusatztaste" for modifier would be correct and nobody
  // would use them. Bitwarden's own German says "Tresor", so we do too.
  de: {
    "overlay.placeholder": "Suchen…",
    "overlay.master_password": "Master-Passwort…",
    "overlay.keys.navigate": "navigieren",
    "overlay.keys.open": "öffnen",
    "overlay.keys.actions": "Aktionen",
    "overlay.keys.dismiss": "schließen",
    "overlay.actions": "Aktionen",
    "overlay.no_results": "Nichts gefunden",
    "overlay.forget": "Aus „Zuletzt genutzt“ entfernen",
    "overlay.confirm": "Zum Bestätigen erneut Enter — Esc bricht ab",
    "overlay.vault.prompt": "Enter entsperrt den Tresor — Esc bricht ab",
    "overlay.vault.unlocking": "Wird entsperrt…",
    "overlay.result": "1 Treffer",
    "overlay.results": "{count} Treffer",
    "overlay.vault": "Bitwarden-Tresor",
    "overlay.recent": "Zuletzt genutzt",
    "overlay.suggested": "Vorschlag",
    "overlay.suggested_for": "Für {app}",
    "overlay.greeting.night": "Gute Nacht",
    "overlay.greeting.morning": "Guten Morgen",
    "overlay.greeting.afternoon": "Guten Tag",
    "overlay.greeting.evening": "Guten Abend",
    "overlay.uptime.hours": "läuft seit {hours} Std. {minutes} Min.",
    "overlay.uptime.minutes": "läuft seit {minutes} Min.",
    "overlay.tip.search": "Einfach tippen — Apps, Dateien und Fenster",
    "overlay.tip.prefixes": "f <Suche> nur Dateien, w Fenster, g das Web, v den Tresor",
    "overlay.tip.clipboard": "c zeigt, was du kopiert hast — Enter fügt es dort ein, wo du herkamst",
    "overlay.tip.actions": "Tab zeigt alle Aktionen zum ausgewählten Treffer",

    "settings.title": "Einstellungen",
    "settings.close": "Schließen (Esc)",
    "settings.nav.general": "Allgemein",
    "settings.nav.appearance": "Darstellung",
    "settings.nav.hotkey": "Hotkey",
    "settings.nav.commands": "Befehle",
    "settings.nav.vault": "Tresor",
    "settings.nav.snippets": "Snippets",
    "settings.nav.plugins": "Plugins",
    "settings.nav.about": "Über Funke",
    "settings.load_failed": "Die Einstellungen konnten nicht vollständig geladen werden: {error}",

    "settings.section.startup": "Start",
    "settings.section.updates": "Updates",
    "settings.section.overlay": "Overlay",
    "settings.section.summon": "Aufrufen",
    "settings.section.inside": "Im Overlay",
    "settings.section.providers": "Quellen",
    "settings.section.providers.note":
      "Alles, womit Funke antworten kann. Deaktivierst du eine Quelle, ist sie in der Suche nicht mehr verwendbar.",
    "settings.section.web": "Websuche",
    "settings.section.files": "Dateisuche",
    "settings.section.vault_unlock": "Entsperren",
    "settings.section.vault_unlock.note": "Verwalte wie du den Tresor entsperrst — und wann er sich wieder schließt.",
    "settings.section.vault_autotype": "Autotype",
    "settings.section.vault_autotype.note":
      "Verwalte Autotype für den Tresor und wie du damit interagieren kannst.",
    "settings.section.vault_suggest": "Vorschläge",
    "settings.section.vault_suggest.note":
      "Der einzige Fall, in dem ein Tresor-Eintrag auftaucht, ohne dass du mit v suchst.",
    "settings.section.vault_privacy": "Privatsphäre & Vertrauen",
    "settings.section.vault_privacy.note":
      "Wer den Tresor sonst noch auf deinem Bildschirm zu sehen bekommt — und welcher bw Funke dein Master-Passwort überhaupt anvertraut.",
    "settings.section.your_snippets": "Deine Snippets",
    "settings.section.placeholders": "Platzhalter",
    "settings.section.installed": "Installiert",
    "settings.section.catalog": "Katalog",
    "settings.section.links": "Links",

    "settings.general.heading": "Allgemein",
    "settings.general.lead": "Wie sich Funke auf diesem Rechner verhält.",
    "settings.autostart": "Mit Windows starten",
    "settings.autostart.desc": "Funke startet im Infobereich, sobald du dich bei Windows anmeldest.",
    "settings.language": "Sprache",
    "settings.language.desc": "Standard ist die Sprache von Windows. Treffer und Menüs wechseln mit.",
    "settings.language.auto": "Systemsprache",
    "settings.updates": "Updates",
    "settings.updates.desc": "Auf GitHub nach neuen Versionen suchen.",
    "settings.updates.check": "Nach Updates suchen",
    "settings.updates.checking": "Wird geprüft…",
    "settings.updates.none": "Du hast die neueste Version.",
    "settings.updates.available": "Version {version} ist da — das hat sich geändert:",
    "settings.updates.install": "Installieren und neu starten",
    "settings.updates.installing": "Wird geladen… Funke startet danach neu.",
    "settings.updates.auto": "Über neue Versionen Bescheid geben",
    "settings.updates.auto.desc":
      "Kurz nach dem Start nach einer neuen Version schauen und beim ersten Mal eine Windows-Benachrichtigung zeigen — einmal pro Version, dieselbe nie wieder. Es sagt dir nur Bescheid; installiert wird erst, wenn du darauf drückst.",

    "settings.appearance.heading": "Darstellung",
    "settings.appearance.lead": "Das Overlay bleibt echtes Glas — Farbe und Breite bestimmst du.",
    "settings.accent": "Akzentfarbe",
    "settings.accent.desc": "Für Auswahl, Cursor und Tastenhinweise.",
    "settings.width": "Breite des Overlays",
    "settings.width.desc": "Wie breit die Suchleiste ist.",

    "settings.hotkey.lead": "Das Tastenkürzel, das Funke von überall aufruft.",
    "settings.hotkey.label": "Funke öffnen",
    "settings.hotkey.desc": "Klicken, dann die gewünschte Kombination drücken. Gilt sofort.",
    "settings.hotkey.fineprint":
      "Ist die Kombination schon von einer anderen App belegt (PowerToys Run mag <kbd>Strg</kbd>+<kbd>Leertaste</kbd> auch), klappt es nicht — und der bisherige Hotkey bleibt aktiv.",
    "settings.hotkey.recording": "Tasten drücken…",
    "settings.hotkey.needs_modifier": "Noch Strg, Alt oder Win dazu…",
    "settings.shortcuts.navigate": "Durch die Treffer navigieren",
    "settings.shortcuts.open": "Den ausgewählten Treffer ausführen",
    "settings.shortcuts.alt": "Seine zweite Aktion ausführen",
    "settings.shortcuts.actions": "Alle Aktionen zum ausgewählten Treffer zeigen",
    "settings.shortcuts.nth": "Die n-te Aktion direkt ausführen",
    "settings.shortcuts.dismiss": "Overlay schließen oder eine Bestätigung abbrechen",

    "settings.commands.heading": "Befehle",
    "settings.commands.lead": "Die Befehle die du in der Spotlight Suche verwenden kannst um nach Inhalten zu filtern.",
    "settings.engine": "Suchmaschine",
    "settings.engine.desc": "Wo „Im Web nach … suchen“ öffnet.",

    "settings.vault.lead":
      "Dein Bitwarden- oder Vaultwarden-Tresor, erreichbar über <kbd>v</kbd>. Funke entschlüsselt nichts selbst — das macht die offizielle <code>bw</code>-CLI, Funke kommuniziert nur mit ihr.",
    "settings.vault.hello": "Mit Windows Hello entsperren",
    "settings.vault.hello.desc":
      "Einmal mit dem Master-Passwort entsperren, danach immer mit Windows Hello. Die gespeicherte Sitzung ist mit einem Schlüssel versiegelt, den dein TPM erst herausrückt, wenn Hello dich erkannt hat — an der Abfrage vorbei kommt also niemand. Was das bringt und was nicht, steht in SECURITY.md.",
    "settings.vault.icons": "Website-Symbole",
    "settings.vault.icons.desc":
      "Zeigt Favicons auf den Tresor-Einträgen. Die kommen vom Icon-Dienst deines Bitwarden-Servers — der erfährt damit, nach welchen Seiten du suchst.",
    "settings.vault.context": "Zur App im Vordergrund vorschlagen",
    "settings.vault.context.desc":
      "Rufst du Funke über einer App auf (oder über einer Website im Browser), schlägt das leere Overlay gleich die passenden Zugangsdaten vor — erkannt an Prozess, Fenstertitel und der Adresszeile des Browsers. Aus: Tresor-Einträge tauchen nur hinter <code>v</code> auf.",
    "settings.vault.guard": "Nur in Login-Formulare tippen",
    "settings.vault.guard.desc":
      "Tippt ein Passwort nur in ein Fenster, das ein Passwortfeld zeigt. Das verhindert, dass Zugangsdaten — und das Enter dahinter — in einem Chatfenster, einer Suchleiste oder auf dem Desktop landen. Wird ein Versuch blockiert, kommt der Eintrag mit dem Grund zurück, dazu ein <b>Trotzdem tippen</b> zum Bestätigen: Fenster, die Funke nicht lesen kann (Spiele, Remote-Sitzungen, Terminals), funktionieren also weiter — sie fragen nur vorher.",
    "settings.vault.autotype_enter": "Autotype drückt Enter",
    "settings.vault.autotype_enter.desc":
      "Schickt das Formular nach dem Passwort gleich ab. Ist es aus, tippt Funke nur die Zugangsdaten — Enter bleibt dir überlassen. Gilt nur für die eingebaute Sequenz; eine eigene unten wird genau so getippt, wie sie dasteht.",
    "settings.vault.sequence": "Autotype-Sequenz",
    "settings.vault.sequence.desc":
      "Was Autotype tippen soll, wenn nicht das übliche „Benutzername ⇥ Passwort“. Platzhalter: <code>{USERNAME}</code> <code>{PASSWORD}</code> <code>{TOTP}</code> <code>{TAB}</code> <code>{ENTER}</code> <code>{DELAY=500}</code>. Ein einzelner Eintrag kann das mit einem <code>autotype</code>-Feld in Bitwarden überschreiben.",
    "settings.vault.lock_screen": "Mit Windows sperren",
    "settings.vault.lock_screen.desc":
      "Sperrt den Tresor, sobald Windows gesperrt wird (Win+L), der Rechner in den Ruhezustand geht oder eine Remote-Sitzung getrennt wird.",
    "settings.vault.capture_shield": "Tresorinhalte vor Bildschirmaufnahmen verbergen",
    "settings.vault.capture_shield.desc":
      "Screenshots, Aufnahmen und geteilte Bildschirme sehen das Overlay nicht, solange es die Master-Passwort-Abfrage oder Tresoreinträge zeigt. Alles andere bleibt aufnehmbar.",
    "settings.vault.signed_cli": "Nur Bitwarden signierte CLI erlauben",
    "settings.vault.signed_cli.desc":
      "Funke gibt dein Master-Passwort an die bw-Datei weiter — deshalb merkt es sich beim Start genau eine und prüft, ob Bitwarden sie signiert hat. Normalerweise läuft eine unbestätigte bw trotzdem, mit dem Grund auf der Tresorzeile: Eine npm-Installation ist ein unsignierter Skript-Wrapper und eine völlig normale Art, sie zu installieren. Schalte das hier ein, wenn Funke sie stattdessen ablehnen soll.",
    "settings.vault.idle": "Automatisches Sperren bei Untätigkeit",
    "settings.vault.idle.desc": "Sperrt den Tresor wieder, wenn du ihn so lange nicht benutzt hast.",
    "settings.idle.minutes": "{count} Minuten",
    "settings.idle.minute": "1 Minute",
    "settings.idle.hour": "1 Stunde",
    "settings.idle.never": "Nie",

    "settings.roots": "Ordner für die Dateisuche",
    "settings.roots.desc":
      "Was die Dateisuche abdeckt. Leer heißt: dein Benutzerordner. Änderungen greifen nach ein paar Sekunden.",
    "settings.roots.add": "Ordner hinzufügen",
    "settings.roots.default": "Es wird dein Benutzerordner durchsucht (Standard).",
    "settings.roots.remove": "Ordner entfernen",
    "settings.everything": "Indizierung durch Everything",
    "settings.everything.detected": "erkannt",
    "settings.everything.desc":
      "Everything wird verwendet, statt selbst zu indizieren. Die Treffer sind sekundenaktuell, und nichts wird doppelt indiziert. Die Ordner oben grenzen die Suche weiterhin ein; für alle Laufwerke einfach <code>C:\\</code> hinzufügen. Schließt du Everything, nutzt Funke wieder seinen eigenen Index.",

    "settings.snippets.lead":
      "Text, den du ständig brauchst — Signatur, Adresse, ein Standardabsatz. Mit <kbd>s</kbd> finden, Enter tippt ihn dort ein, wo du herkamst.",
    "settings.snippets.manage": "Deine Snippets",
    "settings.snippets.manage.desc": "Alles, was du gespeichert hast. Bearbeite oder fünge Einträge hinzu.",
    "settings.snippets.placeholders": "Was reinpasst",
    "settings.snippets.placeholders.desc":
      "Werden beim Einfügen aufgelöst, nicht beim Speichern: <code>{DATE}</code>, <code>{TIME}</code>, <code>{DATETIME}</code> (oder dein eigenes Format, <code>{DATE:%d.%m.%Y}</code>), <code>{CLIPBOARD}</code> für das zuletzt Kopierte und <code>{CURSOR}</code> für die Stelle, an der der Cursor landen soll. Alles andere in geschweiften Klammern wird genau so getippt, wie es dasteht.",
    "settings.snippets.new": "Neues Snippet",
    "settings.snippets.name": "Name",
    "settings.snippets.abbr": "Kürzel",
    "settings.snippets.optional": "optional",
    "settings.snippets.content": "Inhalt",
    "settings.snippets.save": "Änderungen speichern",
    "settings.snippets.create": "Snippet anlegen",
    "settings.snippets.cancel": "Abbrechen",
    "settings.snippets.edit": "Bearbeiten",
    "settings.snippets.delete": "Snippet löschen",
    "settings.snippets.incomplete": "Ein Snippet braucht einen Namen und einen Inhalt.",
    "settings.snippets.empty":
      "Noch keine Snippets. Leg oben eines an und verwende es mit <kbd>s</kbd> oder direkt über den Namen/Kürzel.",

    "settings.plugins.lead":
      "Eigene Programme für die Spotlight Suche — geschrieben in jeder Sprache. Ordner mit einer <code>plugin.json</code> ins Plugin-Verzeichnis legen, dann Aktualisieren.",
    "settings.plugins.folder": "Plugin-Ordner",
    "settings.plugins.folder.desc":
      "Plugin hineinlegen, dann Aktualisieren — ganz ohne Neustart. Für Entwicklung, siehe docs/PLUGINS.md.",
    "settings.plugins.refresh": "Aktualisieren",
    "settings.plugins.open": "Ordner öffnen",
    "settings.plugins.empty":
      "Noch keine Plugins installiert. Schau unten in den Katalog — oder schreib selbst eins (<code>docs/PLUGINS.md</code>). Im Repo liegen Vorlagen (<code>funke-plugins/template</code> in Rust, <code>funke-plugins/template-python</code> in Python): bauen, samt <code>plugin.json</code> in den Ordner oben legen, Aktualisieren drücken und <kbd>tp hello</kbd> probieren.",
    "settings.plugins.suggested": "Empfohlene Plugins",
    "settings.plugins.suggested.desc":
      "Katalog aus dem Funke-Repo. Jeder Eintrag hängt an einer Prüfsumme — installiert wird also genau das, was auch geprüft wurde. Trotzdem bleibt ein Plugin ein Programm mit deinen Rechten, keine Sandbox: installier nur das, dem du vertraust.",
    "settings.plugins.browse": "Katalog ansehen",
    "settings.plugins.loading": "Wird geladen…",
    "settings.plugins.install": "Installieren",
    "settings.plugins.installing": "Wird installiert…",
    "settings.plugins.installed": "Installiert",
    "settings.plugins.uninstall": "{name} deinstallieren",
    "settings.plugins.remove_confirm": "Entfernen?",
    "settings.plugins.removing": "Wird entfernt…",
    "settings.plugins.catalog_empty": "Noch nichts da",
    "settings.plugins.catalog_empty.desc": "Der Katalog ist noch leer, füge ein Plugin hinzu um es zu verwenden.",

    "settings.about.lead": "Was ist Funke?",
    "settings.about.tagline":
      "Ein Launcher für Windows, ganz per Tastatur: Apps, Dateien, Fenster, Snippets, Zwischenablage und dein Tresor — einen Hotkey entfernt.",
    "settings.about.built": "Freie Software, entwickelt für Windows.",
    "settings.about.built.desc":
      "Geschrieben in Rust auf Tauri, unter MIT-Lizenz. Kein Konto, keine Telemetrie, keine Analyse: Nichts von dem, was du tippst, verlässt deinen Rechner. Funke holt sich nur den Plugin-Katalog und die Update-Prüfung — und beides nur, wenn du danach fragst.",
    "settings.about.fineprint": "„Funke“ ist der Projektname. Issues und Pull Requests sind willkommen.",
    "settings.about.source": "Quellcode",
    "settings.about.issues": "Fehler melden",
    "settings.about.releases": "Releases",
    "settings.about.changelog": "Changelog",
    "settings.about.design": "Design & Entscheidungen",
    "settings.about.plugins": "Ein Plugin schreiben",
    "settings.about.security": "Sicherheit",
    "settings.about.license": "Lizenz (MIT)",
  },
};

let locale = "en";

function setLocale(tag) {
  locale = STRINGS[tag] ? tag : "en";
  document.documentElement.lang = locale;
}

// An untranslated key shows itself rather than vanishing: a hole in the catalogue should be
// obvious the first time it renders, not a blank label nobody notices.
function t(key, args) {
  const text = (STRINGS[locale] && STRINGS[locale][key]) || STRINGS.en[key] || key;
  if (!args) return text;
  return Object.entries(args).reduce((filled, [name, value]) => filled.replaceAll(`{${name}}`, value), text);
}

function applyTranslations(root = document) {
  root.querySelectorAll("[data-i18n]").forEach((el) => {
    el.textContent = t(el.dataset.i18n);
  });
  // Only for the handful of strings that carry <code>/<kbd> markup, and only from this file
  // — never from settings, a plugin, or anything else the user can put words into.
  root.querySelectorAll("[data-i18n-html]").forEach((el) => {
    el.innerHTML = t(el.dataset.i18nHtml);
  });
  root.querySelectorAll("[data-i18n-placeholder]").forEach((el) => {
    el.placeholder = t(el.dataset.i18nPlaceholder);
  });
  root.querySelectorAll("[data-i18n-title]").forEach((el) => {
    el.title = t(el.dataset.i18nTitle);
  });
}
