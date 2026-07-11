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
    "overlay.confirm": "Press Enter again to confirm — Esc cancels",
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
    "settings.nav.snippets": "Snippets",
    "settings.nav.plugins": "Plugins",
    "settings.load_failed": "Settings didn't load fully: {error}",

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

    "settings.commands.heading": "Commands",
    "settings.commands.lead": "Which built-in providers answer your searches.",
    "settings.engine": "Web search engine",
    "settings.engine.desc": "Where “Search the web for …” sends you.",
    "settings.vault.hello": "Vault: unlock with Windows Hello",
    "settings.vault.hello.desc":
      "After one master-password unlock, later unlocks are a Hello prompt. Stores a session key protected by Windows — tradeoff in SECURITY.md.",
    "settings.vault.icons": "Vault: website icons",
    "settings.vault.icons.desc":
      "Show site favicons on vault entries, fetched from your Bitwarden server's icon service (it learns the sites you search).",
    "settings.vault.context": "Vault: suggest for the focused app",
    "settings.vault.context.desc":
      "When you summon Funke over Discord (or a GitHub tab), offer that credential in the empty overlay — matched by process, window title, and the browser's address bar. Off means vault entries only ever appear behind <code>v</code>.",
    "settings.vault.autotype_enter": "Vault: autotype presses Enter",
    "settings.vault.autotype_enter.desc":
      "After typing the password, submit the form automatically. Turn off to type the credentials and stop, leaving Enter to you. Applies to the built-in sequence only — a custom one below is typed exactly as written.",
    "settings.vault.sequence": "Vault: autotype sequence",
    "settings.vault.sequence.desc":
      "What autotype types, if not the usual username ⇥ password. Tokens: <code>{USERNAME}</code> <code>{PASSWORD}</code> <code>{TOTP}</code> <code>{TAB}</code> <code>{ENTER}</code> <code>{DELAY=500}</code>. A single entry can override this with an <code>autotype</code> custom field in Bitwarden.",
    "settings.vault.lock_screen": "Vault: lock on screen lock",
    "settings.vault.lock_screen.desc": "Lock the vault immediately when you lock Windows (Win+L).",
    "settings.vault.idle": "Vault: auto-lock after idle",
    "settings.vault.idle.desc": "Lock the vault after this long without using it.",
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
    "settings.snippets.placeholders": "Placeholders",
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
    "overlay.confirm": "Zum Bestätigen noch mal Enter — Esc bricht ab",
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
    "settings.nav.snippets": "Snippets",
    "settings.nav.plugins": "Plugins",
    "settings.load_failed": "Die Einstellungen konnten nicht vollständig geladen werden: {error}",

    "settings.general.heading": "Allgemein",
    "settings.general.lead": "Wie sich Funke auf diesem Rechner verhält.",
    "settings.autostart": "Mit Windows starten",
    "settings.autostart.desc": "Funke startet im Infobereich, sobald du dich bei Windows anmeldest.",
    "settings.language": "Sprache",
    "settings.language.desc": "Standard ist die Sprache von Windows. Treffer und Menüs wechseln sofort mit.",
    "settings.language.auto": "Wie Windows",
    "settings.updates": "Updates",
    "settings.updates.desc": "Auf GitHub nach einer neueren Version schauen.",
    "settings.updates.check": "Nach Updates suchen",
    "settings.updates.checking": "Wird geprüft…",

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
      "Ist die Kombination schon von einer anderen App belegt (PowerToys Run mag <kbd>Strg</kbd>+<kbd>Leertaste</kbd> auch), klappt es nicht und der bisherige Hotkey bleibt.",
    "settings.hotkey.recording": "Tasten drücken…",
    "settings.hotkey.needs_modifier": "Noch Strg, Alt oder Win dazu…",

    "settings.commands.heading": "Befehle",
    "settings.commands.lead": "Welche eingebauten Funktionen auf deine Suche antworten.",
    "settings.engine": "Suchmaschine",
    "settings.engine.desc": "Wohin „Im Web nach … suchen“ dich schickt.",
    "settings.vault.hello": "Tresor: mit Windows Hello entsperren",
    "settings.vault.hello.desc":
      "Einmal mit dem Master-Passwort entsperren — danach reicht Windows Hello. Dafür liegt ein Sitzungsschlüssel auf der Platte, den Windows schützt. Was das bedeutet, steht in SECURITY.md.",
    "settings.vault.icons": "Tresor: Website-Symbole",
    "settings.vault.icons.desc":
      "Zeigt Favicons auf den Tresor-Einträgen. Die kommen vom Icon-Dienst deines Bitwarden-Servers — der erfährt damit, nach welchen Seiten du suchst.",
    "settings.vault.context": "Tresor: Zugangsdaten zur App im Vordergrund",
    "settings.vault.context.desc":
      "Rufst du Funke über Discord auf (oder über einen GitHub-Tab), schlägt das leere Overlay gleich die passenden Zugangsdaten vor — erkannt an Prozess, Fenstertitel und der Adresszeile des Browsers. Aus: Tresor-Einträge tauchen nur hinter <code>v</code> auf.",
    "settings.vault.autotype_enter": "Tresor: Autotype drückt Enter",
    "settings.vault.autotype_enter.desc":
      "Schickt das Formular nach dem Passwort gleich ab. Ist es aus, tippt Funke nur die Zugangsdaten — Enter bleibt dir überlassen. Gilt nur für die eingebaute Sequenz; eine eigene unten wird genau so getippt, wie sie dasteht.",
    "settings.vault.sequence": "Tresor: Autotype-Sequenz",
    "settings.vault.sequence.desc":
      "Was Autotype tippen soll, wenn nicht das übliche „Benutzername ⇥ Passwort“. Platzhalter: <code>{USERNAME}</code> <code>{PASSWORD}</code> <code>{TOTP}</code> <code>{TAB}</code> <code>{ENTER}</code> <code>{DELAY=500}</code>. Ein einzelner Eintrag kann das mit einem <code>autotype</code>-Feld in Bitwarden überschreiben.",
    "settings.vault.lock_screen": "Tresor: bei Bildschirmsperre sperren",
    "settings.vault.lock_screen.desc": "Sperrt den Tresor gleich mit, wenn du Windows sperrst (Win+L).",
    "settings.vault.idle": "Tresor: nach Inaktivität sperren",
    "settings.vault.idle.desc": "Sperrt den Tresor, wenn er so lange nicht benutzt wurde.",
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
    "settings.everything": "Everything übernimmt die Indizierung",
    "settings.everything.detected": "erkannt",
    "settings.everything.desc":
      "Everything hat deine Laufwerke ohnehin im Index — also fragt Funke einfach dort nach, statt selbst die Platte zu durchsuchen. Die Treffer sind sekundenaktuell, und nichts wird doppelt indiziert. Die Ordner oben grenzen die Suche weiterhin ein; für alle Laufwerke einfach <code>C:\\</code> hinzufügen. Schließt du Everything, nutzt Funke wieder seinen eigenen Index.",

    "settings.snippets.lead":
      "Text, den du ständig brauchst — Signatur, Adresse, ein Standardabsatz. Mit <kbd>s</kbd> finden, Enter tippt ihn dort ein, wo du herkamst.",
    "settings.snippets.placeholders": "Platzhalter",
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
      "Noch keine Snippets. Leg oben eines an und hol es dir mit <kbd>s</kbd> — oder direkt über seinen Namen aus der Suche.",

    "settings.plugins.lead":
      "Eigene Programme, die mitsuchen — geschrieben in jeder Sprache. Ordner mit einer <code>plugin.json</code> ins Plugin-Verzeichnis legen, dann Aktualisieren.",
    "settings.plugins.folder": "Plugin-Ordner",
    "settings.plugins.folder.desc":
      "Plugin hineinlegen, dann Aktualisieren — ganz ohne Neustart. Wie man eins schreibt, steht in docs/PLUGINS.md.",
    "settings.plugins.refresh": "Aktualisieren",
    "settings.plugins.open": "Ordner öffnen",
    "settings.plugins.empty":
      "Noch keine Plugins installiert. Schau unten in den Katalog — oder schreib dir selbst eins (<code>docs/PLUGINS.md</code>). Im Repo liegen Vorlagen (<code>funke-plugins/template</code> in Rust, <code>funke-plugins/template-python</code> in Python): bauen, samt <code>plugin.json</code> in den Ordner oben legen, Aktualisieren drücken und <kbd>tp hello</kbd> probieren.",
    "settings.plugins.suggested": "Empfohlene Plugins",
    "settings.plugins.suggested.desc":
      "Ein kuratierter Katalog aus dem Funke-Repo. Jeder Eintrag hängt an einer Prüfsumme — installiert wird also genau das, was auch geprüft wurde. Trotzdem bleibt ein Plugin ein Programm mit deinen Rechten, keine Sandbox: installier nur, was du kennst.",
    "settings.plugins.browse": "Katalog ansehen",
    "settings.plugins.loading": "Wird geladen…",
    "settings.plugins.install": "Installieren",
    "settings.plugins.installing": "Wird installiert…",
    "settings.plugins.installed": "Installiert",
    "settings.plugins.uninstall": "{name} deinstallieren",
    "settings.plugins.remove_confirm": "Entfernen?",
    "settings.plugins.removing": "Wird entfernt…",
    "settings.plugins.catalog_empty": "Noch nichts da",
    "settings.plugins.catalog_empty.desc": "Der Katalog ist leer — schreib das erste Plugin!",
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
