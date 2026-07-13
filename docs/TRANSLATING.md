# Translating Funke

Everything the user reads that Funke itself wrote lives in a catalogue. Nothing is a literal in
the code — that is an invariant, not a style preference, and it is what makes this document
short.

There are **two catalogues**, because there are two halves of the app that write text:

| | Owns | Files |
|---|---|---|
| **Core** | Everything that arrives inside a result row: provider names, action labels, subtitles, vault and update messages. | `crates/funke-core/locales/<tag>.json` |
| **UI** | Everything the windows write themselves: the settings pane, the key hints, the greeting. | `crates/funke-app/ui/locales/<tag>.js` |

They are separate because they are consumed differently — one is compiled into Rust, the other
is read by the webview — not because the split means anything to a translator. If you are
adding a language, you translate both.

**English is the source of truth.** Every other file is checked against `en` by a test: a key
that exists in one language and not another fails `cargo test`, and so does a translation that
drops or invents a `{placeholder}`.

## Translating an existing language

Open the file, change the values, leave the keys alone. Then:

```bash
cargo test --workspace
```

Three things the tests will not catch, and you should watch for yourself:

- **`{placeholders}` are filled at runtime.** `"Funke {version} is available"` — keep the braces
  and the name inside them exactly. You may move it in the sentence; you may not rename it.
- **A few strings carry inline `<code>` and `<kbd>` markup** (they are rendered with
  `data-i18n-html`). Keep the tags, translate around them. These strings are ours; no user input
  ever reaches that path.
- **Keywords are not words.** `f`, `ff`, `v`, `c`, `s`, `w`, `l`, `g` are typed, not read. Do not
  translate them, even where a sentence mentions one.

## Adding a language

Four steps. Say the tag is `fr`:

1. **`crates/funke-core/locales/fr.json`** — copy `en.json`, translate the values.
2. **`crates/funke-app/ui/locales/fr.js`** — copy `en.js`, translate the values, and change the
   one line that registers it: `window.FUNKE_STRINGS.fr = { … }`.
3. **`crates/funke-core/src/i18n.rs`** — add `Fr` to the `Locale` enum, to `Locale::ALL` (in the
   same order — there is a test), and to `tag()` and `source()`. Each is one line.
4. **`crates/funke-app/ui/`** — add `<script src="locales/fr.js"></script>` to **both**
   `index.html` and `settings.html`, next to the others. (A test fails if you only do one.) Then
   add the language to the `LANGUAGES` list in `settings.js`, so it can be chosen.

`cargo test --workspace` now checks the new file the same way it checks German.

## What the rules are protecting

Two of them are load-bearing enough that breaking one is a bug even if the app still runs.

**Ids are never localized.** A result row's id keys the frecency store and the recents file,
both of which outlive a language change. Build an id out of a title and switching to German
silently orphans everything the user has ever launched. Ids come from a stable key
(`system:lock`); only the title is looked up.

**The English word keeps matching.** A German UI still answers to `settings`, because the fuzzy
matcher scores the localized string *and* the English one and keeps the better (`alias_score`).
Muscle memory is not a language, and neither is the word someone learned from a screenshot.

## Why the files are shaped like this

The UI catalogue is `.js` rather than `.json` because there is **no bundler and no dev server**
in this project on purpose. A `.js` file loads with a plain `<script>` tag, synchronously, before
the first paint. JSON would have to be `fetch`ed — and a catalogue that arrives asynchronously
is a catalogue the first paint renders without, which is a flash of untranslated interface at
every summon.

The core catalogue is JSON, compiled in with `include_str!`. Nothing is read from disk at
runtime, so a shipped Funke cannot be broken by a missing or edited locale file.
