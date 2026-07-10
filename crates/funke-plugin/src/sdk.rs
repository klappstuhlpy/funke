//! The plugin author's half: implement [`Plugin`], hand it to [`serve`] in `main`,
//! and the loop speaks the protocol for you. See `funke-plugins/template` for a
//! complete working plugin built on this.

use std::io::{BufRead, Write};

use serde_json::Value;

use crate::proto::{
    InvokeParams, PluginInfo, PluginItem, QueryParams, QueryResult, Request, Response, PROTOCOL_VERSION,
};

pub trait Plugin {
    /// Name + version reported in the `initialize` handshake.
    fn info(&self) -> PluginInfo;

    /// One search. Keep it fast (< ~100 ms) — the host times slow plugins out and
    /// their results simply vanish from that keystroke.
    fn query(&mut self, text: &str) -> Vec<PluginItem>;

    /// The user ran `actions[action_index]` of the item you returned as `item_id`.
    /// The plugin executes it itself; return an error string to surface a failure.
    fn invoke(&mut self, item_id: &str, action_index: usize) -> Result<(), String>;
}

/// Blocking serve loop over stdin/stdout. Returns when the host sends `shutdown`
/// or closes the pipe.
pub fn serve(mut plugin: impl Plugin) -> std::io::Result<()> {
    let stdin = std::io::stdin();
    let mut stdout = std::io::stdout();
    for line in stdin.lock().lines() {
        let line = line?;
        if line.trim().is_empty() {
            continue;
        }
        let Ok(request) = serde_json::from_str::<Request>(&line) else {
            continue; // not ours to crash over
        };
        if request.method == "shutdown" {
            return Ok(());
        }
        let response = dispatch(&mut plugin, &request);
        serde_json::to_writer(&mut stdout, &response)?;
        stdout.write_all(b"\n")?;
        stdout.flush()?;
    }
    Ok(())
}

fn dispatch(plugin: &mut impl Plugin, request: &Request) -> Response {
    match request.method.as_str() {
        "initialize" => {
            let mut info = plugin.info();
            info.protocol = PROTOCOL_VERSION;
            Response::ok(request.id, serde_json::to_value(info).expect("info serializes"))
        }
        "query" => match serde_json::from_value::<QueryParams>(request.params.clone()) {
            Ok(params) => {
                let items = plugin.query(&params.text);
                Response::ok(
                    request.id,
                    serde_json::to_value(QueryResult { items }).expect("items serialize"),
                )
            }
            Err(e) => Response::err(request.id, format!("bad query params: {e}")),
        },
        "invoke" => match serde_json::from_value::<InvokeParams>(request.params.clone()) {
            Ok(params) => match plugin.invoke(&params.item_id, params.action_index) {
                Ok(()) => Response::ok(request.id, Value::Object(Default::default())),
                Err(e) => Response::err(request.id, e),
            },
            Err(e) => Response::err(request.id, format!("bad invoke params: {e}")),
        },
        other => Response::err(request.id, format!("unknown method: {other}")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Echo;

    impl Plugin for Echo {
        fn info(&self) -> PluginInfo {
            PluginInfo {
                name: "Echo".into(),
                version: "0.0.1".into(),
                protocol: 0, // serve() stamps the real one
            }
        }

        fn query(&mut self, text: &str) -> Vec<PluginItem> {
            vec![PluginItem {
                id: "echo".into(),
                title: text.to_string(),
                subtitle: None,
                icon: None,
                score: 1,
                actions: vec![],
            }]
        }

        fn invoke(&mut self, item_id: &str, _action_index: usize) -> Result<(), String> {
            if item_id == "echo" {
                Ok(())
            } else {
                Err("unknown item".into())
            }
        }
    }

    #[test]
    fn dispatch_covers_the_protocol() {
        let mut plugin = Echo;

        let response = dispatch(&mut plugin, &Request::new(1, "initialize", Value::Null));
        let info: PluginInfo = serde_json::from_value(response.result.unwrap()).unwrap();
        assert_eq!(info.protocol, PROTOCOL_VERSION);

        let response = dispatch(
            &mut plugin,
            &Request::new(2, "query", serde_json::json!({"text":"hey"})),
        );
        let result: QueryResult = serde_json::from_value(response.result.unwrap()).unwrap();
        assert_eq!(result.items[0].title, "hey");

        let response = dispatch(
            &mut plugin,
            &Request::new(3, "invoke", serde_json::json!({"item_id":"nope","action_index":0})),
        );
        assert!(response.error.is_some());

        let response = dispatch(&mut plugin, &Request::new(4, "dance", Value::Null));
        assert!(response.error.unwrap().message.contains("unknown method"));
    }
}
