# Writing Funke plugins

Funke plugins are **separate executables** that the launcher runs as child processes
and talks to over **JSON-RPC 2.0 on stdin/stdout** (one JSON object per line). Any
language that can read lines and print JSON can be a plugin — no Rust required, no
dynamic linking, and a crashing plugin can never take the launcher down.

> Why out-of-process? Rust has no stable ABI, so "download a .dll" plugins are off the
> table by design; stdio JSON-RPC is how Flow Launcher/Wox grew their ecosystems. See
> `docs/DESIGN.md` §3.

## Quick start (Rust)

Copy `funke-plugins/template/` — it's a complete plugin in ~60 lines:

```bash
cargo build --release -p funke-plugin-template
```

Install it: create a folder in the plugins directory (open it via
**Settings → Plugins → Open folder**, or `%APPDATA%\funke\plugins`) and copy in the
exe plus its manifest:

```text
%APPDATA%\funke\plugins\
└── template\
    ├── plugin.json
    └── funke-plugin-template.exe
```

Then **Settings → Plugins → Refresh** (or restart Funke) and type `tp hello`.

Rust authors implement two-method trait and are done:

```rust
use funke_plugin::sdk::{serve, Plugin};
use funke_plugin::proto::{PluginInfo, PluginItem, PluginAction};

struct MyPlugin;

impl Plugin for MyPlugin {
    fn info(&self) -> PluginInfo { /* name + version */ }
    fn query(&mut self, text: &str) -> Vec<PluginItem> { /* rows for this keystroke */ }
    fn invoke(&mut self, item_id: &str, action_index: usize) -> Result<(), String> {
        /* the user ran actions[action_index] of your item — do the thing */
    }
}

fn main() -> std::io::Result<()> {
    serve(MyPlugin)
}
```

## Quick start (Python)

No build step — copy `funke-plugins/template-python/`. It's the same demo in ~70 lines of
dependency-free Python:

```text
%APPDATA%\funke\plugins\
└── template-python\
    ├── plugin.json     ← "entry": "run.cmd"
    ├── run.cmd         ← launcher: starts Python on plugin.py
    └── plugin.py       ← your logic (reads/writes line-delimited JSON on stdio)
```

The host runs an *executable*, and a `.py` isn't one — so `run.cmd` is the entry and it
starts `py -3 plugin.py` (edit it to `python` if you don't have the `py` launcher). Python
must be on PATH. Install it via **Settings → Plugins → Open folder**, then hit **Refresh**
(no restart needed) and type `tpy hello`.

The protocol is language-agnostic — anything that reads stdin lines and writes JSON works;
`plugin.py` is just the shortest complete example. See the protocol section below.

## The manifest — `plugin.json`

Lives next to your executable, one folder per plugin:

```json
{
  "id": "template",
  "name": "Template",
  "version": "0.1.0",
  "description": "Shown in Settings → Plugins",
  "entry": "funke-plugin-template.exe",
  "prefix": "tp",
  "prefix_only": true
}
```

| Field | Meaning |
|---|---|
| `id` | Stable identifier (lowercase, no spaces). Provider id becomes `plugin:<id>`. |
| `name` | Display name — the section label above your results and the settings row. |
| `entry` | Executable path relative to the plugin folder. |
| `prefix` | Optional scope keyword: `tp hello` routes only to you, keyword stripped. |
| `prefix_only` | `true` = only answer prefixed queries, never global ones. Use it if your results are noisy or private. |

## The protocol (any language)

Line-delimited JSON-RPC 2.0 on stdio. The host sends requests; you answer with the
same `id`. Protocol version is currently **1**.

**`initialize`** — sent once at startup. Echo your metadata; the `protocol` field must
match the host's or you'll be rejected at load:

```json
← {"jsonrpc":"2.0","id":0,"method":"initialize","params":{"protocol":1}}
→ {"jsonrpc":"2.0","id":0,"result":{"name":"Template","version":"0.1.0","protocol":1}}
```

**`query`** — sent per (debounced) keystroke. Answer with result rows. **Budget:
~100 ms** — the host drops answers that take longer than 300 ms and your rows simply
don't appear for that keystroke:

```json
← {"jsonrpc":"2.0","id":1,"method":"query","params":{"text":"hello"}}
→ {"jsonrpc":"2.0","id":1,"result":{"items":[
     {"id":"upper:hello","title":"HELLO","subtitle":"UPPERCASE — Enter copies",
      "score":10,"actions":[{"label":"Copy","confirm":false}]}
   ]}}
```

- `id` — yours to choose; it comes back verbatim in `invoke`. Encode whatever state
  you need into it (the template packs the query text in).
- `score` — higher ranks higher; built-in fuzzy matches land roughly in 0–200.
- `icon` — optional data URL (`data:image/png;base64,…` or percent-encoded SVG).
- `actions` — labels the user sees. Index 0 runs on Enter, index 1 on Shift+Enter,
  Tab lists all. `"confirm": true` makes the UI demand a second Enter (destructive
  actions). Omit `actions` and the host synthesizes a default "Run".

**`invoke`** — the user picked one of your actions. *You* execute it (open a URL,
copy to the clipboard, call an API…) and answer; an `error` surfaces the message:

```json
← {"jsonrpc":"2.0","id":2,"method":"invoke","params":{"item_id":"upper:hello","action_index":0}}
→ {"jsonrpc":"2.0","id":2,"result":{}}
```

**`shutdown`** — a notification (no response expected). Exit promptly; the host kills
lingering processes.

## Rules of the road

- **Never print anything but protocol JSON to stdout.** Log to stderr; the host
  ignores it.
- **Be fast in `query`.** Index or cache in the background at startup; a query should
  only read memory. Your process stays alive between queries precisely so you can.
- **Spawned lazily**: your process starts on the first query that reaches you (type
  your prefix once), not at launcher startup.
- **Discovery** is at startup, plus **Settings → Plugins → Refresh** for newly added
  plugins. Installing and uninstalling from the catalog take effect live too — your
  process is stopped before your folder is deleted.
- Users can disable you in **Settings → Plugins** (you won't be queried or spawned).

## Distribution

Build your exe, zip it with `plugin.json` inside a folder named after your plugin id,
and users unzip it into the plugins folder. First-party plugins in `funke-plugins/`
ship exactly that way automatically: the release workflow
(`.github/workflows/release.yml`) packages each one as `funke-plugin-<id>-<tag>.zip`
on every tagged GitHub release.

### Getting into the catalog

**Settings → Plugins → Browse** lists a curated catalog and installs from it in one
click. The catalog is [`plugins.json`](../plugins.json) in this repository — to be
listed, open a pull request adding an entry:

```json
{
  "id": "weather",
  "name": "Weather",
  "description": "Current conditions and the forecast for any city",
  "author": "you",
  "homepage": "https://github.com/you/funke-weather",
  "version": "1.0.0",
  "prefix": "wx",
  "url": "https://github.com/you/funke-weather/releases/download/v1.0.0/funke-plugin-weather-v1.0.0.zip",
  "sha256": "b1946ac92492d2347c6235b4d2611184…"
}
```

- Host the zip wherever you like (a GitHub release is easiest) — the URL must be
  `https`, and the archive must contain `<id>/plugin.json` plus your entry point.
- `sha256` is the hash of that zip: `Get-FileHash .\your-plugin.zip -Algorithm SHA256`.
  **It is pinned.** Funke refuses any download that doesn't match, so re-releasing your
  plugin means a new PR with the new hash — that's deliberate: it means the bytes a user
  installs are the bytes that were reviewed.
- Your plugin's `id` in `plugin.json` must equal the `id` in the catalog entry.
- Merging an entry is a review of *your source*. Keep it readable, and expect questions.

None of this sandboxes anything: a plugin is a normal program running with the user's
full rights. The catalog makes a plugin *identifiable*, not safe — which is exactly what
the Plugins pane tells users before they install one.
