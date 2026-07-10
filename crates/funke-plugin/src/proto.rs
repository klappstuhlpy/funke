//! Wire types: JSON-RPC 2.0, one JSON object per line, over the plugin's stdio.
//!
//! Methods (host → plugin): `initialize`, `query {text}`, `invoke {item_id,
//! action_index}`, `shutdown` (a notification — no response expected).

use serde::{Deserialize, Serialize};
use serde_json::Value;

pub const PROTOCOL_VERSION: u32 = 1;

#[derive(Debug, Serialize, Deserialize)]
pub struct Request {
    pub jsonrpc: String,
    pub id: u64,
    pub method: String,
    #[serde(default)]
    pub params: Value,
}

impl Request {
    pub fn new(id: u64, method: &str, params: Value) -> Self {
        Self {
            jsonrpc: "2.0".into(),
            id,
            method: method.into(),
            params,
        }
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Response {
    pub jsonrpc: String,
    pub id: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub result: Option<Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<RpcError>,
}

impl Response {
    pub fn ok(id: u64, result: Value) -> Self {
        Self {
            jsonrpc: "2.0".into(),
            id,
            result: Some(result),
            error: None,
        }
    }

    pub fn err(id: u64, message: impl Into<String>) -> Self {
        Self {
            jsonrpc: "2.0".into(),
            id,
            result: None,
            error: Some(RpcError {
                code: -32000,
                message: message.into(),
            }),
        }
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub struct RpcError {
    pub code: i64,
    pub message: String,
}

/// `initialize` result: how the plugin introduces itself. The manifest carries the
/// same fields; the handshake exists so the plugin can set itself up and so version
/// mismatches fail loudly at load instead of quietly at query time.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginInfo {
    pub name: String,
    pub version: String,
    #[serde(default)]
    pub protocol: u32,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct QueryParams {
    pub text: String,
}

#[derive(Debug, Default, Serialize, Deserialize)]
pub struct QueryResult {
    pub items: Vec<PluginItem>,
}

/// One result row, mirroring `funke_core::ResultItem` minus everything host-side
/// (provider id, real `Action`s — the host synthesizes `PluginInvoke` routes).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginItem {
    pub id: String,
    pub title: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub subtitle: Option<String>,
    /// Data URL (`data:image/png;base64,…` or inline SVG), same contract as core.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub icon: Option<String>,
    pub score: i64,
    #[serde(default)]
    pub actions: Vec<PluginAction>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginAction {
    pub label: String,
    #[serde(default)]
    pub confirm: bool,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct InvokeParams {
    pub item_id: String,
    pub action_index: usize,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn requests_and_responses_round_trip_as_single_lines() {
        let request = Request::new(7, "query", serde_json::json!({ "text": "hi" }));
        let line = serde_json::to_string(&request).unwrap();
        assert!(!line.contains('\n'));
        let parsed: Request = serde_json::from_str(&line).unwrap();
        assert_eq!(parsed.id, 7);
        assert_eq!(parsed.method, "query");

        let response = Response::ok(7, serde_json::to_value(QueryResult::default()).unwrap());
        let parsed: Response = serde_json::from_str(&serde_json::to_string(&response).unwrap()).unwrap();
        assert!(parsed.error.is_none());
    }

    #[test]
    fn items_tolerate_missing_optional_fields() {
        let item: PluginItem = serde_json::from_str(r#"{"id":"a","title":"T","score":5}"#).unwrap();
        assert!(item.actions.is_empty());
        assert!(item.subtitle.is_none());
    }
}
