//! The template plugin: `tp <text>` offers transformed variants of what you typed,
//! Enter copies one. Small on purpose — this is the file docs/PLUGINS.md walks
//! through, and the folder plugin authors copy to start their own.

use funke_plugin::proto::{PluginAction, PluginInfo, PluginItem};
use funke_plugin::sdk::{serve, Plugin};

struct Template;

/// (row id, row title, transform)
type Transform = (&'static str, &'static str, fn(&str) -> String);

const TRANSFORMS: &[Transform] = &[
    ("upper", "UPPERCASE", |text| text.to_uppercase()),
    ("lower", "lowercase", |text| text.to_lowercase()),
    ("reverse", "esreveR", |text| text.chars().rev().collect()),
];

impl Plugin for Template {
    fn info(&self) -> PluginInfo {
        PluginInfo {
            name: "Template".into(),
            version: env!("CARGO_PKG_VERSION").into(),
            protocol: 0, // stamped by serve()
        }
    }

    fn query(&mut self, text: &str) -> Vec<PluginItem> {
        TRANSFORMS
            .iter()
            .enumerate()
            .map(|(rank, (id, label, transform))| PluginItem {
                // Encode the query into the row id so invoke() can recompute the value.
                id: format!("{id}:{text}"),
                title: transform(text),
                subtitle: Some(format!("{label} — Enter copies")),
                icon: None,
                score: 10 - rank as i64,
                actions: vec![PluginAction {
                    label: "Copy".into(),
                    confirm: false,
                }],
            })
            .collect()
    }

    fn invoke(&mut self, item_id: &str, _action_index: usize) -> Result<(), String> {
        let (kind, text) = item_id.split_once(':').ok_or("malformed item id")?;
        let transform = TRANSFORMS
            .iter()
            .find(|(id, ..)| *id == kind)
            .map(|(.., transform)| transform)
            .ok_or("unknown item")?;
        arboard::Clipboard::new()
            .and_then(|mut clipboard| clipboard.set_text(transform(text)))
            .map_err(|e| format!("copy failed: {e}"))
    }
}

fn main() -> std::io::Result<()> {
    serve(Template)
}
