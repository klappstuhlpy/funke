# Overlay customization — design

**Date:** 2026-07-22
**Status:** Approved, ready for implementation plan
**Approach:** A — generalize the CSS-token pipe, add built-in presets, no new store.

## Goal

Give the user more control over the summon overlay ("spotlight bar"): its
appearance, its position/size, and a few behavior toggles — reusing the
existing settings → `settings-changed` → live re-theme path rather than
adding new machinery.

## Non-goals (deliberately skipped)

- **Native blur-amount tuning.** The acrylic backdrop is native
  (`apply_acrylic`); design invariant #3 keeps blur out of CSS. Tuning it
  would need a native re-apply on every change. Panel *opacity* is offered
  instead, via the `#panel` CSS tint alpha.
- **User-named, saveable themes** (Approach B). Presets are built-in only.
  Add a store + management UI when someone actually asks to save their own.
- **Per-monitor placement.** Overlay follows the monitor it summons on, as
  today.

## New `Settings` fields

All added to `funke_core::Settings` with `#[serde(default)]` so existing
`settings.json` files load unchanged. Defaults reproduce today's look exactly.

| field | type | default | drives |
|---|---|---|---|
| `overlay_position` | `f64` | `0.24` | vertical screen fraction in `position_overlay` |
| `font_family` | `String` | `""` (empty = built-in stack) | `--font` |
| `font_scale` | `f64` | `1.0` | base font-size multiplier, clamped 0.85–1.3 |
| `corner_radius` | `f64` | `9.0` | `--radius` (and `--radius-s` = radius − 2, floored at 0) |
| `row_density` | `String` | `"comfortable"` | row padding/height token set (`comfortable` \| `compact`) |
| `panel_opacity` | `f64` | `1.0` | `#panel` tint alpha, clamped 0.5–1.0 |
| `max_visible_rows` | `u32` | `8` | `--max-visible-rows` → results list max-height |
| `placeholder` | `String` | `""` (empty = localized default) | input placeholder text |
| `hide_on_blur` | `bool` | `true` | existing hide-on-blur handler |
| `clear_on_hide` | `bool` | `true` | clear the query when the overlay hides |

Defaults set in `Settings::default()` alongside the existing fields.

### Value handling / clamping

- Numeric knobs are clamped at the point of use (Rust for `overlay_position`,
  JS `applyTheme` for the CSS-var knobs) so a hand-edited `settings.json`
  can't produce an unusable overlay. `overlay_position` clamped 0.0–0.9.
- Empty string sentinels (`font_family`, `placeholder`) fall back to the
  built-in stack / localized default — never render an empty control.

## Overlay side (`crates/funke-app/ui/main.js`, `style.css`)

- Rename `applyAccent(settings)` → `applyTheme(settings)`. It keeps the
  existing accent math and additionally `setProperty`s:
  `--font` (when `font_family` non-empty), a font-size base scaled by
  `font_scale`, `--radius` / `--radius-s` from `corner_radius`, the density
  token set from `row_density`, `#panel` background alpha from
  `panel_opacity`, and `--max-visible-rows` from `max_visible_rows`.
- Already called on load and on `settings-changed`, so live re-theme is free —
  no new listener.
- `row_density` toggles a small set of spacing tokens (row padding, row
  min-height). Two named sets defined in `:root` / applied via a class or
  var group; `applyTheme` selects between them.
- `--max-visible-rows` drives a `max-height` on the results scroll container
  (rows × row-height). Beyond it the existing themed scrollbar takes over.
  The window-height-follows-content rule (invariant #4) still holds: the panel
  measures and calls `resize_overlay`; the cap only bounds the results list.
- Placeholder: `applyTheme` sets `input.placeholder` to `settings.placeholder`
  or the localized default when empty.
- `hide_on_blur` / `clear_on_hide`: read from the settings object the overlay
  already receives. The blur handler checks `hide_on_blur` before hiding
  (respecting the existing `hello_in_flight` suppression). On hide, the query
  is cleared only when `clear_on_hide` is true.

## Rust side (`crates/funke-app/src/main.rs`)

- `position_overlay` reads `overlay_position` (clamped) in place of the
  hardcoded `0.24`.
- Nothing else changes: `overlay_width` is already wired through
  `resize_overlay`; the rest is CSS-var driven and needs no Rust.

## Presets (`settings.js`)

- Three buttons in the Appearance section: **Default**, **Compact**,
  **Terminal**.
- Each preset is a fixed JS object of the appearance-field values. Clicking
  writes those values into the settings controls; the user still presses Save
  (consistent with the rest of the settings pane). No store, no persistence of
  its own.
- Default = the field defaults above. Compact = smaller font_scale, compact
  density, tighter radius. Terminal = monospace font_family, compact density,
  small radius. Exact values decided during implementation.

## Settings UI (`settings.html` / `settings.js` / `settings.css`)

- New **Appearance** section grouping the new controls (and the existing
  accent + width, moved in for coherence — or left in place and referenced;
  decided during implementation to keep the diff small).
- Controls follow the existing accent/width patterns: range sliders for
  numeric knobs, a select for `font_family` (a short curated list + "System
  default"), a select for `row_density`, text input for `placeholder`,
  checkboxes for `hide_on_blur` / `clear_on_hide`.
- Any new user-visible string goes through both i18n catalogues
  (`funke-core/locales/*.json` if core-produced — none expected here — and
  `ui/locales/*.js` for UI strings), keeping the parity tests green.

## Testing / verification

- Core: serde round-trip test proving an old `settings.json` (without the new
  keys) loads with the documented defaults, and that the new fields serialize.
- i18n: existing parity tests cover any new UI keys (missing/duplicate/dropped
  placeholder fails `cargo test`).
- The four CI gates stay clean: `fmt --check`, `clippy -D warnings`, `test`,
  `build`.
- Overlay can't be exercised headlessly: after the app-crate changes,
  smoke-run the binary — it must print the tray line and stay alive — and
  eyeball the overlay + settings pane.

## Auditing

Per CLAUDE.md: update `CHANGELOG.md` and the version, and note the new
settings surface. `docs/DESIGN.md` is a record, not a roadmap — update §7
(the interface's rules) only if a customization changes a stated rule;
otherwise leave it.
