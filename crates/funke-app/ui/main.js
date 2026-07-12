const { invoke } = window.__TAURI__.core;
const { listen } = window.__TAURI__.event;

const panel = document.getElementById("panel");
const input = document.getElementById("query");
const context = document.getElementById("context");
const list = document.getElementById("results");
const footer = document.getElementById("status");
const count = document.getElementById("count");

// Keys, not text: the tips are re-read on every render, so they follow a language change
// without the overlay being reopened.
const TIPS = ["overlay.tip.search", "overlay.tip.prefixes", "overlay.tip.clipboard", "overlay.tip.actions"];

let items = [];
let sections = []; // results mode: [{ label, items }] — `items` stays the flat list for navigation
// overview mode: the same shape, plus whether its rows can be removed from recents
// (credential suggestions can't — they aren't stored anywhere). Headings appear only
// once there's a suggestion to explain; a lone "Recent" over the only group is noise.
let groups = [];
let overviewLabels = false;
let selected = 0;
let mode = "overview"; // "overview" (empty input) | "results"

// Actions menu (Tab): the item whose actions are listed, the highlighted action, and
// whether the highlighted action is armed and waiting for a confirming Enter.
let actionsFor = null;
let actionSelected = 0;
let confirming = false;

// Vault unlock prompt: the input becomes a masked master-password field; the query it
// replaced is restored (and re-run) after a successful unlock.
let vaultPrompt = false;
let vaultReturnQuery = "";
let unlocking = false;

// Every path that paints the list takes a ticket first. The ones that await the backend
// (overview, search) check it again afterwards and drop their result if something else has
// claimed the screen since — a blocked autotype arrives *while* the re-summoned overlay is
// still loading its overview, and the overview must not paint over the warning.
let paintToken = 0;

// A blocked autotype is on screen (see showBlocked): the warning holds the list until the
// user answers it — by overriding, by picking a copy, by typing, or by leaving.
let blocked = false;

// The window is sized by its content: report the panel height after every render.
function resize() {
  invoke("resize_overlay", { height: Math.ceil(panel.getBoundingClientRect().height) });
}

// The accent token family follows settings; everything else in the palette is fixed.
function applyAccent(settings) {
  const match = /^#([0-9a-f]{6})$/i.exec(settings.accent || "");
  if (!match) return;
  const n = parseInt(match[1], 16);
  const [r, g, b] = [n >> 16, (n >> 8) & 255, n & 255];
  const root = document.documentElement.style;
  root.setProperty("--accent", settings.accent);
  root.setProperty("--accent-soft", `rgba(${r}, ${g}, ${b}, 0.15)`);
  root.setProperty("--accent-stroke", `rgba(${r}, ${g}, ${b}, 0.3)`);
}

function greeting() {
  const h = new Date().getHours();
  if (h < 5) return t("overlay.greeting.night");
  if (h < 12) return t("overlay.greeting.morning");
  if (h < 18) return t("overlay.greeting.afternoon");
  return t("overlay.greeting.evening");
}

function formatUptime(secs) {
  const h = Math.floor(secs / 3600);
  const m = Math.floor((secs % 3600) / 60);
  return h ? t("overlay.uptime.hours", { hours: h, minutes: m }) : t("overlay.uptime.minutes", { minutes: m });
}

function iconFor(item) {
  if (item.icon) {
    const img = document.createElement("img");
    img.className = "icon";
    img.src = item.icon;
    img.alt = "";
    return img;
  }
  const div = document.createElement("div");
  div.className = "icon monogram";
  div.textContent = (item.title[0] || "?").toUpperCase();
  return div;
}

function itemRow(item, index, { removable = false } = {}) {
  const li = document.createElement("li");
  li.className = "item" + (index === selected ? " selected" : "");
  li.appendChild(iconFor(item));

  const text = document.createElement("div");
  text.className = "text";
  const title = document.createElement("div");
  title.className = "title";
  title.textContent = item.title;
  text.appendChild(title);
  if (item.subtitle) {
    const sub = document.createElement("div");
    sub.className = "subtitle";
    sub.textContent = item.subtitle;
    text.appendChild(sub);
  }
  li.appendChild(text);

  const hint = keycaps("Enter");
  hint.classList.add("hint");
  li.appendChild(hint);

  if (removable && !actionsFor && index >= 0) {
    // Recents are removable: the ✕ deletes the entry without running it.
    const remove = document.createElement("button");
    remove.className = "remove";
    remove.title = t("overlay.forget");
    remove.innerHTML =
      '<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.4" stroke-linecap="round">' +
      '<path d="M6 6l12 12M18 6L6 18"/></svg>';
    remove.addEventListener("click", (e) => {
      e.stopPropagation();
      // Call it with no argument: a `()` command resolves to *null*, and passing that
      // straight in as the options object would throw on destructuring (`= {}` only
      // defaults away `undefined`), leaving the row on screen until the next open.
      invoke("remove_recent", { id: item.id }).then(() => loadOverview());
    });
    li.appendChild(remove);
  }

  li.addEventListener("click", () => maybeRun(item, 0));
  return li;
}

function actionRow(item, named, index) {
  const li = document.createElement("li");
  const armed = confirming && index === actionSelected;
  li.className =
    "item action" + (index === actionSelected ? " selected" : "") + (named.confirm ? " danger" : "") + (armed ? " armed" : "");

  const text = document.createElement("div");
  text.className = "text";
  const title = document.createElement("div");
  title.className = "title";
  title.textContent = named.label;
  text.appendChild(title);
  if (armed) {
    const sub = document.createElement("div");
    sub.className = "subtitle";
    sub.textContent = t("overlay.confirm");
    text.appendChild(sub);
  }
  li.appendChild(text);

  // Every action has a shortcut: Enter / Shift+Enter for the first two, Ctrl+digit
  // beyond (Ctrl+1/2 also work, but the Enter forms are the memorable labels).
  const chord = index === 0 ? "Enter" : index === 1 ? "Shift+Enter" : index < 9 ? `Ctrl+${index + 1}` : "";
  const hint = keycaps(chord);
  hint.classList.add("hint");
  if (!chord) hint.style.visibility = "hidden";
  li.appendChild(hint);

  li.addEventListener("click", () => maybeRun(item, index, { fromMenu: true }));
  return li;
}

function render() {
  list.innerHTML = "";

  if (actionsFor) {
    // Actions menu: the item pinned for context, then one row per action.
    context.hidden = true;
    footer.hidden = false;
    count.textContent = actionsFor.title;

    const pinned = itemRow(actionsFor, -1);
    pinned.classList.add("pinned");
    list.appendChild(pinned);

    const label = document.createElement("li");
    label.className = "group";
    label.textContent = t("overlay.actions");
    list.appendChild(label);

    actionsFor.actions.forEach((named, i) => list.appendChild(actionRow(actionsFor, named, i)));
    resize();
    return;
  }

  // The standing "Recent" strip is the heading for an *unlabelled* overview — the one group
  // it could possibly be about. The moment a suggestion arrives, the groups grow their own
  // headings ("For github.com", then "Recent"), and the strip becomes a second heading above
  // the first: it names the recents while sitting over the credential.
  context.hidden = !(mode === "overview" && items.length > 0 && !overviewLabels);
  footer.hidden = mode === "results" && items.length === 0;

  if (!items.length) {
    if (mode === "overview") {
      TIPS.forEach((tip) => {
        const li = document.createElement("li");
        li.className = "tip";
        li.textContent = t(tip);
        list.appendChild(li);
      });
    } else if (input.value.trim()) {
      const li = document.createElement("li");
      li.className = "empty";
      li.textContent = t("overlay.no_results");
      list.appendChild(li);
    }
    resize();
    return;
  }

  // Sectioned in both modes: label + rows per group; `index` keeps navigation flat
  // across groups.
  const rendered = mode === "results" ? sections : groups;
  const labelled = mode === "results" || overviewLabels;
  let index = 0;
  rendered.forEach((section) => {
    if (labelled) {
      const label = document.createElement("li");
      label.className = "group";
      label.textContent = section.label;
      list.appendChild(label);
    }
    section.items.forEach((item) => {
      list.appendChild(itemRow(item, index, { removable: section.removable }));
      index += 1;
    });
  });

  const current = list.querySelectorAll(".item")[selected];
  if (current) current.scrollIntoView({ block: "nearest" });
  resize();
}

function closeActions() {
  actionsFor = null;
  actionSelected = 0;
  confirming = false;
}

/* ── vault unlock prompt ── */

function enterVaultPrompt() {
  vaultPrompt = true;
  vaultReturnQuery = input.value;
  closeActions();
  input.value = "";
  input.type = "password";
  input.placeholder = t("overlay.master_password");
  input.focus();
  renderVaultPrompt(null);
}

function exitVaultPrompt(restoreQuery) {
  vaultPrompt = false;
  unlocking = false;
  input.value = "";
  input.type = "text";
  input.placeholder = t("overlay.placeholder");
  input.value = restoreQuery ? vaultReturnQuery : "";
  vaultReturnQuery = "";
  input.focus();
  search();
}

function renderVaultPrompt(error) {
  list.innerHTML = "";
  context.hidden = true;
  footer.hidden = false;
  count.textContent = t("overlay.vault");

  const tip = document.createElement("li");
  tip.className = "tip";
  tip.textContent = unlocking ? t("overlay.vault.unlocking") : t("overlay.vault.prompt");
  list.appendChild(tip);

  if (error) {
    const row = document.createElement("li");
    row.className = "empty vault-error";
    row.textContent = error;
    list.appendChild(row);
  }
  resize();
}

async function submitVaultPassword() {
  if (unlocking) return;
  const password = input.value;
  if (!password) return;
  unlocking = true;
  input.value = "";
  renderVaultPrompt(null);
  try {
    await invoke("vault_unlock", { password });
    exitVaultPrompt(true); // restores and re-runs the original `v …` query
  } catch (e) {
    unlocking = false;
    renderVaultPrompt(String(e));
  }
}

// The empty state: credentials for the app you came from (the vault's context
// suggestions — or its unlock row, when it's locked and can't know yet), then recents.
// `keepSelection` is for in-place refreshes (the context or a favicon arriving late).
async function loadOverview({ keepSelection = false } = {}) {
  const token = ++paintToken;
  const data = await invoke("overview");
  if (token !== paintToken) return;
  blocked = false;
  const previous = selected;
  mode = "overview";
  sections = [];
  groups = [];
  overviewLabels = data.suggestions.length > 0;
  if (overviewLabels) {
    groups.push({
      label: data.suggestion_label
        ? t("overlay.suggested_for", { app: data.suggestion_label })
        : t("overlay.suggested"),
      items: data.suggestions,
      removable: false,
    });
  }
  if (data.recents.length) {
    groups.push({ label: t("overlay.recent"), items: data.recents, removable: true });
  }
  items = groups.flatMap((group) => group.items);
  selected = keepSelection ? Math.min(previous, Math.max(0, items.length - 1)) : 0;
  closeActions();
  const date = new Date().toLocaleDateString(undefined, { weekday: "short", day: "numeric", month: "short" });
  count.textContent = `${greeting()} · ${date} · ${formatUptime(data.uptime_secs)}`;
  render();
}

async function search() {
  const text = input.value;
  blocked = false; // typing (or a re-run) leaves the warning behind
  closeActions();
  if (!text.trim()) {
    loadOverview();
    return;
  }
  const token = ++paintToken;
  const results = await invoke("search", { text });
  if (token !== paintToken) return;
  mode = "results";
  groups = [];
  sections = results;
  items = sections.flatMap((section) => section.items);
  selected = 0;
  count.textContent = items.length === 1 ? t("overlay.result") : t("overlay.results", { count: items.length });
  render();
}

/* ── a refused autotype ── */

// The vault's login-form guard turned an autotype down (it would have typed a password
// into a chat box, or found no field at all). The credential comes back with the reason
// nothing was typed; its first action is the armed "type it anyway", so Enter arms and a
// second Enter overrides — the same confirm path every destructive action uses. Nothing
// here is interpreted: the row and its actions are the backend's, as always.
function showBlocked(label, item) {
  paintToken += 1;
  vaultPrompt = false;
  unlocking = false;
  blocked = true;
  closeActions();
  input.type = "text";
  input.placeholder = t("overlay.placeholder");
  input.value = "";
  mode = "results";
  groups = [];
  sections = [{ label, items: [item] }];
  items = [item];
  selected = 0;
  count.textContent = label;
  render();
  input.focus();
}

// Re-run whatever is on screen in place, keeping the highlighted row — used when vault
// favicons (or the focus context) arrive in the background. Never disturbs an open
// actions menu, the password prompt, or a warning the user hasn't answered yet.
async function refreshResults() {
  if (actionsFor || vaultPrompt || blocked) return;
  const text = input.value;
  if (!text.trim()) {
    loadOverview({ keepSelection: true });
    return;
  }
  const prev = selected;
  const token = ++paintToken;
  const results = await invoke("search", { text });
  if (token !== paintToken) return;
  mode = "results";
  sections = results;
  items = sections.flatMap((section) => section.items);
  selected = Math.min(prev, Math.max(0, items.length - 1));
  count.textContent = items.length === 1 ? t("overlay.result") : t("overlay.results", { count: items.length });
  render();
}

async function run(item, actionIndex) {
  try {
    // The backend hides the overlay itself on success (launched apps keep focus).
    await invoke("run_action", { item, actionIndex });
  } catch (e) {
    console.error(e);
  }
  closeActions();
}

/// Run the item's action at `index`, unless it needs confirming — then the actions
/// menu opens (or stays open) with that action armed, and the next Enter runs it.
function maybeRun(item, index, { fromMenu = false } = {}) {
  const named = item.actions[index] || item.actions[0];
  if (!named) return;
  if (named.confirm && !(fromMenu && confirming && index === actionSelected)) {
    actionsFor = item;
    actionSelected = item.actions.indexOf(named);
    confirming = true;
    render();
    return;
  }
  run(item, item.actions.indexOf(named));
}

function moveSelection(delta) {
  if (actionsFor) {
    const total = actionsFor.actions.length;
    actionSelected = (actionSelected + delta + total) % total;
    confirming = false;
    render();
    return;
  }
  if (!items.length) return;
  selected = (selected + delta + items.length) % items.length;
  render();
}

input.addEventListener("input", () => {
  if (vaultPrompt) return; // password keystrokes never touch search
  search();
});

document.addEventListener("keydown", (e) => {
  if (vaultPrompt) {
    if (e.key === "Escape") {
      exitVaultPrompt(true);
    } else if (e.key === "Enter") {
      submitVaultPassword();
    } else if (e.key === "Tab" || e.key === "ArrowDown" || e.key === "ArrowUp") {
      e.preventDefault(); // nothing to navigate while prompting
    }
    return;
  }
  if (e.key === "Escape") {
    if (confirming) {
      confirming = false;
      render();
    } else if (actionsFor) {
      closeActions();
      render();
    } else {
      invoke("hide_overlay");
    }
  } else if (e.key === "ArrowDown") {
    e.preventDefault();
    moveSelection(1);
  } else if (e.key === "ArrowUp") {
    e.preventDefault();
    moveSelection(-1);
  } else if (e.key === "Tab") {
    e.preventDefault();
    if (actionsFor) {
      closeActions();
    } else if (items[selected]) {
      actionsFor = items[selected];
      actionSelected = 0;
      confirming = false;
    }
    render();
  } else if (e.key === "Enter") {
    if (actionsFor) {
      maybeRun(actionsFor, actionSelected, { fromMenu: true });
    } else if (items[selected]) {
      const item = items[selected];
      const index = e.shiftKey && item.actions.length > 1 ? 1 : 0;
      maybeRun(item, index);
    }
  } else if (e.ctrlKey && !e.altKey && /^[1-9]$/.test(e.key)) {
    // Ctrl+n runs the selected item's nth action directly — the shortcuts the
    // actions menu advertises, usable with or without the menu open.
    const target = actionsFor || items[selected];
    const index = Number(e.key) - 1;
    if (target && index < target.actions.length) {
      e.preventDefault();
      maybeRun(target, index, { fromMenu: !!actionsFor });
    }
  }
});

// Reset while invisible: the next summon then opens straight onto a fresh,
// correctly-sized overview instead of flashing the previous search.
listen("overlay-hidden", () => {
  if (vaultPrompt) {
    vaultPrompt = false;
    unlocking = false;
    vaultReturnQuery = "";
    input.type = "text";
    input.placeholder = t("overlay.placeholder");
  }
  input.value = "";
  loadOverview();
});

// The locked vault's "Unlock vault" row lands here (run_action emits, overlay stays up).
listen("vault-unlock", () => enterVaultPrompt());

// A Windows Hello unlock finished backend-side; the current `v …` query has rows now.
// Move the caret back into the search field too (the Hello dialog stole focus).
listen("vault-unlocked", () => {
  input.focus();
  search();
});

// Autotype refused: the vault won't type a password into a window that shows no login
// form (funke-shell's `form` guard). The overlay takes the warning over whatever it was
// showing — including the overview it may still be loading, hence the paint token.
listen("autotype-blocked", (e) => showBlocked(e.payload.label, e.payload.item));

// Background favicon fetches populated the cache: re-render the current results so
// the icons appear in place, without disturbing the selection.
listen("vault-icons-updated", () => refreshResults());

// Reading the foreground window (and, in a browser, its URL) happens off-thread, so it
// can land a few milliseconds after the overlay is already up: pull the credential
// suggestions for it in without touching what the user is doing.
listen("focus-context", () => refreshResults());

// A clip was removed (or the history cleared) while its list is on screen — the row has
// to go now, not on the next summon.
listen("clipboard-changed", () => refreshResults());

// Hello unlock failed (cancelled, expired session, Hello not set up, …): fall back to
// the masked password prompt with the reason shown — Esc returns to the query.
listen("vault-unlock-failed", (e) => {
  if (!vaultPrompt) enterVaultPrompt();
  renderVaultPrompt(String(e.payload));
});

listen("overlay-shown", () => {
  // A leftover password prompt (e.g. a Hello failure that arrived while hidden)
  // must never greet the next summon.
  if (vaultPrompt) {
    vaultPrompt = false;
    unlocking = false;
    vaultReturnQuery = "";
    input.type = "text";
    input.placeholder = t("overlay.placeholder");
  }
  input.value = "";
  loadOverview(); // refreshes greeting/uptime; content is already reset
  input.focus();
  panel.classList.remove("opening");
  void panel.offsetWidth; // restart the summon animation
  panel.classList.add("opening");
});

// Re-theme, re-translate and re-measure while hidden, so changes from the settings window
// are already in place the next time the overlay shows.
listen("settings-changed", async (e) => {
  applyAccent(e.payload);
  // Rust resolves `auto` against Windows, so ask it rather than reading the raw setting.
  setLocale(await invoke("locale"));
  applyTranslations();
  await loadOverview();
  resize();
});

// The locale first: everything below renders text, and rendering it twice would flash
// English at a German user on every launch.
(async () => {
  setLocale(await invoke("locale"));
  applyTranslations();
  applyKeycaps(); // the footer's key hints: markup, so they are drawn once
  invoke("get_settings").then(applyAccent);
  await loadOverview();
  input.focus();
})();
