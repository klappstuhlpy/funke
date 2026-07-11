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

  // German. Sie-form, and the launcher's own vocabulary is kept where translating it would
  // only obscure it: "Hotkey", "Plugin", "Overlay" are what a German user calls these too.
  de: {
    "overlay.placeholder": "Suchen…",
    "overlay.master_password": "Master-Passwort…",
    "overlay.keys.navigate": "navigieren",
    "overlay.keys.open": "öffnen",
    "overlay.keys.actions": "Aktionen",
    "overlay.keys.dismiss": "schließen",
    "overlay.actions": "Aktionen",
    "overlay.no_results": "Keine Treffer",
    "overlay.confirm": "Zum Bestätigen erneut Enter drücken — Esc bricht ab",
    "overlay.result": "1 Treffer",
    "overlay.results": "{count} Treffer",
    "overlay.vault": "Bitwarden-Tresor",
    "overlay.recent": "Zuletzt verwendet",
    "overlay.suggested": "Vorschlag",
    "overlay.suggested_for": "Für {app}",
    "overlay.greeting.night": "Gute Nacht",
    "overlay.greeting.morning": "Guten Morgen",
    "overlay.greeting.afternoon": "Guten Tag",
    "overlay.greeting.evening": "Guten Abend",
    "overlay.uptime.hours": "seit {hours} Std. {minutes} Min.",
    "overlay.uptime.minutes": "seit {minutes} Min.",
    "overlay.tip.search": "Tippen, um Anwendungen, Dateien und Fenster zu durchsuchen",
    "overlay.tip.prefixes": "f <Suche> nur Dateien, w Fenster, g das Web, v Ihren Tresor",
    "overlay.tip.clipboard": "c zeigt Kopiertes — Enter fügt es in das Fenster ein, aus dem Sie kamen",
    "overlay.tip.actions": "Tab zeigt alle Aktionen des gewählten Treffers",

    "settings.title": "Einstellungen",
    "settings.close": "Schließen (Esc)",
    "settings.nav.general": "Allgemein",
    "settings.nav.appearance": "Aussehen",
    "settings.nav.hotkey": "Hotkey",
    "settings.nav.commands": "Befehle",
    "settings.nav.snippets": "Textbausteine",
    "settings.nav.plugins": "Plugins",
    "settings.load_failed": "Die Einstellungen wurden nicht vollständig geladen: {error}",

    "settings.general.heading": "Allgemein",
    "settings.general.lead": "Wie Funke auf diesem Rechner lebt.",
    "settings.autostart": "Beim Anmelden starten",
    "settings.autostart.desc": "Funke startet im Infobereich, sobald Sie sich bei Windows anmelden.",
    "settings.language": "Sprache",
    "settings.language.desc": "Folgt Windows, solange Sie nichts auswählen. Treffer und Menüs wechseln mit.",
    "settings.language.auto": "Windows folgen",
    "settings.updates": "Updates",
    "settings.updates.desc": "Auf GitHub nach einer neueren Version sehen.",
    "settings.updates.check": "Nach Updates suchen",
    "settings.updates.checking": "Wird geprüft…",

    "settings.appearance.heading": "Aussehen",
    "settings.appearance.lead": "Das Overlay bleibt echtes Glas — dies stimmt seinen Charakter ab.",
    "settings.accent": "Akzentfarbe",
    "settings.accent.desc": "Für Auswahl, Cursor und Tastenhinweise.",
    "settings.width": "Breite des Overlays",
    "settings.width.desc": "Wie breit die Suchleiste ist, in jedem Modus.",

    "settings.hotkey.lead": "Das globale Tastenkürzel, das das Overlay aufruft.",
    "settings.hotkey.label": "Funke aufrufen",
    "settings.hotkey.desc": "Klicken, dann die neue Kombination drücken. Gilt sofort.",
    "settings.hotkey.fineprint":
      "Gehört die Kombination bereits einer anderen Anwendung (auch PowerToys Run mag <kbd>Strg</kbd>+<kbd>Leertaste</kbd>), schlägt die Belegung fehl und der bisherige Hotkey bleibt aktiv.",
    "settings.hotkey.recording": "Tasten drücken…",
    "settings.hotkey.needs_modifier": "Zusatztaste hinzufügen…",

    "settings.commands.heading": "Befehle",
    "settings.commands.lead": "Welche eingebauten Anbieter Ihre Suchen beantworten.",
    "settings.engine": "Web-Suchmaschine",
    "settings.engine.desc": "Wohin „Im Web nach … suchen“ Sie schickt.",
    "settings.vault.hello": "Tresor: mit Windows Hello entsperren",
    "settings.vault.hello.desc":
      "Nach einmaligem Entsperren mit dem Master-Passwort genügt später Windows Hello. Dafür wird ein von Windows geschützter Sitzungsschlüssel gespeichert — die Abwägung steht in SECURITY.md.",
    "settings.vault.icons": "Tresor: Website-Symbole",
    "settings.vault.icons.desc":
      "Favicons auf Tresor-Einträgen anzeigen, geladen vom Icon-Dienst Ihres Bitwarden-Servers (er erfährt dadurch, nach welchen Seiten Sie suchen).",
    "settings.vault.context": "Tresor: Vorschlag für die Anwendung im Vordergrund",
    "settings.vault.context.desc":
      "Rufen Sie Funke über Discord (oder einem GitHub-Tab) auf, bietet das leere Overlay die passenden Zugangsdaten an — erkannt an Prozess, Fenstertitel und der Adresszeile des Browsers. Aus heißt: Tresor-Einträge erscheinen nur hinter <code>v</code>.",
    "settings.vault.autotype_enter": "Tresor: Autotype drückt Enter",
    "settings.vault.autotype_enter.desc":
      "Das Formular nach dem Passwort automatisch abschicken. Aus getippt Funke nur die Zugangsdaten und überlässt Enter Ihnen. Gilt nur für die eingebaute Sequenz — eine eigene unten wird exakt so getippt, wie sie dasteht.",
    "settings.vault.sequence": "Tresor: Autotype-Sequenz",
    "settings.vault.sequence.desc":
      "Was Autotype tippt, wenn nicht das übliche Benutzername ⇥ Passwort. Platzhalter: <code>{USERNAME}</code> <code>{PASSWORD}</code> <code>{TOTP}</code> <code>{TAB}</code> <code>{ENTER}</code> <code>{DELAY=500}</code>. Ein einzelner Eintrag kann dies mit einem <code>autotype</code>-Feld in Bitwarden überschreiben.",
    "settings.vault.lock_screen": "Tresor: bei Bildschirmsperre sperren",
    "settings.vault.lock_screen.desc": "Den Tresor sofort sperren, wenn Sie Windows sperren (Win+L).",
    "settings.vault.idle": "Tresor: nach Untätigkeit sperren",
    "settings.vault.idle.desc": "Den Tresor nach dieser Zeit ohne Nutzung sperren.",
    "settings.idle.minutes": "{count} Minuten",
    "settings.idle.minute": "1 Minute",
    "settings.idle.hour": "1 Stunde",
    "settings.idle.never": "Nie",

    "settings.roots": "Ordner für den Dateiindex",
    "settings.roots.desc":
      "Was die Dateisuche abdeckt. Leer bedeutet Ihren Benutzerordner; Änderungen greifen binnen Sekunden.",
    "settings.roots.add": "Ordner hinzufügen",
    "settings.roots.default": "Es wird Ihr Benutzerordner durchsucht (Standard).",
    "settings.roots.remove": "Ordner entfernen",
    "settings.everything": "Everything übernimmt die Indizierung",
    "settings.everything.detected": "erkannt",
    "settings.everything.desc":
      "Everything hält einen laufenden Index Ihrer Laufwerke bereit, also fragt Funke ihn, statt selbst die Festplatte zu durchlaufen — die Treffer sind sekundenaktuell und nichts wird doppelt indiziert. Die Ordner oben grenzen die Suche weiterhin ein; <code>C:\\</code> hinzufügen, um alle Laufwerke zu durchsuchen. Schließen Sie Everything, nutzt Funke wieder den eigenen Index.",

    "settings.snippets.lead":
      "Text, den Sie oft einfügen — eine Signatur, eine Adresse, ein Textblock. Mit <kbd>s</kbd> finden, Enter tippt ihn in das Fenster, aus dem Sie kamen.",
    "settings.snippets.placeholders": "Platzhalter",
    "settings.snippets.placeholders.desc":
      "Werden beim Einfügen aufgelöst, nicht beim Speichern: <code>{DATE}</code>, <code>{TIME}</code>, <code>{DATETIME}</code> (oder Ihr eigenes Format, <code>{DATE:%d.%m.%Y}</code>), <code>{CLIPBOARD}</code> für das zuletzt Kopierte und <code>{CURSOR}</code> für die Stelle, an der der Cursor landen soll. Alles andere in geschweiften Klammern wird genau so getippt, wie es dasteht.",
    "settings.snippets.new": "Neuer Textbaustein",
    "settings.snippets.name": "Name",
    "settings.snippets.abbr": "Kürzel",
    "settings.snippets.optional": "optional",
    "settings.snippets.content": "Inhalt",
    "settings.snippets.save": "Änderungen speichern",
    "settings.snippets.create": "Textbaustein anlegen",
    "settings.snippets.cancel": "Abbrechen",
    "settings.snippets.edit": "Bearbeiten",
    "settings.snippets.delete": "Textbaustein löschen",
    "settings.snippets.incomplete": "Ein Textbaustein braucht einen Namen und einen Inhalt.",
    "settings.snippets.empty":
      "Noch keine Textbausteine. Legen Sie oben einen an und erreichen Sie ihn mit <kbd>s</kbd> — oder über den Namen, den Sie ihm gegeben haben, direkt aus der Suche.",

    "settings.plugins.lead":
      "Anbieter als eigene Programme, in jeder Sprache — einen Ordner mit einer <code>plugin.json</code> in das Plugin-Verzeichnis legen, dann Aktualisieren.",
    "settings.plugins.folder": "Plugin-Ordner",
    "settings.plugins.folder.desc":
      "Plugin hineinlegen, dann Aktualisieren — ohne Neustart. Wie man eines schreibt, steht in docs/PLUGINS.md.",
    "settings.plugins.refresh": "Aktualisieren",
    "settings.plugins.open": "Ordner öffnen",
    "settings.plugins.empty":
      "Noch keine Plugins installiert. Sehen Sie sich unten den Katalog an oder schreiben Sie eines mit <code>docs/PLUGINS.md</code> — das Repository bringt Vorlagen mit (<code>funke-plugins/template</code> in Rust, <code>funke-plugins/template-python</code> in Python): eines neben seine <code>plugin.json</code> in den Ordner oben bauen, Aktualisieren drücken, dann <kbd>tp hello</kbd> probieren.",
    "settings.plugins.suggested": "Empfohlene Plugins",
    "settings.plugins.suggested.desc":
      "Ein kuratierter Katalog aus dem Funke-Repository. Jeder Eintrag ist auf eine Prüfsumme festgelegt — installiert wird also genau das, was geprüft wurde. Ein Plugin bleibt trotzdem ein Programm mit Ihren Rechten, keine Sandbox. Installieren Sie nur, was Sie kennen.",
    "settings.plugins.browse": "Durchsuchen",
    "settings.plugins.loading": "Wird geladen…",
    "settings.plugins.install": "Installieren",
    "settings.plugins.installing": "Wird installiert…",
    "settings.plugins.installed": "Installiert",
    "settings.plugins.uninstall": "{name} deinstallieren",
    "settings.plugins.remove_confirm": "Entfernen?",
    "settings.plugins.removing": "Wird entfernt…",
    "settings.plugins.catalog_empty": "Noch nichts hier",
    "settings.plugins.catalog_empty.desc": "Der Katalog ist leer — schreiben Sie das erste Plugin!",
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
