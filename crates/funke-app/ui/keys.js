// Keycaps, shared by both windows (styling in keys.css).
//
// A chord is written the way a hotkey is written everywhere else in Funke — the settings
// file, the recorder, `GlobalShortcutExt` — as plus-joined key names: "Ctrl+Shift+Enter".
// One string, one spelling, and this file is the only place that knows how a key is drawn.
//
// Static markup asks for a chord with `data-keys` and `applyKeycaps()` fills it in, the way
// `data-i18n` works; anything assembled at runtime calls `keycaps(chord)` for the element.

// What a key looks like on a Windows keyboard. Keys whose glyph is printed on the physical
// cap wear it; modifiers keep their words, because ⌃⌥⌘ are not on this keyboard.
const KEYCAPS = {
  ctrl: "Ctrl",
  control: "Ctrl",
  alt: "Alt",
  shift: "⇧",
  super: "Win",
  meta: "Win",
  win: "Win",
  enter: "↵",
  return: "↵",
  tab: "⇥",
  esc: "Esc",
  escape: "Esc",
  space: "Space",
  up: "↑",
  down: "↓",
  left: "←",
  right: "→",
  backspace: "⌫",
  delete: "⌦",
  pageup: "⇞",
  pagedown: "⇟",
  home: "↖",
  end: "↘",
  comma: ",",
  period: ".",
  minus: "−",
  equal: "=",
  slash: "/",
  backslash: "\\",
  semicolon: ";",
  quote: "'",
  backquote: "`",
  bracketleft: "[",
  bracketright: "]",
};

// The caps of one chord, in press order. A key the table doesn't know (a letter, a digit,
// F5) is drawn as itself — the table is a spelling aid, not a whitelist.
function capsFor(chord) {
  return String(chord)
    .split("+")
    .map((key) => key.trim())
    .filter(Boolean)
    .map((key) => {
      const glyph = KEYCAPS[key.toLowerCase()] || key;
      const cap = document.createElement("kbd");
      cap.className = [...glyph].length === 1 ? "cap glyph" : "cap word";
      cap.textContent = glyph;
      return cap;
    });
}

function keycaps(chord) {
  const group = document.createElement("span");
  group.className = "chord";
  group.append(...capsFor(chord));
  return group;
}

// Draw the chord into an element that already exists (and re-draw it: the caps are replaced,
// never appended, so this is safe to call again after a hotkey changes).
function fillKeycaps(el, chord) {
  el.classList.add("chord");
  el.replaceChildren(...capsFor(chord));
}

function applyKeycaps(root = document) {
  root.querySelectorAll("[data-keys]").forEach((el) => fillKeycaps(el, el.dataset.keys));
}
