const { invoke } = window.__TAURI__.core;

const errorBox = document.getElementById("error");
const recorder = document.getElementById("recorder");

// Curated accents: terracotta first (the default), then equally muted warm-adjacent picks.
const ACCENTS = [
  ["#d97757", "Terracotta"],
  ["#d9a757", "Amber"],
  ["#8fa877", "Sage"],
  ["#7fa3c7", "Sky"],
  ["#b58ac9", "Orchid"],
];

const WIDTHS = [
  ["Compact", 600],
  ["Cozy", 680],
  ["Wide", 780],
];

// Minutes, and how to say them: the label is a key so the dropdown follows the language.
const IDLE_MINUTES = [
  [1, () => t("settings.idle.minute")],
  [5, () => t("settings.idle.minutes", { count: 5 })],
  [10, () => t("settings.idle.minutes", { count: 10 })],
  [15, () => t("settings.idle.minutes", { count: 15 })],
  [30, () => t("settings.idle.minutes", { count: 30 })],
  [60, () => t("settings.idle.hour")],
  [0, () => t("settings.idle.never")],
];

// `auto` first: it is the default, and the one most people should stay on. The language
// names are deliberately *not* translated — you look for "Deutsch" when you want German,
// whatever the UI currently says.
const LANGUAGES = [
  ["auto", () => t("settings.language.auto")],
  ["en", () => "English"],
  ["de", () => "Deutsch"],
];

// Where Funke lives. Everything the About pane links to hangs off the repository, so there
// is one URL to change if it ever moves.
const REPO = "https://github.com/klappstuhlpy/funke";

// Stroke glyphs in the sidebar's family — no vendor marks, no bitmaps.
const ICONS = {
  code: '<path d="M9.5 7 5 12l4.5 5M14.5 7l4.5 5-4.5 5" />',
  bug: '<circle cx="12" cy="12" r="8.5" /><path d="M12 7.8v5M12 15.9v.01" />',
  tag: '<path d="M12.6 4H20v7.4l-8 8a1.4 1.4 0 0 1-2 0l-5.4-5.4a1.4 1.4 0 0 1 0-2L12.6 4z" /><path d="M16.4 7.6v.01" />',
  list: '<path d="M5 6.5h14M5 12h14M5 17.5h9" />',
  book: '<path d="M12 6.6C10.6 5.2 8.6 4.5 5 4.5v13c3.6 0 5.6.7 7 2.1 1.4-1.4 3.4-2.1 7-2.1v-13c-3.6 0-5.6.7-7 2.1z" /><path d="M12 6.6v13" />',
  shield: '<path d="M12 3.6l7 2.9v5c0 4-2.9 7.1-7 8.9-4.1-1.8-7-4.9-7-8.9v-5l7-2.9z" />',
  scale: '<path d="M6 3.6h7l5 5v11.8H6z" /><path d="M13 3.6v5h5" /><path d="M9 13h6M9 16.5h4" />',
};

const LINKS = [
  ["settings.about.source", REPO, "code"],
  ["settings.about.issues", `${REPO}/issues`, "bug"],
  ["settings.about.releases", `${REPO}/releases`, "tag"],
  ["settings.about.changelog", `${REPO}/blob/main/CHANGELOG.md`, "list"],
  ["settings.about.design", `${REPO}/blob/main/docs/DESIGN.md`, "book"],
  ["settings.about.plugins", `${REPO}/blob/main/docs/PLUGINS.md`, "book"],
  ["settings.about.security", `${REPO}/blob/main/SECURITY.md`, "shield"],
  ["settings.about.license", `${REPO}/blob/main/LICENSE`, "scale"],
];

// The keys that work *inside* the overlay, next to the one that summons it — the hotkey pane
// is where you look for "what do I press", so both halves of the answer live there.
const SHORTCUTS = [
  ["Up+Down", "settings.shortcuts.navigate"],
  ["Enter", "settings.shortcuts.open"],
  ["Shift+Enter", "settings.shortcuts.alt"],
  ["Tab", "settings.shortcuts.actions"],
  ["Ctrl+1…9", "settings.shortcuts.nth"],
  ["Esc", "settings.shortcuts.dismiss"],
];

let settings = null;
// The recorder currently listening for keys — `{ el, apply }` — or null. One at a time: the
// summon recorder and every scope-hotkey recorder share the same machinery, and while one is
// armed, `renderAll` must not rebuild the element out from under it.
let recording = null;
// The id of the snippet the editor is editing, "" while creating a new one, null when the
// editor is closed. Snippets and quicklinks are the two settings with enough structure to
// need a form of their own.
let editingSnippet = null;
let editingQuicklink = null;

/* ── persistence: instant apply, revert on rejection ── */

async function save(patch) {
  const previous = structuredClone(settings);
  Object.assign(settings, patch);
  try {
    await invoke("save_settings", { settings });
    hideError();
  } catch (err) {
    settings = previous;
    showError(String(err));
  }
  // A language change repaints this window immediately — being told to restart to see your
  // own setting take effect is the kind of thing that makes a setting feel broken.
  if (patch.language !== undefined) await retranslate();
  renderAll();
}

/// Ask Rust which locale won (it resolves `auto` against Windows) and repaint every string.
async function retranslate() {
  setLocale(await invoke("locale"));
  applyTranslations();
  relabel(document.getElementById("vault-idle"), IDLE_MINUTES);
  relabel(document.getElementById("language"), LANGUAGES);
}

// The <option>s were filled in from the catalogue, so they need re-filling too — they are
// the one piece of text `applyTranslations` cannot reach from the markup.
function relabel(select, entries) {
  entries.forEach(([value, label], index) => {
    if (select.options[index]) select.options[index].textContent = label(value);
  });
}

function showError(message) {
  errorBox.textContent = message;
  errorBox.hidden = false;
}

function hideError() {
  errorBox.hidden = true;
}

/* ── accent is applied to this window too, so the change previews live ── */

function applyAccent(hex) {
  const match = /^#([0-9a-f]{6})$/i.exec(hex || "");
  if (!match) return;
  const n = parseInt(match[1], 16);
  const [r, g, b] = [n >> 16, (n >> 8) & 255, n & 255];
  const root = document.documentElement.style;
  root.setProperty("--accent", hex);
  root.setProperty("--accent-soft", `rgba(${r}, ${g}, ${b}, 0.15)`);
  root.setProperty("--accent-stroke", `rgba(${r}, ${g}, ${b}, 0.3)`);
}

/* ── rendering ── */

function renderAll() {
  applyAccent(settings.accent);

  const autostart = document.getElementById("autostart");
  autostart.setAttribute("aria-checked", String(settings.autostart));
  document.getElementById("update-auto").setAttribute("aria-checked", String(settings.update_check));

  document.getElementById("vault-hello").setAttribute("aria-checked", String(settings.vault_hello));
  document.getElementById("vault-icons").setAttribute("aria-checked", String(settings.vault_icons));
  document.getElementById("vault-guard").setAttribute("aria-checked", String(settings.vault_autotype_guard));
  document
    .getElementById("vault-autotype-enter")
    .setAttribute("aria-checked", String(settings.vault_autotype_enter));
  document.getElementById("vault-lock-screen").setAttribute("aria-checked", String(settings.vault_lock_on_screen_lock));
  document
    .getElementById("vault-capture-shield")
    .setAttribute("aria-checked", String(settings.vault_capture_shield));
  document
    .getElementById("vault-signed-cli")
    .setAttribute("aria-checked", String(settings.vault_require_signed_cli));
  document.getElementById("vault-context").setAttribute("aria-checked", String(settings.vault_context_suggest));

  const sequence = document.getElementById("vault-sequence");
  if (document.activeElement !== sequence) sequence.value = settings.vault_autotype_sequence;

  const idle = document.getElementById("vault-idle");
  if (idle.options.length) idle.value = String(settings.vault_idle_lock_minutes);

  document.querySelectorAll(".swatch").forEach((el) => {
    el.classList.toggle("active", el.dataset.accent === settings.accent);
  });
  document.querySelectorAll(".segment").forEach((el) => {
    el.classList.toggle("active", Number(el.dataset.width) === settings.overlay_width);
  });
  document.querySelectorAll(".toggle[data-provider]").forEach((el) => {
    const enabled = !settings.disabled_providers.includes(el.dataset.provider);
    el.setAttribute("aria-checked", String(enabled));
  });

  const engine = document.getElementById("engine");
  if (engine.options.length) engine.value = settings.web_engine;

  const language = document.getElementById("language");
  if (language.options.length) language.value = settings.language;

  renderRoots();
  renderSnippets();
  renderQuicklinks();
  // Both take their words from the catalogue, so they are rebuilt rather than translated in
  // place — a language change repaints them for free.
  renderShortcuts();
  renderLinks();

  // Both would replace the element an armed recorder is standing on, so neither runs while one
  // is listening for keys — a repaint mid-chord would eat the chord.
  if (!recording) {
    renderScopeHotkeys();
    showChord(settings.hotkey);
  }
}

/* ── a recorder shows a chord as keys, and prose as prose ── */

function showChord(chord) {
  fillKeycaps(recorder, chord);
}

function showTextIn(el, text) {
  el.classList.remove("chord");
  el.textContent = text;
}

function renderShortcuts() {
  const card = document.getElementById("shortcuts");
  card.replaceChildren(
    ...SHORTCUTS.map(([chord, key]) => {
      const row = pluginRow(t(key), "");
      row.querySelector(".desc").remove();
      row.appendChild(keycaps(chord));
      return row;
    }),
  );
}

function renderLinks() {
  const box = document.getElementById("links");
  box.replaceChildren(
    ...LINKS.map(([key, url, icon]) => {
      const chip = document.createElement("button");
      chip.className = "chip";
      chip.innerHTML = `<svg viewBox="0 0 24 24">${ICONS[icon]}</svg>`;
      chip.append(t(key));
      const arrow = document.createElement("span");
      arrow.className = "external";
      arrow.textContent = "↗";
      chip.appendChild(arrow);
      // The browser opens it, not the webview: these are the user's links, in the user's
      // browser, and the settings window is not a place to navigate away from.
      chip.addEventListener("click", () => invoke("open_url", { url }).catch((err) => showError(String(err))));
      return chip;
    }),
  );
}

function buildStaticControls() {
  const accents = document.getElementById("accents");
  ACCENTS.forEach(([hex, name]) => {
    const el = document.createElement("button");
    el.className = "swatch";
    el.dataset.accent = hex;
    el.title = name;
    el.style.background = hex;
    el.addEventListener("click", () => save({ accent: hex }));
    accents.appendChild(el);
  });

  const widths = document.getElementById("widths");
  WIDTHS.forEach(([name, px]) => {
    const el = document.createElement("button");
    el.className = "segment";
    el.dataset.width = px;
    el.textContent = name;
    el.title = `${px} px`;
    el.addEventListener("click", () => save({ overlay_width: px }));
    widths.appendChild(el);
  });

  document.getElementById("autostart").addEventListener("click", () => save({ autostart: !settings.autostart }));

  // Checking and installing are two presses, not one. The first tells you what is out
  // there and what changed in it; the second is you saying yes to a new program on your
  // machine. Collapsing them, as this used to, meant the "check" button silently installed.
  const checkUpdate = document.getElementById("check-update");
  const installUpdate = document.getElementById("install-update");
  const updateStatus = document.getElementById("update-status");
  const updateNotes = document.getElementById("update-notes");

  checkUpdate.addEventListener("click", async () => {
    checkUpdate.disabled = true;
    installUpdate.hidden = true;
    updateNotes.hidden = true;
    updateStatus.textContent = t("settings.updates.checking");
    try {
      const update = await invoke("check_update");
      if (update) {
        updateStatus.textContent = t("settings.updates.available", { version: update.version });
        // Release notes are somebody else's text, so they go in as text, never as markup.
        if (update.notes.trim()) {
          updateNotes.textContent = update.notes.trim();
          updateNotes.hidden = false;
        }
        installUpdate.hidden = false;
      } else {
        updateStatus.textContent = t("settings.updates.none");
      }
    } catch (err) {
      updateStatus.textContent = String(err);
    } finally {
      checkUpdate.disabled = false;
    }
  });

  installUpdate.addEventListener("click", async () => {
    installUpdate.disabled = true;
    checkUpdate.disabled = true;
    updateStatus.textContent = t("settings.updates.installing");
    try {
      // On success Funke is replaced and restarts, so there is no "done" to render here.
      await invoke("install_update");
    } catch (err) {
      updateStatus.textContent = String(err);
      installUpdate.disabled = false;
      checkUpdate.disabled = false;
    }
  });

  document.getElementById("update-auto").addEventListener("click", () => save({ update_check: !settings.update_check }));
  document.getElementById("vault-hello").addEventListener("click", () => save({ vault_hello: !settings.vault_hello }));
  document.getElementById("vault-icons").addEventListener("click", () => save({ vault_icons: !settings.vault_icons }));
  document
    .getElementById("vault-guard")
    .addEventListener("click", () => save({ vault_autotype_guard: !settings.vault_autotype_guard }));
  document
    .getElementById("vault-autotype-enter")
    .addEventListener("click", () => save({ vault_autotype_enter: !settings.vault_autotype_enter }));
  document
    .getElementById("vault-lock-screen")
    .addEventListener("click", () => save({ vault_lock_on_screen_lock: !settings.vault_lock_on_screen_lock }));
  document
    .getElementById("vault-capture-shield")
    .addEventListener("click", () => save({ vault_capture_shield: !settings.vault_capture_shield }));
  document
    .getElementById("vault-signed-cli")
    .addEventListener("click", () => save({ vault_require_signed_cli: !settings.vault_require_signed_cli }));
  document
    .getElementById("vault-context")
    .addEventListener("click", () => save({ vault_context_suggest: !settings.vault_context_suggest }));
  // Saved when the field is done being typed in, not on every keystroke.
  const sequence = document.getElementById("vault-sequence");
  sequence.addEventListener("change", () => save({ vault_autotype_sequence: sequence.value.trim() }));

  const idleSelect = document.getElementById("vault-idle");
  IDLE_MINUTES.forEach(([minutes, label]) => {
    const option = document.createElement("option");
    option.value = String(minutes);
    option.textContent = label();
    idleSelect.appendChild(option);
  });
  idleSelect.addEventListener("change", (e) => save({ vault_idle_lock_minutes: Number(e.target.value) }));

  const languageSelect = document.getElementById("language");
  LANGUAGES.forEach(([tag, label]) => {
    const option = document.createElement("option");
    option.value = tag;
    option.textContent = label();
    languageSelect.appendChild(option);
  });
  languageSelect.addEventListener("change", (e) => save({ language: e.target.value }));
  document.getElementById("engine").addEventListener("change", (e) => save({ web_engine: e.target.value }));
  document.getElementById("add-root").addEventListener("click", async () => {
    const picked = await invoke("pick_index_root");
    if (picked && !settings.index_roots.includes(picked)) {
      save({ index_roots: [...settings.index_roots, picked] });
    }
  });
  document.getElementById("open-plugins").addEventListener("click", () => invoke("open_plugins_folder"));
  document.getElementById("refresh-plugins").addEventListener("click", async () => {
    try {
      buildPluginRows(await invoke("reload_plugins"));
      renderAll();
      hideError();
    } catch (err) {
      showError(String(err));
    }
  });
  document.getElementById("browse-plugins").addEventListener("click", browseCatalog);
  buildSnippetControls();
  buildQuicklinkControls();
  buildScopeControls();
}

// The catalog is fetched over the network on demand, never at startup: opening Settings
// must not depend on GitHub being reachable.
async function browseCatalog() {
  const button = document.getElementById("browse-plugins");
  await withBusy(button, t("settings.plugins.loading"), async () => {
    const available = await invoke("browse_plugins");
    buildCatalogRows(available);
  });
}

// Run an async action with the button showing progress, surfacing failures in the error bar.
async function withBusy(button, busyLabel, action) {
  const label = button.textContent;
  button.textContent = busyLabel;
  button.disabled = true;
  try {
    await action();
    hideError();
  } catch (err) {
    showError(String(err));
  } finally {
    button.textContent = label;
    button.disabled = false;
  }
}

function buildCatalogRows(available) {
  const card = document.getElementById("catalog-list");
  card.innerHTML = "";
  card.hidden = false;
  if (!available.length) {
    card.appendChild(pluginRow(t("settings.plugins.catalog_empty"), t("settings.plugins.catalog_empty.desc")));
    return;
  }
  available.forEach((plugin) => {
    // An outdated copy says so where the version normally sits: "v1.0 → v1.2".
    const version = plugin.update
      ? `v${plugin.installed_version} → v${plugin.version}`
      : plugin.version && `v${plugin.version}`;
    const byline = [version, plugin.author && `by ${plugin.author}`].filter(Boolean).join(" · ");
    const row = pluginRow(
      plugin.prefix ? `${plugin.name} · ${plugin.prefix} <query>` : plugin.name,
      [plugin.description, byline].filter(Boolean).join(" — "),
    );

    const button = document.createElement("button");
    button.className = "button";
    if (plugin.update) {
      button.classList.add("primary");
      button.textContent = t("settings.plugins.update", { version: plugin.version });
      button.addEventListener("click", () =>
        withBusy(button, t("settings.plugins.updating"), async () => {
          buildPluginRows(await invoke("update_plugin", { id: plugin.id }));
          renderAll();
          await browseCatalog(); // re-fetch so this row settles back to "Installed"
        }),
      );
    } else if (plugin.installed) {
      button.textContent = t("settings.plugins.installed");
      button.disabled = true;
    } else {
      button.textContent = t("settings.plugins.install");
      button.addEventListener("click", () =>
        withBusy(button, t("settings.plugins.installing"), async () => {
          buildPluginRows(await invoke("install_plugin", { id: plugin.id }));
          renderAll();
          await browseCatalog(); // re-fetch so this row flips to "Installed"
        }),
      );
    }
    row.appendChild(button);
    card.appendChild(row);
  });
}

function pluginRow(labelText, descText) {
  const row = document.createElement("div");
  row.className = "row";
  const what = document.createElement("div");
  what.className = "what";
  const label = document.createElement("div");
  label.className = "label";
  label.textContent = labelText;
  what.appendChild(label);
  const desc = document.createElement("div");
  desc.className = "desc";
  desc.textContent = descText;
  what.appendChild(desc);
  row.appendChild(what);
  return row;
}

function buildPluginRows(plugins) {
  const card = document.getElementById("plugin-list");
  const empty = document.getElementById("plugins-empty");
  card.innerHTML = ""; // rebuilt from scratch (also on Refresh)
  card.hidden = plugins.length === 0;
  empty.hidden = plugins.length > 0;
  plugins.forEach((plugin) => {
    const row = pluginRow(
      plugin.prefix ? `${plugin.name} · ${plugin.prefix} <query>` : plugin.name,
      [plugin.version && `v${plugin.version}`, plugin.description].filter(Boolean).join(" — "),
    );

    const toggle = document.createElement("button");
    toggle.className = "toggle";
    toggle.role = "switch";
    toggle.dataset.provider = plugin.id; // rendered by renderAll like any provider toggle
    toggle.addEventListener("click", () => {
      const disabled = settings.disabled_providers.filter((id) => id !== plugin.id);
      if (!settings.disabled_providers.includes(plugin.id)) disabled.push(plugin.id);
      save({ disabled_providers: disabled });
    });
    row.appendChild(toggle);
    row.appendChild(uninstallButton(plugin));
    card.appendChild(row);
  });
}

// Deleting a plugin's folder is not undoable, so the ✕ arms first and acts on the second
// click — the same "armed, then confirm" idiom the overlay uses for destructive actions.
function uninstallButton(plugin) {
  const button = document.createElement("button");
  button.className = "remove";
  button.title = t("settings.plugins.uninstall", { name: plugin.name });
  button.textContent = "✕";
  let armed = null;

  // Back to a bare ✕: the width and the danger fill come off with the class.
  const disarm = () => {
    armed = null;
    button.classList.remove("armed");
    button.textContent = "✕";
  };

  button.addEventListener("click", async () => {
    if (!armed) {
      button.classList.add("armed");
      button.textContent = t("settings.plugins.remove_confirm");
      armed = setTimeout(disarm, 3000);
      return;
    }
    clearTimeout(armed);
    armed = null;
    button.disabled = true;
    button.textContent = t("settings.plugins.removing"); // still a word — stays armed, keeps the room
    try {
      buildPluginRows(await invoke("remove_plugin", { id: plugin.id })); // rebuilds this row away
      renderAll();
      hideError();
    } catch (err) {
      showError(String(err));
      button.disabled = false;
      disarm();
    }
  });
  return button;
}

function renderRoots() {
  const box = document.getElementById("roots");
  box.innerHTML = "";
  if (!settings.index_roots.length) {
    const row = document.createElement("div");
    row.className = "row";
    const what = document.createElement("div");
    what.className = "what";
    const desc = document.createElement("div");
    desc.className = "desc";
    // "Searching", not "indexing": with Everything running, Funke indexes nothing at all.
    desc.textContent = t("settings.roots.default");
    what.appendChild(desc);
    row.appendChild(what);
    box.appendChild(row);
    return;
  }
  settings.index_roots.forEach((root) => {
    const row = document.createElement("div");
    row.className = "row";

    const what = document.createElement("div");
    what.className = "what";
    const label = document.createElement("div");
    label.className = "label path";
    label.textContent = root;
    what.appendChild(label);
    row.appendChild(what);

    const remove = document.createElement("button");
    remove.className = "remove";
    remove.title = t("settings.roots.remove");
    remove.textContent = "✕";
    remove.addEventListener("click", () => {
      save({ index_roots: settings.index_roots.filter((existing) => existing !== root) });
    });
    row.appendChild(remove);
    box.appendChild(row);
  });
}

/* ── snippets ── */

const snippetEditor = document.getElementById("snippet-editor");
const snippetName = document.getElementById("snippet-name");
const snippetAbbr = document.getElementById("snippet-abbr");
const snippetContent = document.getElementById("snippet-content");

function buildSnippetControls() {
  document.getElementById("add-snippet").addEventListener("click", () => openSnippetEditor(null));
  document.getElementById("snippet-cancel").addEventListener("click", closeSnippetEditor);
  snippetEditor.addEventListener("submit", (e) => {
    e.preventDefault();
    commitSnippet();
  });
}

// `snippet` is the one being edited, or null to create. Editing keeps the id, so frecency
// and the snippet's place in the list survive a rename.
function openSnippetEditor(snippet) {
  editingSnippet = snippet ? snippet.id : "";
  snippetName.value = snippet ? snippet.name : "";
  snippetAbbr.value = snippet ? snippet.abbreviation : "";
  snippetContent.value = snippet ? snippet.content : "";
  document.getElementById("snippet-save").textContent = snippet
    ? t("settings.snippets.save")
    : t("settings.snippets.create");
  snippetEditor.hidden = false;
  snippetName.focus();
}

function closeSnippetEditor() {
  editingSnippet = null;
  snippetEditor.hidden = true;
}

function commitSnippet() {
  const name = snippetName.value.trim();
  const content = snippetContent.value;
  if (!name || !content.trim()) {
    showError(t("settings.snippets.incomplete"));
    return;
  }
  const edited = {
    // crypto.randomUUID keeps ids stable and unique without a counter to persist.
    id: editingSnippet || crypto.randomUUID(),
    name,
    abbreviation: snippetAbbr.value.trim(),
    content,
  };
  const snippets = editingSnippet
    ? settings.snippets.map((snippet) => (snippet.id === editingSnippet ? edited : snippet))
    : [...settings.snippets, edited];
  closeSnippetEditor();
  save({ snippets });
}

function renderSnippets() {
  const box = document.getElementById("snippet-list");
  const empty = document.getElementById("snippets-empty");
  box.innerHTML = "";
  box.hidden = settings.snippets.length === 0;
  empty.hidden = settings.snippets.length > 0;

  settings.snippets.forEach((snippet) => {
    const row = document.createElement("div");
    row.className = "row";

    const what = document.createElement("div");
    what.className = "what";
    const label = document.createElement("div");
    label.className = "label";
    label.textContent = snippet.name;
    if (snippet.abbreviation) {
      const tag = document.createElement("span");
      tag.className = "tag";
      tag.textContent = snippet.abbreviation;
      label.appendChild(tag);
    }
    const desc = document.createElement("div");
    desc.className = "desc";
    // One line: the body may be a paragraph, and the row is not the place to read it.
    desc.textContent = snippet.content.replace(/\s+/g, " ").trim().slice(0, 120);
    what.append(label, desc);
    row.appendChild(what);

    const edit = document.createElement("button");
    edit.className = "button";
    edit.textContent = t("settings.snippets.edit");
    edit.addEventListener("click", () => openSnippetEditor(snippet));

    const remove = document.createElement("button");
    remove.className = "remove";
    remove.title = t("settings.snippets.delete");
    remove.textContent = "✕";
    remove.addEventListener("click", () => {
      if (editingSnippet === snippet.id) closeSnippetEditor();
      save({ snippets: settings.snippets.filter((existing) => existing.id !== snippet.id) });
    });

    const group = document.createElement("div");
    group.className = "button-group";
    group.append(edit, remove);
    row.appendChild(group);
    box.appendChild(row);
  });
}

/* ── quicklinks ── */

const quicklinkEditor = document.getElementById("quicklink-editor");
const quicklinkName = document.getElementById("quicklink-name");
const quicklinkAbbr = document.getElementById("quicklink-abbr");
const quicklinkUrl = document.getElementById("quicklink-url");

function buildQuicklinkControls() {
  document.getElementById("add-quicklink").addEventListener("click", () => openQuicklinkEditor(null));
  document.getElementById("quicklink-cancel").addEventListener("click", closeQuicklinkEditor);
  quicklinkEditor.addEventListener("submit", (e) => {
    e.preventDefault();
    commitQuicklink();
  });
}

// `link` is the one being edited, or null to create. Editing keeps the id, so frecency and the
// link's place in the list survive a rename.
function openQuicklinkEditor(link) {
  editingQuicklink = link ? link.id : "";
  quicklinkName.value = link ? link.name : "";
  quicklinkAbbr.value = link ? link.abbreviation : "";
  quicklinkUrl.value = link ? link.url : "";
  document.getElementById("quicklink-save").textContent = link
    ? t("settings.quicklinks.save")
    : t("settings.quicklinks.create");
  quicklinkEditor.hidden = false;
  quicklinkName.focus();
}

function closeQuicklinkEditor() {
  editingQuicklink = null;
  quicklinkEditor.hidden = true;
}

function commitQuicklink() {
  const name = quicklinkName.value.trim();
  const url = quicklinkUrl.value.trim();
  if (!name || !url) {
    showError(t("settings.quicklinks.incomplete"));
    return;
  }
  // Rust refuses a non-http quicklink too, and that refusal is the one that counts — but it
  // arrives after the editor has closed and takes the typed URL with it. Saying so here keeps
  // the form open with the text still in it, which is where the mistake can actually be fixed.
  if (!/^https?:\/\//i.test(url)) {
    showError(t("settings.quicklinks.bad_url"));
    quicklinkUrl.focus();
    return;
  }
  const edited = {
    // crypto.randomUUID keeps ids stable and unique without a counter to persist.
    id: editingQuicklink || crypto.randomUUID(),
    name,
    abbreviation: quicklinkAbbr.value.trim(),
    url,
  };
  const quicklinks = editingQuicklink
    ? settings.quicklinks.map((link) => (link.id === editingQuicklink ? edited : link))
    : [...settings.quicklinks, edited];
  closeQuicklinkEditor();
  save({ quicklinks });
}

function renderQuicklinks() {
  const box = document.getElementById("quicklink-list");
  const empty = document.getElementById("quicklinks-empty");
  box.innerHTML = "";
  box.hidden = settings.quicklinks.length === 0;
  empty.hidden = settings.quicklinks.length > 0;

  settings.quicklinks.forEach((link) => {
    const row = document.createElement("div");
    row.className = "row";

    const what = document.createElement("div");
    what.className = "what";
    const label = document.createElement("div");
    label.className = "label";
    label.textContent = link.name;
    if (link.abbreviation) {
      const tag = document.createElement("span");
      tag.className = "tag";
      tag.textContent = link.abbreviation;
      label.appendChild(tag);
    }
    const desc = document.createElement("div");
    desc.className = "desc";
    desc.textContent = link.url;
    what.append(label, desc);
    row.appendChild(what);

    const edit = document.createElement("button");
    edit.className = "button";
    edit.textContent = t("settings.quicklinks.edit");
    edit.addEventListener("click", () => openQuicklinkEditor(link));

    const remove = document.createElement("button");
    remove.className = "remove";
    remove.title = t("settings.quicklinks.delete");
    remove.textContent = "✕";
    remove.addEventListener("click", () => {
      if (editingQuicklink === link.id) closeQuicklinkEditor();
      save({ quicklinks: settings.quicklinks.filter((existing) => existing.id !== link.id) });
    });

    const group = document.createElement("div");
    group.className = "button-group";
    group.append(edit, remove);
    row.appendChild(group);
    box.appendChild(row);
  });
}

function buildProviderRows(providers) {
  const card = document.getElementById("providers");
  providers.forEach((provider) => {
    const row = document.createElement("div");
    row.className = "row";

    const what = document.createElement("div");
    what.className = "what";
    const label = document.createElement("div");
    label.className = "label";
    label.textContent = provider.name;
    what.appendChild(label);
    // The keyword, where the source has one — written down here because this is the only
    // place it is written down at all.
    if (provider.prefix) {
      const desc = document.createElement("div");
      desc.className = "desc";
      desc.textContent = t("settings.providers.keyword", { prefix: provider.prefix });
      what.appendChild(desc);
    }
    row.appendChild(what);

    const toggle = document.createElement("button");
    toggle.className = "toggle";
    toggle.role = "switch";
    toggle.dataset.provider = provider.id;
    toggle.addEventListener("click", () => {
      const disabled = settings.disabled_providers.filter((id) => id !== provider.id);
      if (!settings.disabled_providers.includes(provider.id)) disabled.push(provider.id);
      save({ disabled_providers: disabled });
    });
    row.appendChild(toggle);
    card.appendChild(row);
  });
}

function buildEngineOptions(engines) {
  const select = document.getElementById("engine");
  engines.forEach((engine) => {
    const option = document.createElement("option");
    option.value = engine.id;
    option.textContent = engine.name;
    select.appendChild(option);
  });
}

/* ── sidebar navigation ── */

document.querySelectorAll(".nav").forEach((nav) => {
  nav.addEventListener("click", () => {
    document.querySelectorAll(".nav").forEach((el) => el.classList.toggle("active", el === nav));
    document.querySelectorAll(".pane").forEach((el) => {
      el.classList.toggle("active", el.id === `pane-${nav.dataset.pane}`);
    });
  });
});

/* ── scope hotkeys: a shortcut that opens the overlay already inside one source ── */

// [prefix, display name] for every source that has a keyword — the choices a scope hotkey has.
// Filled at boot from the same lists the Sources and Plugins panes use, so a newly installed
// plugin's keyword is bindable without anything here knowing it exists.
let scopeChoices = [];

function buildScopeChoices(providers, plugins) {
  scopeChoices = [...providers, ...plugins]
    .filter((source) => source.prefix)
    .map((source) => [source.prefix, source.name]);
}

function buildScopeControls() {
  document.getElementById("add-scope").addEventListener("click", () => {
    if (!scopeChoices.length) return;
    // A new row starts unbound: it registers nothing until it is given a chord, which is what
    // lets it exist on screen long enough to be configured.
    save({ scope_hotkeys: [...settings.scope_hotkeys, { hotkey: "", prefix: scopeChoices[0][0] }] });
  });
}

function saveScopes(scopes) {
  save({ scope_hotkeys: scopes });
}

function renderScopeHotkeys() {
  const box = document.getElementById("scope-list");
  box.innerHTML = "";
  box.hidden = settings.scope_hotkeys.length === 0;

  settings.scope_hotkeys.forEach((scope, index) => {
    const row = document.createElement("div");
    row.className = "row";

    const what = document.createElement("div");
    what.className = "what";
    const label = document.createElement("div");
    label.className = "label";
    label.textContent = t("settings.scopes.opens");
    const desc = document.createElement("div");
    desc.className = "desc";
    desc.textContent = t("settings.scopes.opens.desc", { prefix: scope.prefix });
    what.append(label, desc);
    row.appendChild(what);

    const select = document.createElement("select");
    scopeChoices.forEach(([prefix, name]) => {
      const option = document.createElement("option");
      option.value = prefix;
      option.textContent = `${name} · ${prefix}`;
      select.appendChild(option);
    });
    // A prefix from a source that has since been uninstalled would otherwise silently become
    // the first one in the list. Keep it, and let the user see what they bound.
    if (!scopeChoices.some(([prefix]) => prefix === scope.prefix)) {
      const orphan = document.createElement("option");
      orphan.value = scope.prefix;
      orphan.textContent = scope.prefix;
      select.appendChild(orphan);
    }
    select.value = scope.prefix;
    select.addEventListener("change", () => {
      const scopes = settings.scope_hotkeys.map((existing, at) =>
        at === index ? { ...existing, prefix: select.value } : existing,
      );
      saveScopes(scopes);
    });

    const chord = document.createElement("button");
    chord.className = "recorder compact";
    const paint = () => {
      if (scope.hotkey) fillKeycaps(chord, scope.hotkey);
      else showTextIn(chord, t("settings.scopes.unbound"));
    };
    paint();
    makeRecorder(
      chord,
      (captured) => {
        const scopes = settings.scope_hotkeys.map((existing, at) =>
          at === index ? { ...existing, hotkey: captured } : existing,
        );
        saveScopes(scopes);
      },
      paint,
    );

    const remove = document.createElement("button");
    remove.className = "remove";
    remove.title = t("settings.scopes.delete");
    remove.textContent = "✕";
    remove.addEventListener("click", () => {
      saveScopes(settings.scope_hotkeys.filter((_, at) => at !== index));
    });

    const group = document.createElement("div");
    group.className = "button-group";
    group.append(select, chord, remove);
    row.appendChild(group);
    box.appendChild(row);
  });
}

/* ── hotkey recorders ── */

// Turn a button into a chord recorder. `apply(chord)` is what to do with the captured keys —
// save the summon hotkey, or set one scope hotkey's chord. `restore()` puts the button back to
// what it showed before, for when nothing is captured after all.
function makeRecorder(el, apply, restore) {
  el.addEventListener("click", () => {
    if (recording) stopRecording();
    recording = { el, apply, restore };
    el.classList.add("recording");
    showTextIn(el, t("settings.hotkey.recording"));
    el.focus();
  });
  el.addEventListener("blur", () => {
    if (recording && recording.el === el) stopRecording();
  });
  el.addEventListener("keydown", captureChord);
}

function captureChord(e) {
  if (!recording) return;
  e.preventDefault();
  e.stopPropagation();
  const el = recording.el;
  if (e.key === "Escape") {
    stopRecording();
    return;
  }
  if (["Control", "Alt", "Shift", "Meta"].includes(e.key)) {
    const held = [e.ctrlKey && "Ctrl", e.altKey && "Alt", e.shiftKey && "Shift", e.metaKey && "Super"]
      .filter(Boolean)
      .join("+");
    // The modifiers already down, as caps, with the key still to come.
    if (held) {
      fillKeycaps(el, held);
      el.append("…");
    } else {
      showTextIn(el, t("settings.hotkey.recording"));
    }
    return;
  }
  const key = keyName(e);
  const mods = [e.ctrlKey && "Ctrl", e.altKey && "Alt", e.shiftKey && "Shift", e.metaKey && "Super"].filter(Boolean);
  if (!key || !mods.length) {
    showTextIn(el, t("settings.hotkey.needs_modifier"));
    return;
  }
  // Disarm *before* applying: `apply` saves, which repaints — and a repaint would replace the
  // element this handler is still standing on.
  const { apply } = recording;
  el.classList.remove("recording");
  recording = null;
  apply([...mods, key].join("+"));
}

function keyName(e) {
  const c = e.code;
  if (c.startsWith("Key")) return c.slice(3);
  if (c.startsWith("Digit")) return c.slice(5);
  if (/^F\d{1,2}$/.test(c)) return c;
  const named = {
    Space: "Space",
    Enter: "Enter",
    Tab: "Tab",
    Backspace: "Backspace",
    Delete: "Delete",
    Home: "Home",
    End: "End",
    PageUp: "PageUp",
    PageDown: "PageDown",
    ArrowUp: "Up",
    ArrowDown: "Down",
    ArrowLeft: "Left",
    ArrowRight: "Right",
    Comma: "Comma",
    Period: "Period",
    Minus: "Minus",
    Equal: "Equal",
    Slash: "Slash",
    Backslash: "Backslash",
    Semicolon: "Semicolon",
    Quote: "Quote",
    Backquote: "Backquote",
    BracketLeft: "BracketLeft",
    BracketRight: "BracketRight",
  };
  return named[c] || null;
}

// Cancelled (Escape, or clicked away). Nothing was captured, so put the button back to what it
// showed — and *only* that button. A full `renderAll()` here would rebuild the scope-hotkey
// rows, and clicking straight from one recorder to the next fires this from the first one's
// blur: the element the click is on its way to would be destroyed underneath it, leaving a
// recorder that is armed, detached, and impossible to type into.
function stopRecording() {
  if (!recording) return;
  const { el, restore } = recording;
  el.classList.remove("recording");
  recording = null;
  restore();
}

makeRecorder(
  recorder,
  (chord) => save({ hotkey: chord }),
  () => showChord(settings.hotkey),
);

/* ── window chrome ── */

document.getElementById("close").addEventListener("click", () => invoke("close_settings"));

document.addEventListener("keydown", (e) => {
  if (e.key === "Escape" && !recording) invoke("close_settings");
});

/* ── boot ── */

async function init() {
  try {
    const [loaded, engines, providers, plugins, everything, tag] = await Promise.all([
      invoke("get_settings"),
      invoke("list_engines"),
      invoke("list_providers"),
      invoke("list_plugins"),
      invoke("everything_is_indexing"),
      invoke("locale"),
    ]);
    settings = loaded;
    // Before a single control is built: they take their labels from the catalogue.
    setLocale(tag);
    applyTranslations();
    // Which index the Files pane is describing is a fact about right now — Everything can be
    // started or closed while Funke runs, so it is read when the pane opens, not cached.
    document.getElementById("everything-row").hidden = !everything;
    // Version is inferred from funke-app's Cargo.toml at build time (single source of truth).
    window.__TAURI__.app
      .getVersion()
      .then((v) => {
        document.getElementById("version").textContent = `Funke ${v}`;
        document.getElementById("about-version").textContent = `v${v}`;
      })
      .catch(() => {});
    buildStaticControls();
    buildEngineOptions(engines);
    buildProviderRows(providers);
    buildPluginRows(plugins);
    buildScopeChoices(providers, plugins);
    renderAll();
  } catch (err) {
    // A half-built pane the user can see and close beats a window that silently never
    // appears: the window is created hidden and only this call reveals it, so anything
    // thrown on the way here would strand it invisible forever.
    showError(t("settings.load_failed", { error: String(err) }));
  } finally {
    // Painted and styled — the window may show itself now (created hidden).
    requestAnimationFrame(() => invoke("settings_ready"));
  }
}

init();
