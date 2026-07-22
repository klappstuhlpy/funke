# Overlay Customization Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add appearance, position/size, and behavior customization for the summon overlay, driven through the existing settings → `settings-changed` → live re-theme pipe, plus built-in presets.

**Architecture:** New `funke_core::Settings` fields (all `#[serde(default)]`, defaults reproduce today's look). The overlay's `applyAccent` becomes `applyTheme`, pushing the appearance knobs as CSS custom properties on `document.documentElement`; a cached `cfg` object lets the reset listeners honor behavior toggles. Position and hide-on-blur are read Rust-side. Settings UI gains an Appearance group and preset buttons.

**Tech Stack:** Rust (Tauri v2, serde), vanilla JS/CSS (no bundler), embedded static UI.

## Global Constraints

- Rust stable ≥ 1.85; Windows 10/11. Edition 2021, rustfmt `max_width = 120`.
- CI gates, all four must stay clean: `cargo fmt --all --check`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo test --workspace`, `cargo build`.
- **Core purity:** `funke-core` never imports tauri/webview/Win32.
- **Tokens only** (design invariant): CSS component rules never hard-code raw colors/radius/spacing — everything comes from `:root` vars. New knobs must be vars.
- **Glass is native:** never add CSS blur/box-shadow to the panel. Panel *opacity* is a tint alpha only.
- **Window height follows content** (invariant #4): the panel measures and calls `resize_overlay`; new caps bound only the results list, never the panel.
- **i18n parity:** every new UI string goes into BOTH `crates/funke-app/ui/locales/en.js` and `de.js`, same key set, same `{placeholders}`. The `ui_locales_stay_in_step` test enforces it.
- Commits authored by the repo owner only — **no AI co-author trailers**.
- Defaults that must reproduce today's look: `overlay_position=0.24`, `font_family=""`, `font_scale=1.0`, `corner_radius=9.0`, `row_density="comfortable"`, `panel_opacity=1.0`, `max_visible_rows=8`, `placeholder=""`, `hide_on_blur=true`, `clear_on_hide=true`.

---

### Task 1: Core Settings fields

**Files:**
- Modify: `crates/funke-core/src/settings.rs` (struct fields + `Default`)
- Test: `crates/funke-core/src/settings.rs` (inline `#[cfg(test)]`)

**Interfaces:**
- Produces: ten new public fields on `funke_core::Settings` — `overlay_position: f64`, `font_family: String`, `font_scale: f64`, `corner_radius: f64`, `row_density: String`, `panel_opacity: f64`, `max_visible_rows: u32`, `placeholder: String`, `hide_on_blur: bool`, `clear_on_hide: bool`. All serialize with these serde field names (snake_case).

- [ ] **Step 1: Write the failing test**

Add to the existing test module in `crates/funke-core/src/settings.rs` (or create one at the end of the file if none exists there):

```rust
#[cfg(test)]
mod customization_tests {
    use super::Settings;

    #[test]
    fn old_settings_json_loads_new_fields_as_defaults() {
        // A settings.json written before these fields existed.
        let json = r#"{
            "language": "auto", "hotkey": "Ctrl+Space", "scope_hotkeys": [],
            "accent": "#d97757", "overlay_width": 680.0, "web_engine": "duckduckgo",
            "disabled_providers": [], "index_roots": [], "index_hidden": false,
            "autostart": false, "update_check": true, "vault_hello": false,
            "vault_icons": true, "vault_idle_lock_minutes": 10, "vault_autotype_enter": true,
            "vault_autotype_sequence": "", "vault_autotype_guard": true,
            "vault_lock_on_screen_lock": true, "vault_capture_shield": true,
            "vault_require_signed_cli": false, "vault_context_suggest": true,
            "snippets": [], "quicklinks": [], "pinned": [], "pins_collapsed": false
        }"#;
        let s: Settings = serde_json::from_str(json).expect("old json must still parse");
        assert_eq!(s.overlay_position, 0.24);
        assert_eq!(s.font_family, "");
        assert_eq!(s.font_scale, 1.0);
        assert_eq!(s.corner_radius, 9.0);
        assert_eq!(s.row_density, "comfortable");
        assert_eq!(s.panel_opacity, 1.0);
        assert_eq!(s.max_visible_rows, 8);
        assert_eq!(s.placeholder, "");
        assert!(s.hide_on_blur);
        assert!(s.clear_on_hide);
    }

    #[test]
    fn new_fields_round_trip() {
        let mut s = Settings::default();
        s.overlay_position = 0.3;
        s.font_scale = 1.15;
        s.row_density = "compact".into();
        let json = serde_json::to_string(&s).unwrap();
        let back: Settings = serde_json::from_str(&json).unwrap();
        assert_eq!(back.overlay_position, 0.3);
        assert_eq!(back.font_scale, 1.15);
        assert_eq!(back.row_density, "compact");
    }
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p funke-core customization_tests`
Expected: FAIL — the struct has no `overlay_position` field (compile error).

- [ ] **Step 3: Add the fields**

In `crates/funke-core/src/settings.rs`, inside `pub struct Settings { … }`, before the closing brace (after `pins_collapsed`), add each field with `#[serde(default = "…")]` so old JSON loads them. Add these default-fn helpers and fields:

```rust
    /// Vertical screen fraction (0.0 top … 0.9) for the overlay's top edge. 0.24 = Spotlight.
    #[serde(default = "default_overlay_position")]
    pub overlay_position: f64,
    /// Overlay font family; empty = the built-in Segoe stack.
    #[serde(default)]
    pub font_family: String,
    /// Overlay text scale multiplier (clamped 0.85–1.3 at apply time).
    #[serde(default = "default_font_scale")]
    pub font_scale: f64,
    /// Corner radius in px for the panel and rows.
    #[serde(default = "default_corner_radius")]
    pub corner_radius: f64,
    /// Row density: `comfortable` (default) or `compact`.
    #[serde(default = "default_row_density")]
    pub row_density: String,
    /// Panel tint opacity multiplier (clamped 0.5–1.0 at apply time). 1.0 = today's tint.
    #[serde(default = "default_panel_opacity")]
    pub panel_opacity: f64,
    /// Rows visible in the results list before it scrolls.
    #[serde(default = "default_max_visible_rows")]
    pub max_visible_rows: u32,
    /// Custom search-field placeholder; empty = the localized default.
    #[serde(default)]
    pub placeholder: String,
    /// Hide the overlay when it loses focus (click-away). Off keeps it up until Esc.
    #[serde(default = "default_true")]
    pub hide_on_blur: bool,
    /// Clear the typed query when the overlay hides. Off preserves it across summons.
    #[serde(default = "default_true")]
    pub clear_on_hide: bool,
```

Add the default helper fns near the top of the file (after the `use` lines, before `pub struct Settings`):

```rust
fn default_overlay_position() -> f64 {
    0.24
}
fn default_font_scale() -> f64 {
    1.0
}
fn default_corner_radius() -> f64 {
    9.0
}
fn default_row_density() -> String {
    "comfortable".into()
}
fn default_panel_opacity() -> f64 {
    1.0
}
fn default_max_visible_rows() -> u32 {
    8
}
fn default_true() -> bool {
    true
}
```

In `impl Default for Settings`, add the matching initializers before the closing `}` (after `pins_collapsed: false,`):

```rust
            overlay_position: 0.24,
            font_family: String::new(),
            font_scale: 1.0,
            corner_radius: 9.0,
            row_density: "comfortable".into(),
            panel_opacity: 1.0,
            max_visible_rows: 8,
            placeholder: String::new(),
            hide_on_blur: true,
            clear_on_hide: true,
```

> Note: if the file's `[dev-dependencies]` lacks `serde_json`, the test's `serde_json::from_str` still works because `serde_json` is a normal dependency of core; if the test fails to compile on `serde_json`, add `serde_json` to `crates/funke-core/Cargo.toml` `[dev-dependencies]`.

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test -p funke-core customization_tests`
Expected: PASS (both tests).

- [ ] **Step 5: Verify formatting/lint**

Run: `cargo fmt --all && cargo clippy -p funke-core --all-targets -- -D warnings`
Expected: no changes needed, no warnings.

- [ ] **Step 6: Commit**

```bash
git add crates/funke-core/src/settings.rs crates/funke-core/Cargo.toml
git commit -m "feat(core): overlay customization settings fields"
```

---

### Task 2: Rust — position and hide-on-blur read settings

**Files:**
- Modify: `crates/funke-app/src/main.rs` — `position_overlay` (~line 1292) and the `WindowEvent::Focused(false)` arm (~line 1647).

**Interfaces:**
- Consumes: `Settings::overlay_position`, `Settings::hide_on_blur` from Task 1.
- Produces: nothing new; behavior change only.

- [ ] **Step 1: Position reads the setting**

In `position_overlay` (`crates/funke-app/src/main.rs`), it currently has no access to state. Change its signature to take the fraction as a parameter and pass it from the caller, OR read state inside. Simplest: change the caller. Find `fn position_overlay(win: &tauri::WebviewWindow)` and its body line:

```rust
        let y = mpos.y + (msize.height as f64 * 0.24) as i32;
```

Replace `0.24` with a parameter. Change the signature to:

```rust
fn position_overlay(win: &tauri::WebviewWindow, fraction: f64) {
```

and the line to:

```rust
        let y = mpos.y + (msize.height as f64 * fraction.clamp(0.0, 0.9)) as i32;
```

- [ ] **Step 2: Update the caller**

Find the single call site (`position_overlay(&win);` around line 1309, inside `show`). It has access to `state`. Replace with:

```rust
        let fraction = state.settings.read().unwrap().overlay_position;
        position_overlay(&win, fraction);
```

If `state` is not in scope at that call site, read the setting via `app.state::<AppState>()`:

```rust
        let fraction = app.state::<AppState>().settings.read().unwrap().overlay_position;
        position_overlay(&win, fraction);
```

(Use whichever compiles; `show`'s surrounding code shows which handle is available.)

- [ ] **Step 3: hide-on-blur honors the setting**

In the `WindowEvent::Focused(false)` arm (~line 1647), after the existing `hello` guard and before `let _ = window.hide();`, add a settings check:

```rust
                    let app = window.app_handle();
                    let st = app.state::<AppState>();
                    if st.hello_in_flight.load(std::sync::atomic::Ordering::SeqCst) {
                        return;
                    }
                    if !st.settings.read().unwrap().hide_on_blur {
                        return; // user disabled click-away dismissal
                    }
                    let _ = window.hide();
                    let _ = window.emit("overlay-hidden", ());
```

Replace the existing arm body (the `let hello = …; if hello { return; } let _ = window.hide(); let _ = window.emit(…)`) with the block above so the `hello_in_flight` check isn't duplicated. Keep the surrounding comment.

- [ ] **Step 4: Build and smoke-run**

Run: `cargo build -p funke-app`
Expected: compiles.

Run the binary briefly (it must print the tray line and stay alive):
Run: `cargo run -p funke-app` — confirm it prints the tray startup line and does not exit; then quit via tray or by typing `quit`.
Expected: starts cleanly.

- [ ] **Step 5: Verify lint**

Run: `cargo clippy -p funke-app --all-targets -- -D warnings`
Expected: no warnings.

- [ ] **Step 6: Commit**

```bash
git add crates/funke-app/src/main.rs
git commit -m "feat(app): overlay position and hide-on-blur follow settings"
```

---

### Task 3: Overlay — applyTheme, CSS tokens, behavior toggles

**Files:**
- Modify: `crates/funke-app/ui/main.js` (rename `applyAccent`→`applyTheme`, add cfg cache, `refreshPlaceholder`, reset listeners)
- Modify: `crates/funke-app/ui/style.css` (token-ize the knobs)

**Interfaces:**
- Consumes: the full `Settings` object from `get_settings` / `settings-changed`.
- Produces: `applyTheme(settings)`, `refreshPlaceholder()`, module var `cfg`.

- [ ] **Step 1: Token-ize CSS**

In `crates/funke-app/ui/style.css`:

In `:root` add defaults (do not remove existing `--radius`/`--radius-s`; they'll be overridden by JS at runtime, but the file keeps its authored defaults):

```css
  --scale: 1;
  --item-pad-y: 8px;
  --item-gap-y: 2px;
  --results-max: 416px;
```

Change `#query` font-size (line ~103) from `font-size: 17px;` to:

```css
  font-size: calc(17px * var(--scale));
```

Change `.title` font-size (line ~257) from `font-size: 14px;` to:

```css
  font-size: calc(14px * var(--scale));
```

Change `.subtitle` font-size (line ~266) from `font-size: 11.5px;` to:

```css
  font-size: calc(11.5px * var(--scale));
```

Change `.item` padding (line ~168) from `padding: 8px 10px;` to:

```css
  padding: var(--item-pad-y) 10px;
```

Change `.item + .item` margin (line ~174) from `margin-top: 2px;` to:

```css
  margin-top: var(--item-gap-y);
```

Change `#results` max-height (line ~156) from `max-height: 416px;` to:

```css
  max-height: var(--results-max);
```

- [ ] **Step 2: Rename applyAccent → applyTheme and extend it**

In `crates/funke-app/ui/main.js`, replace the `applyAccent` function (lines ~71–81) with:

```javascript
// Cached settings so the reset listeners can honor behavior toggles.
let cfg = {};

// The overlay's look follows settings; the base palette (beyond accent) is fixed.
function applyTheme(settings) {
  cfg = settings || {};
  const root = document.documentElement.style;

  // accent (unchanged math)
  const match = /^#([0-9a-f]{6})$/i.exec(settings.accent || "");
  if (match) {
    const n = parseInt(match[1], 16);
    const [r, g, b] = [n >> 16, (n >> 8) & 255, n & 255];
    root.setProperty("--accent", settings.accent);
    root.setProperty("--accent-soft", `rgba(${r}, ${g}, ${b}, 0.15)`);
    root.setProperty("--accent-stroke", `rgba(${r}, ${g}, ${b}, 0.3)`);
  }

  // font family
  if (settings.font_family) root.setProperty("--font", settings.font_family);
  else root.removeProperty("--font");

  // text scale (clamped)
  const scale = Math.min(1.3, Math.max(0.85, Number(settings.font_scale) || 1));
  root.setProperty("--scale", String(scale));

  // corner radius
  const radius = Math.max(0, Number(settings.corner_radius));
  if (Number.isFinite(radius)) {
    root.setProperty("--radius", `${radius}px`);
    root.setProperty("--radius-s", `${Math.max(0, radius - 2)}px`);
  }

  // row density
  const compact = settings.row_density === "compact";
  root.setProperty("--item-pad-y", compact ? "5px" : "8px");
  root.setProperty("--item-gap-y", compact ? "1px" : "2px");

  // panel tint opacity (multiplies the authored 0.58 alpha)
  const op = Math.min(1, Math.max(0.5, Number(settings.panel_opacity) || 1));
  root.setProperty("--glass-tint", `rgba(30, 27, 24, ${0.58 * op})`);

  // visible rows before scroll (~52px/row comfortable; keeps 8→416)
  const rows = Math.max(3, Number(settings.max_visible_rows) || 8);
  root.setProperty("--results-max", `${Math.round(rows * 52)}px`);

  refreshPlaceholder();
}

// The search field's placeholder: the user's custom text, else the localized default.
function refreshPlaceholder() {
  if (!vaultPrompt) input.placeholder = cfg.placeholder || t("overlay.placeholder");
}
```

- [ ] **Step 3: Route every applyAccent call through applyTheme**

Search `crates/funke-app/ui/main.js` for `applyAccent` and replace all remaining references:
- Line ~730 (`settings-changed` listener): `applyAccent(e.payload);` → `applyTheme(e.payload);`
- Line ~744 (boot): `invoke("get_settings").then(applyAccent);` → `invoke("get_settings").then(applyTheme);`

- [ ] **Step 4: Replace default-placeholder resets with refreshPlaceholder**

In `crates/funke-app/ui/main.js`, replace every `input.placeholder = t("overlay.placeholder");` that restores the *default* (exit-vault-prompt paths at lines ~294, ~503, ~650, ~715) with `refreshPlaceholder();`. Do NOT touch line ~283 (`input.placeholder = t("overlay.master_password");`) — that sets the masked prompt.

- [ ] **Step 5: clear_on_hide in the reset listeners**

In the `overlay-hidden` listener (lines ~644–654), keep the `vaultPrompt` cleanup block unconditional (security), but guard the query reset:

```javascript
listen("overlay-hidden", () => {
  if (vaultPrompt) {
    vaultPrompt = false;
    unlocking = false;
    vaultReturnQuery = "";
    input.type = "text";
    refreshPlaceholder();
  }
  if (cfg.clear_on_hide === false) return; // preserve the typed query across summons
  input.value = "";
  loadOverview();
});
```

In the `overlay-shown` listener (lines ~707–725), after the `vaultPrompt` cleanup block, replace the `input.value = ""; loadOverview();` lines with a branch that preserves a query when requested:

```javascript
  if (cfg.clear_on_hide === false && input.value.trim()) {
    search(); // re-run the preserved query against the freshly-summoned state
  } else {
    input.value = "";
    loadOverview(); // refreshes greeting/uptime; content is already reset
  }
  input.focus();
  panel.classList.remove("opening");
  void panel.offsetWidth; // restart the summon animation
  panel.classList.add("opening");
```

(Also change the `input.placeholder = t("overlay.placeholder");` inside that listener's vaultPrompt block to `refreshPlaceholder();`.)

- [ ] **Step 6: Verify the parser tests still hold and build the app**

Run: `cargo test -p funke-app ui_locales`
Expected: PASS (no locale changes yet — sanity that nothing broke).

Run: `cargo run -p funke-app` — summon the overlay (Ctrl+Space), confirm it renders and the tray line printed. Quit.
Expected: overlay shows normally with default look unchanged.

- [ ] **Step 7: Commit**

```bash
git add crates/funke-app/ui/main.js crates/funke-app/ui/style.css
git commit -m "feat(ui): applyTheme drives overlay appearance and behavior tokens"
```

---

### Task 4: Settings UI — controls, presets, strings

**Files:**
- Modify: `crates/funke-app/ui/settings.html` (Appearance pane: new controls)
- Modify: `crates/funke-app/ui/settings.js` (constants, wiring, render, presets)
- Modify: `crates/funke-app/ui/settings.css` (slider styling only)
- Modify: `crates/funke-app/ui/locales/en.js` and `crates/funke-app/ui/locales/de.js` (new keys)

**Interfaces:**
- Consumes: `save(patch)` (existing), `settings` object, `t(key)` (existing).
- Produces: new DOM ids used by `renderAll`: `overlay-position`, `font-family`, `font-scale`, `corner-radius`, `row-density`, `panel-opacity`, `max-rows`, `placeholder`, `hide-on-blur`, `clear-on-hide`, `presets`.

- [ ] **Step 1: Add locale keys to BOTH en.js and de.js**

Append these keys (keep the `"key": "value",` one-per-line shape the parity parser expects). In `crates/funke-app/ui/locales/en.js`, inside the strings object, add:

```javascript
  "settings.section.tuning": "Layout & feel",
  "settings.position.label": "Position",
  "settings.position.desc": "How far down the screen the bar appears.",
  "settings.font.label": "Font",
  "settings.font.desc": "Typeface for the overlay. System default keeps the built-in stack.",
  "settings.font.system": "System default",
  "settings.fontscale.label": "Text size",
  "settings.fontscale.desc": "Scales the bar's text.",
  "settings.radius.label": "Corner radius",
  "settings.radius.desc": "Roundness of the panel and rows.",
  "settings.density.label": "Row density",
  "settings.density.desc": "Spacing between result rows.",
  "settings.density.comfortable": "Comfortable",
  "settings.density.compact": "Compact",
  "settings.opacity.label": "Panel opacity",
  "settings.opacity.desc": "Tint strength over the glass backdrop.",
  "settings.rows.label": "Visible rows",
  "settings.rows.desc": "Results shown before the list scrolls.",
  "settings.placeholder.label": "Placeholder",
  "settings.placeholder.desc": "Hint text in the empty search field.",
  "settings.behavior.hideblur.label": "Hide when it loses focus",
  "settings.behavior.hideblur.desc": "Dismiss the bar when you click away. Off keeps it up until Esc.",
  "settings.behavior.clearhide.label": "Clear query on hide",
  "settings.behavior.clearhide.desc": "Forget the typed text each time the bar hides.",
  "settings.presets.label": "Presets",
  "settings.presets.desc": "Apply a bundled look. You can still fine-tune after.",
  "settings.presets.default": "Default",
  "settings.presets.compact": "Compact",
  "settings.presets.terminal": "Terminal",
```

In `crates/funke-app/ui/locales/de.js`, add the SAME keys with German values (match placeholders — none here):

```javascript
  "settings.section.tuning": "Layout & Gefühl",
  "settings.position.label": "Position",
  "settings.position.desc": "Wie weit unten am Bildschirm die Leiste erscheint.",
  "settings.font.label": "Schrift",
  "settings.font.desc": "Schriftart der Oberfläche. Systemstandard behält den eingebauten Satz.",
  "settings.font.system": "Systemstandard",
  "settings.fontscale.label": "Textgröße",
  "settings.fontscale.desc": "Skaliert den Text der Leiste.",
  "settings.radius.label": "Eckenradius",
  "settings.radius.desc": "Rundung von Panel und Zeilen.",
  "settings.density.label": "Zeilendichte",
  "settings.density.desc": "Abstand zwischen den Ergebniszeilen.",
  "settings.density.comfortable": "Komfortabel",
  "settings.density.compact": "Kompakt",
  "settings.opacity.label": "Panel-Deckkraft",
  "settings.opacity.desc": "Tönungsstärke über dem Glashintergrund.",
  "settings.rows.label": "Sichtbare Zeilen",
  "settings.rows.desc": "Ergebnisse, bevor die Liste scrollt.",
  "settings.placeholder.label": "Platzhalter",
  "settings.placeholder.desc": "Hinweistext im leeren Suchfeld.",
  "settings.behavior.hideblur.label": "Ausblenden bei Fokusverlust",
  "settings.behavior.hideblur.desc": "Leiste schließen, wenn du wegklickst. Aus lässt sie bis Esc offen.",
  "settings.behavior.clearhide.label": "Eingabe beim Ausblenden löschen",
  "settings.behavior.clearhide.desc": "Den Text bei jedem Ausblenden vergessen.",
  "settings.presets.label": "Voreinstellungen",
  "settings.presets.desc": "Ein gebündeltes Aussehen anwenden. Du kannst danach weiter anpassen.",
  "settings.presets.default": "Standard",
  "settings.presets.compact": "Kompakt",
  "settings.presets.terminal": "Terminal",
```

- [ ] **Step 2: Run the parity test — it must pass**

Run: `cargo test -p funke-app ui_locales`
Expected: PASS (both locales now carry the identical key set).

- [ ] **Step 3: Add the HTML controls**

In `crates/funke-app/ui/settings.html`, inside `<div class="pane" id="pane-appearance">`, after the existing overlay `<div class="card">` that closes at line ~142, add a new section. Insert before the closing `</div>` of the appearance pane:

```html
            <h2 class="section" data-i18n="settings.section.tuning"></h2>
            <div class="card">
              <div class="row">
                <div class="what">
                  <div class="label" data-i18n="settings.presets.label"></div>
                  <div class="desc" data-i18n="settings.presets.desc"></div>
                </div>
                <div class="segmented" id="presets"></div>
              </div>
              <div class="row">
                <div class="what">
                  <div class="label" data-i18n="settings.position.label"></div>
                  <div class="desc" data-i18n="settings.position.desc"></div>
                </div>
                <input type="range" id="overlay-position" class="slider" min="5" max="60" step="1" />
              </div>
              <div class="row">
                <div class="what">
                  <div class="label" data-i18n="settings.font.label"></div>
                  <div class="desc" data-i18n="settings.font.desc"></div>
                </div>
                <select id="font-family" class="select"></select>
              </div>
              <div class="row">
                <div class="what">
                  <div class="label" data-i18n="settings.fontscale.label"></div>
                  <div class="desc" data-i18n="settings.fontscale.desc"></div>
                </div>
                <input type="range" id="font-scale" class="slider" min="85" max="130" step="5" />
              </div>
              <div class="row">
                <div class="what">
                  <div class="label" data-i18n="settings.radius.label"></div>
                  <div class="desc" data-i18n="settings.radius.desc"></div>
                </div>
                <input type="range" id="corner-radius" class="slider" min="0" max="20" step="1" />
              </div>
              <div class="row">
                <div class="what">
                  <div class="label" data-i18n="settings.density.label"></div>
                  <div class="desc" data-i18n="settings.density.desc"></div>
                </div>
                <div class="segmented" id="row-density"></div>
              </div>
              <div class="row">
                <div class="what">
                  <div class="label" data-i18n="settings.opacity.label"></div>
                  <div class="desc" data-i18n="settings.opacity.desc"></div>
                </div>
                <input type="range" id="panel-opacity" class="slider" min="50" max="100" step="5" />
              </div>
              <div class="row">
                <div class="what">
                  <div class="label" data-i18n="settings.rows.label"></div>
                  <div class="desc" data-i18n="settings.rows.desc"></div>
                </div>
                <input type="range" id="max-rows" class="slider" min="3" max="14" step="1" />
              </div>
              <div class="row">
                <div class="what">
                  <div class="label" data-i18n="settings.placeholder.label"></div>
                  <div class="desc" data-i18n="settings.placeholder.desc"></div>
                </div>
                <input type="text" id="placeholder" class="text-input" maxlength="60" />
              </div>
            </div>

            <h2 class="section" data-i18n="settings.section.behavior"></h2>
            <div class="card">
              <div class="row">
                <div class="what">
                  <div class="label" data-i18n="settings.behavior.hideblur.label"></div>
                  <div class="desc" data-i18n="settings.behavior.hideblur.desc"></div>
                </div>
                <button class="toggle" id="hide-on-blur" role="switch" aria-checked="true"></button>
              </div>
              <div class="row">
                <div class="what">
                  <div class="label" data-i18n="settings.behavior.clearhide.label"></div>
                  <div class="desc" data-i18n="settings.behavior.clearhide.desc"></div>
                </div>
                <button class="toggle" id="clear-on-hide" role="switch" aria-checked="true"></button>
              </div>
            </div>
```

Add `"settings.section.behavior"` to BOTH locale files as well (en: `"Behavior"`, de: `"Verhalten"`) — append next to the keys from Step 1, and re-run the parity test after.

> Reuse existing classes: `.select` (as `#engine`/`#language` use), `.text-input` (as the vault-sequence input uses — confirm its class name in settings.html; if it's a bare `input` with a different class, match that class instead), `.toggle` (aria-checked switch, as vault toggles use), `.segmented` (as `#widths` uses).

- [ ] **Step 4: Wire constants + presets + rendering in settings.js**

In `crates/funke-app/ui/settings.js`, near the other `const` tables (after `WIDTHS`, ~line 19), add:

```javascript
// A short curated font list (name shown, family stored). "" = system default.
const FONTS = [
  ["", () => t("settings.font.system")],
  ["Segoe UI", () => "Segoe UI"],
  ["Cascadia Code", () => "Cascadia Code"],
  ["Consolas", () => "Consolas"],
  ["Georgia", () => "Georgia"],
];

// Built-in looks: each fills the appearance fields; the user still Saves via save().
const PRESETS = {
  default: { font_family: "", font_scale: 1.0, corner_radius: 9, row_density: "comfortable", panel_opacity: 1.0 },
  compact: { font_family: "", font_scale: 0.9, corner_radius: 6, row_density: "compact", panel_opacity: 1.0 },
  terminal: { font_family: "Cascadia Code", font_scale: 0.95, corner_radius: 4, row_density: "compact", panel_opacity: 1.0 },
};
```

In `buildStaticControls` (after the `widths` block, ~line 273), add the segmented controls, sliders, selects, toggles, and presets:

```javascript
  const presets = document.getElementById("presets");
  ["default", "compact", "terminal"].forEach((name) => {
    const el = document.createElement("button");
    el.className = "segment";
    el.dataset.preset = name;
    el.setAttribute("data-i18n", `settings.presets.${name}`);
    el.addEventListener("click", () => save(PRESETS[name]));
    presets.appendChild(el);
  });

  const density = document.getElementById("row-density");
  [["comfortable", "settings.density.comfortable"], ["compact", "settings.density.compact"]].forEach(([val, key]) => {
    const el = document.createElement("button");
    el.className = "segment";
    el.dataset.density = val;
    el.setAttribute("data-i18n", key);
    el.addEventListener("click", () => save({ row_density: val }));
    density.appendChild(el);
  });

  const fontSel = document.getElementById("font-family");
  FONTS.forEach(([value]) => {
    const opt = document.createElement("option");
    opt.value = value;
    fontSel.appendChild(opt); // text filled by relabel() in renderAll
  });
  fontSel.addEventListener("change", () => save({ font_family: fontSel.value }));

  // Sliders commit on release (change), not on every drag (input), to avoid a save per pixel.
  bindSlider("overlay-position", (v) => ({ overlay_position: v / 100 }));
  bindSlider("font-scale", (v) => ({ font_scale: v / 100 }));
  bindSlider("corner-radius", (v) => ({ corner_radius: v }));
  bindSlider("panel-opacity", (v) => ({ panel_opacity: v / 100 }));
  bindSlider("max-rows", (v) => ({ max_visible_rows: v }));

  const placeholder = document.getElementById("placeholder");
  placeholder.addEventListener("change", () => save({ placeholder: placeholder.value }));

  document.getElementById("hide-on-blur").addEventListener("click", () => save({ hide_on_blur: !settings.hide_on_blur }));
  document.getElementById("clear-on-hide").addEventListener("click", () => save({ clear_on_hide: !settings.clear_on_hide }));
```

Add the `bindSlider` helper near the top-level functions (e.g. after `buildStaticControls`):

```javascript
function bindSlider(id, patchFor) {
  const el = document.getElementById(id);
  el.addEventListener("change", () => save(patchFor(Number(el.value))));
}
```

- [ ] **Step 5: Reflect state in renderAll**

In `renderAll` (settings.js ~line 147), after the existing width `.segment` toggle block (~line 181), add rendering for the new controls:

```javascript
  const posSlider = document.getElementById("overlay-position");
  if (document.activeElement !== posSlider) posSlider.value = String(Math.round(settings.overlay_position * 100));
  const scaleSlider = document.getElementById("font-scale");
  if (document.activeElement !== scaleSlider) scaleSlider.value = String(Math.round(settings.font_scale * 100));
  const radiusSlider = document.getElementById("corner-radius");
  if (document.activeElement !== radiusSlider) radiusSlider.value = String(Math.round(settings.corner_radius));
  const opacitySlider = document.getElementById("panel-opacity");
  if (document.activeElement !== opacitySlider) opacitySlider.value = String(Math.round(settings.panel_opacity * 100));
  const rowsSlider = document.getElementById("max-rows");
  if (document.activeElement !== rowsSlider) rowsSlider.value = String(settings.max_visible_rows);

  const fontSel = document.getElementById("font-family");
  if (fontSel.options.length) fontSel.value = settings.font_family;

  const placeholder = document.getElementById("placeholder");
  if (document.activeElement !== placeholder) placeholder.value = settings.placeholder;

  document.querySelectorAll(".segment[data-density]").forEach((el) => {
    el.classList.toggle("active", el.dataset.density === settings.row_density);
  });

  document.getElementById("hide-on-blur").setAttribute("aria-checked", String(settings.hide_on_blur));
  document.getElementById("clear-on-hide").setAttribute("aria-checked", String(settings.clear_on_hide));
```

In `retranslate` (settings.js ~line 108), add a `relabel` for the font select so its option text follows the language:

```javascript
  relabel(document.getElementById("font-family"), FONTS);
```

And in `renderAll`, ensure the font select is labelled on first render — call the same relabel once after building. Simplest: at the end of `buildStaticControls`, after appending FONT options, add `relabel(document.getElementById("font-family"), FONTS);` (the `relabel` fn exists at line ~117).

- [ ] **Step 6: Slider CSS**

In `crates/funke-app/ui/settings.css`, add a minimal themed range style (tokens only):

```css
.slider {
  width: 160px;
  accent-color: var(--accent);
  cursor: pointer;
}
```

(If `.text-input` doesn't already exist for the vault-sequence field, reuse the class that field actually uses instead of inventing one — check settings.css/html for the input class and match it in the HTML from Step 3.)

- [ ] **Step 7: Build, parity test, smoke-run**

Run: `cargo test -p funke-app ui_locales`
Expected: PASS.

Run: `cargo run -p funke-app` — open Settings (type `settings` or the tray), go to Appearance, drag each slider / toggle each switch / pick a preset, and summon the overlay (Ctrl+Space) to confirm each change takes effect live (no restart). Quit.
Expected: position, font, scale, radius, density, opacity, rows, placeholder, hide-on-blur, clear-on-hide all work; presets fill the controls.

- [ ] **Step 8: Commit**

```bash
git add crates/funke-app/ui/settings.html crates/funke-app/ui/settings.js crates/funke-app/ui/settings.css crates/funke-app/ui/locales/en.js crates/funke-app/ui/locales/de.js
git commit -m "feat(ui): appearance & behavior controls with presets in settings"
```

---

### Task 5: Auditing — changelog, version, docs

**Files:**
- Modify: `CHANGELOG.md`
- Modify: `Cargo.toml` (workspace `version`) and any per-crate version that tracks it
- Modify: `docs/DESIGN.md` §7 only if a stated interface rule changed (it does not; the overlay stays created-once, position/appearance are settings)

**Interfaces:** none (documentation/metadata).

- [ ] **Step 1: Bump the version**

Current release is `0.9.0`. This adds a user-facing feature → bump minor to `0.10.0`. In the workspace `Cargo.toml`, find `version = "0.9.0"` (workspace `[workspace.package]` or the app crate) and set it to `0.10.0`. Run `cargo build` afterward so `Cargo.lock` updates.

- [ ] **Step 2: Changelog entry**

In `CHANGELOG.md`, add a new section at the top under the heading style already used:

```markdown
## 0.10.0

### Added
- Overlay customization: vertical position, font family and text size, corner radius, row density, panel opacity, visible-row count, and custom placeholder text — all live, no restart.
- Behavior toggles: hide-on-blur (keep the bar up until Esc) and clear-query-on-hide (preserve the typed query across summons).
- Built-in appearance presets (Default, Compact, Terminal) in Settings → Appearance.
```

- [ ] **Step 3: Full CI gate**

Run all four gates:
```bash
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
cargo build
```
Expected: all clean.

- [ ] **Step 4: Final smoke-run**

Run: `cargo run -p funke-app` — confirm the tray line prints and the app stays alive; summon and quit.

- [ ] **Step 5: Commit**

```bash
git add CHANGELOG.md Cargo.toml Cargo.lock
git commit -m "chore: release 0.10.0 — overlay customization"
```

---

## Self-Review

**Spec coverage:**
- Appearance (font family/size, radius, opacity, density) → Task 3 (CSS/applyTheme) + Task 4 (controls). ✓
- Position & size (vertical position, max rows; width already existed) → Task 2 (position), Task 3 (`--results-max`), Task 4 (controls). ✓
- Behavior (hide-on-blur, placeholder, clear-on-hide) → Task 2 (blur), Task 3 (placeholder + clear-on-hide), Task 4 (controls). ✓
- Theme presets (built-in) → Task 4 `PRESETS`. ✓
- Serde-default back-compat + tests → Task 1. ✓
- i18n parity → Task 4 (both locales + parity test run). ✓
- Auditing (changelog/version) → Task 5. ✓
- Skipped items (native blur, user-named themes, per-monitor) stay out — none scheduled. ✓

**Blur not tunable via CSS** — respected: `panel_opacity` is a tint alpha only (Task 3 sets `--glass-tint`), no CSS blur added.

**Type consistency:** field names (`overlay_position`, `font_scale`, `row_density`, `panel_opacity`, `max_visible_rows`, `hide_on_blur`, `clear_on_hide`, `font_family`, `corner_radius`, `placeholder`) are identical across Task 1 (Rust), Task 3 (JS reads), Task 4 (JS writes). `applyTheme` (Task 3) is the name referenced by Task 3 Step 3. `cfg` set in `applyTheme`, read in listeners. `bindSlider`/`refreshPlaceholder`/`relabel` all defined before use.

**Open verification point flagged for the implementer:** Task 4 reuses class names (`.select`, `.text-input`, `.toggle`, `.segmented`, `.segment.active`) assumed from existing controls — confirm the exact class the vault-sequence text input uses and match it, rather than assuming `.text-input`.
