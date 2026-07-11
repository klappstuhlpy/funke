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

let settings = null;
let recording = false;
// The id of the snippet the editor is editing, "" while creating a new one, null when the
// editor is closed. Snippets are the one setting with enough structure to need a form.
let editingSnippet = null;

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

  document.getElementById("vault-hello").setAttribute("aria-checked", String(settings.vault_hello));
  document.getElementById("vault-icons").setAttribute("aria-checked", String(settings.vault_icons));
  document
    .getElementById("vault-autotype-enter")
    .setAttribute("aria-checked", String(settings.vault_autotype_enter));
  document.getElementById("vault-lock-screen").setAttribute("aria-checked", String(settings.vault_lock_on_screen_lock));
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

  if (!recording) recorder.textContent = settings.hotkey;
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

  const checkUpdate = document.getElementById("check-update");
  checkUpdate.addEventListener("click", async () => {
    const status = document.getElementById("update-status");
    checkUpdate.disabled = true;
    status.textContent = t("settings.updates.checking");
    try {
      status.textContent = await invoke("check_update");
    } catch (err) {
      status.textContent = String(err);
    } finally {
      checkUpdate.disabled = false;
    }
  });
  document.getElementById("vault-hello").addEventListener("click", () => save({ vault_hello: !settings.vault_hello }));
  document.getElementById("vault-icons").addEventListener("click", () => save({ vault_icons: !settings.vault_icons }));
  document
    .getElementById("vault-autotype-enter")
    .addEventListener("click", () => save({ vault_autotype_enter: !settings.vault_autotype_enter }));
  document
    .getElementById("vault-lock-screen")
    .addEventListener("click", () => save({ vault_lock_on_screen_lock: !settings.vault_lock_on_screen_lock }));
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
    const byline = [plugin.version && `v${plugin.version}`, plugin.author && `by ${plugin.author}`]
      .filter(Boolean)
      .join(" · ");
    const row = pluginRow(
      plugin.prefix ? `${plugin.name} · ${plugin.prefix} <query>` : plugin.name,
      [plugin.description, byline].filter(Boolean).join(" — "),
    );

    const button = document.createElement("button");
    button.className = "button";
    if (plugin.installed) {
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
  button.addEventListener("click", async () => {
    if (!armed) {
      button.textContent = t("settings.plugins.remove_confirm");
      button.style.width = "auto";
      armed = setTimeout(() => {
        armed = null;
        button.textContent = "✕";
        button.style.width = "";
      }, 3000);
      return;
    }
    clearTimeout(armed);
    armed = null;
    button.disabled = true;
    button.textContent = t("settings.plugins.removing");
    try {
      buildPluginRows(await invoke("remove_plugin", { id: plugin.id })); // rebuilds this row away
      renderAll();
      hideError();
    } catch (err) {
      showError(String(err));
      button.disabled = false;
      button.textContent = "✕";
      button.style.width = "";
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

/* ── hotkey recorder ── */

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

function stopRecording() {
  recording = false;
  recorder.classList.remove("recording");
  renderAll();
}

recorder.addEventListener("click", () => {
  recording = true;
  recorder.classList.add("recording");
  recorder.textContent = t("settings.hotkey.recording");
  recorder.focus();
});

recorder.addEventListener("blur", () => {
  if (recording) stopRecording();
});

recorder.addEventListener("keydown", (e) => {
  if (!recording) return;
  e.preventDefault();
  e.stopPropagation();
  if (e.key === "Escape") {
    stopRecording();
    return;
  }
  if (["Control", "Alt", "Shift", "Meta"].includes(e.key)) {
    const held = [e.ctrlKey && "Ctrl", e.altKey && "Alt", e.shiftKey && "Shift", e.metaKey && "Super"]
      .filter(Boolean)
      .join("+");
    recorder.textContent = held ? `${held}+…` : t("settings.hotkey.recording");
    return;
  }
  const key = keyName(e);
  const mods = [e.ctrlKey && "Ctrl", e.altKey && "Alt", e.shiftKey && "Shift", e.metaKey && "Super"].filter(Boolean);
  if (!key || !mods.length) {
    recorder.textContent = t("settings.hotkey.needs_modifier");
    return;
  }
  stopRecording();
  save({ hotkey: [...mods, key].join("+") });
});

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
      .then((v) => (document.getElementById("version").textContent = `Funke ${v}`))
      .catch(() => {});
    buildStaticControls();
    buildEngineOptions(engines);
    buildProviderRows(providers);
    buildPluginRows(plugins);
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
