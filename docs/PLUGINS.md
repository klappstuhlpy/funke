# Writing Funke plugins

Funke plugins are **separate executables** that the launcher runs as child processes
and talks to over **JSON-RPC 2.0 on stdin/stdout** (one JSON object per line). Any
language that can read lines and print JSON can be a plugin — no Rust required, no
dynamic linking, and a crashing plugin can never take the launcher down.

> Why out-of-process? Rust has no stable ABI, so "download a .dll" plugins are off the
> table by design; stdio JSON-RPC is how Flow Launcher/Wox grew their ecosystems. See
> `docs/PLAN.md` §2.

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

Restart Funke (plugins are discovered at startup), then type `tp hello`.

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
- Users can disable you in **Settings → Plugins** (you won't be queried or spawned).

## Distribution

Build your exe, zip it with `plugin.json` inside a folder named after your plugin id,
and users unzip it into the plugins folder. First-party plugins in `funke-plugins/`
ship exactly that way automatically: the release workflow
(`.github/workflows/release.yml`) packages each one as `funke-plugin-<id>-<tag>.zip`
on every tagged GitHub release. Planned next: a suggested-plugins catalog inside
Settings → Plugins that downloads them for you. Until a code-signing story exists,
treat any plugin like any other executable you'd run — it runs with your user's
permissions.
