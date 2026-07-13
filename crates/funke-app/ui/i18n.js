// The frontend's half of the string catalogue — everything the UI writes itself: the key hints,
// the settings pane, the greeting. The other half (funke-core's `i18n`, backed by
// crates/funke-core/locales/) owns everything that arrives inside a ResultItem.
//
// The strings live in `locales/<tag>.js`, one file per language, each registering itself into
// `window.FUNKE_STRINGS`. They are plain <script> tags loaded before this file — deliberately
// not fetched and not imported: there is no bundler here, and a catalogue that arrives
// asynchronously is a catalogue the first paint renders without. Adding a language is a file
// next to the others plus a <script> tag in both HTML pages; see docs/TRANSLATING.md.
//
// Static text lives in the markup behind a `data-i18n` key and is filled in by
// `applyTranslations` — a translator sees it in context, and the HTML stays the layout it is.
// Text assembled at runtime calls `t(key, args)`.

const STRINGS = window.FUNKE_STRINGS || { en: {} };

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
  // Only for the handful of strings that carry <code>/<kbd> markup, and only from the locale
  // files — never from settings, a plugin, or anything else the user can put words into.
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
