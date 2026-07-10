const { invoke } = window.__TAURI__.core;
const { listen } = window.__TAURI__.event;

const panel = document.getElementById("panel");
const input = document.getElementById("query");
const context = document.getElementById("context");
const list = document.getElementById("results");
const footer = document.getElementById("status");
const count = document.getElementById("count");

const TIPS = [
  "Type to search apps, files, and windows",
  "f <query> searches files only, w windows, g the web, v your vault",
  "Tab lists every action of the selected result",
];

const SEARCH_PLACEHOLDER = "Search…";

let items = [];
let sections = []; // results mode: [{ label, items }] — `items` stays the flat list for navigation
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
  if (h < 5) return "Good night";
  if (h < 12) return "Good morning";
  if (h < 18) return "Good afternoon";
  return "Good evening";
}

function formatUptime(secs) {
  const h = Math.floor(secs / 3600);
  const m = Math.floor((secs % 3600) / 60);
  return h ? `up ${h} h ${m} min` : `up ${m} min`;
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

function itemRow(item, index) {
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

  const hint = document.createElement("kbd");
  hint.className = "hint";
  hint.textContent = "↵";
  li.appendChild(hint);

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
    sub.textContent = "Press Enter again to confirm — Esc cancels";
    text.appendChild(sub);
  }
  li.appendChild(text);

  const hint = document.createElement("kbd");
  hint.className = "hint";
  hint.textContent = index === 0 ? "↵" : index === 1 ? "⇧↵" : "";
  if (!hint.textContent) hint.style.visibility = "hidden";
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
    label.textContent = "Actions";
    list.appendChild(label);

    actionsFor.actions.forEach((named, i) => list.appendChild(actionRow(actionsFor, named, i)));
    resize();
    return;
  }

  context.hidden = !(mode === "overview" && items.length > 0);
  footer.hidden = mode === "results" && items.length === 0;

  if (!items.length) {
    if (mode === "overview") {
      TIPS.forEach((tip) => {
        const li = document.createElement("li");
        li.className = "tip";
        li.textContent = tip;
        list.appendChild(li);
      });
    } else if (input.value.trim()) {
      const li = document.createElement("li");
      li.className = "empty";
      li.textContent = "No results";
      list.appendChild(li);
    }
    resize();
    return;
  }

  if (mode === "results") {
    // Sectioned: label + rows per group; `index` keeps navigation flat across groups.
    let index = 0;
    sections.forEach((section) => {
      const label = document.createElement("li");
      label.className = "group";
      label.textContent = section.label;
      list.appendChild(label);
      section.items.forEach((item) => {
        list.appendChild(itemRow(item, index));
        index += 1;
      });
    });
  } else {
    items.forEach((item, i) => list.appendChild(itemRow(item, i)));
  }

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
  input.placeholder = "Master password…";
  input.focus();
  renderVaultPrompt(null);
}

function exitVaultPrompt(restoreQuery) {
  vaultPrompt = false;
  unlocking = false;
  input.value = "";
  input.type = "text";
  input.placeholder = SEARCH_PLACEHOLDER;
  input.value = restoreQuery ? vaultReturnQuery : "";
  vaultReturnQuery = "";
  input.focus();
  search();
}

function renderVaultPrompt(error) {
  list.innerHTML = "";
  context.hidden = true;
  footer.hidden = false;
  count.textContent = "Bitwarden vault";

  const tip = document.createElement("li");
  tip.className = "tip";
  tip.textContent = unlocking ? "Unlocking…" : "Enter unlocks the vault — Esc cancels";
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

async function loadOverview() {
  const data = await invoke("overview");
  mode = "overview";
  sections = [];
  items = data.recents;
  selected = 0;
  closeActions();
  const date = new Date().toLocaleDateString(undefined, { weekday: "short", day: "numeric", month: "short" });
  count.textContent = `${greeting()} · ${date} · ${formatUptime(data.uptime_secs)}`;
  render();
}

async function search() {
  const text = input.value;
  closeActions();
  if (!text.trim()) {
    loadOverview();
    return;
  }
  mode = "results";
  sections = await invoke("search", { text });
  items = sections.flatMap((section) => section.items);
  selected = 0;
  count.textContent = items.length === 1 ? "1 result" : `${items.length} results`;
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
    input.placeholder = SEARCH_PLACEHOLDER;
  }
  input.value = "";
  loadOverview();
});

// The locked vault's "Unlock vault" row lands here (run_action emits, overlay stays up).
listen("vault-unlock", () => enterVaultPrompt());

listen("overlay-shown", () => {
  input.value = "";
  loadOverview(); // refreshes greeting/uptime; content is already reset
  input.focus();
  panel.classList.remove("opening");
  void panel.offsetWidth; // restart the summon animation
  panel.classList.add("opening");
});

// Re-theme and re-measure while hidden, so changes from the settings window are
// already in place the next time the overlay shows.
listen("settings-changed", (e) => {
  applyAccent(e.payload);
  resize();
});

invoke("get_settings").then(applyAccent);
loadOverview();
input.focus();
