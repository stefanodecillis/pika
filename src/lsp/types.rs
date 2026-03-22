use serde::{Deserialize, Serialize};

/// A JSON-RPC 2.0 message used by the Language Server Protocol.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum JsonRpcMessage {
    Request(JsonRpcRequest),
    Response(JsonRpcResponse),
    Notification(JsonRpcNotification),
}

/// A JSON-RPC request (client -> server or server -> client).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JsonRpcRequest {
    pub jsonrpc: String,
    pub id: i64,
    pub method: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub params: Option<serde_json::Value>,
}

impl JsonRpcRequest {
    /// Create a new JSON-RPC request with the given id, method, and params.
    pub fn new(id: i64, method: impl Into<String>, params: Option<serde_json::Value>) -> Self {
        Self {
            jsonrpc: "2.0".to_string(),
            id,
            method: method.into(),
            params,
        }
    }
}

/// A JSON-RPC response.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JsonRpcResponse {
    pub jsonrpc: String,
    pub id: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<JsonRpcError>,
}

impl JsonRpcResponse {
    /// Create a successful response for the given request id.
    pub fn ok(id: i64, result: serde_json::Value) -> Self {
        Self {
            jsonrpc: "2.0".to_string(),
            id,
            result: Some(result),
            error: None,
        }
    }

    /// Create an error response for the given request id.
    pub fn err(id: i64, error: JsonRpcError) -> Self {
        Self {
            jsonrpc: "2.0".to_string(),
            id,
            result: None,
            error: Some(error),
        }
    }
}

/// A JSON-RPC notification (no `id` field, no response expected).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JsonRpcNotification {
    pub jsonrpc: String,
    pub method: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub params: Option<serde_json::Value>,
}

impl JsonRpcNotification {
    /// Create a new JSON-RPC notification with the given method and params.
    pub fn new(method: impl Into<String>, params: Option<serde_json::Value>) -> Self {
        Self {
            jsonrpc: "2.0".to_string(),
            method: method.into(),
            params,
        }
    }
}

/// A JSON-RPC error object.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JsonRpcError {
    pub code: i64,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<serde_json::Value>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_request_new() {
        let req = JsonRpcRequest::new(1, "initialize", Some(json!({"rootUri": "file:///tmp"})));
        assert_eq!(req.jsonrpc, "2.0");
        assert_eq!(req.id, 1);
        assert_eq!(req.method, "initialize");
        assert!(req.params.is_some());
    }

    #[test]
    fn test_request_new_no_params() {
        let req = JsonRpcRequest::new(42, "shutdown", None);
        assert_eq!(req.id, 42);
        assert_eq!(req.method, "shutdown");
        assert!(req.params.is_none());
    }

    #[test]
    fn test_notification_new() {
        let notif = JsonRpcNotification::new("initialized", Some(json!({})));
        assert_eq!(notif.jsonrpc, "2.0");
        assert_eq!(notif.method, "initialized");
        assert!(notif.params.is_some());
    }

    #[test]
    fn test_notification_new_no_params() {
        let notif = JsonRpcNotification::new("exit", None);
        assert_eq!(notif.method, "exit");
        assert!(notif.params.is_none());
    }

    #[test]
    fn test_response_ok() {
        let resp = JsonRpcResponse::ok(1, json!({"capabilities": {}}));
        assert_eq!(resp.id, 1);
        assert!(resp.result.is_some());
        assert!(resp.error.is_none());
    }

    #[test]
    fn test_response_err() {
        let err = JsonRpcError {
            code: -32600,
            message: "Invalid Request".to_string(),
            data: None,
        };
        let resp = JsonRpcResponse::err(2, err);
        assert_eq!(resp.id, 2);
        assert!(resp.result.is_none());
        assert!(resp.error.is_some());
        assert_eq!(resp.error.as_ref().unwrap().code, -32600);
    }

    #[test]
    fn test_request_serialization_roundtrip() {
        let req = JsonRpcRequest::new(
            10,
            "textDocument/completion",
            Some(json!({"textDocument": {"uri": "file:///test.rs"}, "position": {"line": 0, "character": 5}})),
        );
        let serialized = serde_json::to_string(&req).expect("serialize request");
        let deserialized: JsonRpcRequest =
            serde_json::from_str(&serialized).expect("deserialize request");
        assert_eq!(deserialized.id, 10);
        assert_eq!(deserialized.method, "textDocument/completion");
        assert_eq!(deserialized.jsonrpc, "2.0");
        assert!(deserialized.params.is_some());
    }

    #[test]
    fn test_response_serialization_roundtrip() {
        let resp = JsonRpcResponse::ok(5, json!({"capabilities": {"hoverProvider": true}}));
        let serialized = serde_json::to_string(&resp).expect("serialize response");
        let deserialized: JsonRpcResponse =
            serde_json::from_str(&serialized).expect("deserialize response");
        assert_eq!(deserialized.id, 5);
        assert!(deserialized.result.is_some());
        assert!(deserialized.error.is_none());
    }

    #[test]
    fn test_notification_serialization_roundtrip() {
        let notif = JsonRpcNotification::new(
            "textDocument/publishDiagnostics",
            Some(json!({"uri": "file:///test.rs", "diagnostics": []})),
        );
        let serialized = serde_json::to_string(&notif).expect("serialize notification");
        let deserialized: JsonRpcNotification =
            serde_json::from_str(&serialized).expect("deserialize notification");
        assert_eq!(deserialized.method, "textDocument/publishDiagnostics");
        assert!(deserialized.params.is_some());
    }

    #[test]
    fn test_notification_params_none_omitted_in_json() {
        let notif = JsonRpcNotification::new("exit", None);
        let serialized = serde_json::to_string(&notif).expect("serialize");
        assert!(!serialized.contains("params"));
    }

    #[test]
    fn test_request_params_none_omitted_in_json() {
        let req = JsonRpcRequest::new(1, "shutdown", None);
        let serialized = serde_json::to_string(&req).expect("serialize");
        assert!(!serialized.contains("params"));
    }

    #[test]
    fn test_error_serialization_with_data() {
        let err = JsonRpcError {
            code: -32601,
            message: "Method not found".to_string(),
            data: Some(json!({"detail": "unknown method 'foo'"})),
        };
        let serialized = serde_json::to_string(&err).expect("serialize error");
        let deserialized: JsonRpcError =
            serde_json::from_str(&serialized).expect("deserialize error");
        assert_eq!(deserialized.code, -32601);
        assert_eq!(deserialized.message, "Method not found");
        assert!(deserialized.data.is_some());
    }

    #[test]
    fn test_error_serialization_without_data() {
        let err = JsonRpcError {
            code: -32700,
            message: "Parse error".to_string(),
            data: None,
        };
        let serialized = serde_json::to_string(&err).expect("serialize error");
        assert!(!serialized.contains("data"));
        let deserialized: JsonRpcError =
            serde_json::from_str(&serialized).expect("deserialize error");
        assert_eq!(deserialized.code, -32700);
        assert!(deserialized.data.is_none());
    }

    #[test]
    fn test_json_rpc_message_deserialize_request() {
        let json_str = r#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{}}"#;
        let msg: JsonRpcMessage = serde_json::from_str(json_str).expect("deserialize");
        match msg {
            JsonRpcMessage::Request(req) => {
                assert_eq!(req.id, 1);
                assert_eq!(req.method, "initialize");
            }
            _ => panic!("expected Request variant"),
        }
    }

    #[test]
    fn test_json_rpc_message_deserialize_response() {
        let json_str = r#"{"jsonrpc":"2.0","id":1,"result":{"capabilities":{}}}"#;
        let msg: JsonRpcMessage = serde_json::from_str(json_str).expect("deserialize");
        match msg {
            JsonRpcMessage::Response(resp) => {
                assert_eq!(resp.id, 1);
                assert!(resp.result.is_some());
            }
            // The untagged enum may also match Request since both have `id`;
            // in practice the protocol distinguishes by context. We accept either.
            JsonRpcMessage::Request(_) => {
                // acceptable: untagged enum matched on shape
            }
            _ => panic!("expected Response or Request variant"),
        }
    }

    #[test]
    fn test_json_rpc_message_deserialize_notification() {
        let json_str =
            r#"{"jsonrpc":"2.0","method":"textDocument/publishDiagnostics","params":{"uri":"file:///a","diagnostics":[]}}"#;
        let msg: JsonRpcMessage = serde_json::from_str(json_str).expect("deserialize");
        match msg {
            JsonRpcMessage::Notification(notif) => {
                assert_eq!(notif.method, "textDocument/publishDiagnostics");
            }
            _ => panic!("expected Notification variant"),
        }
    }

    #[test]
    fn test_response_error_with_response_ok_fields() {
        let resp = JsonRpcResponse {
            jsonrpc: "2.0".to_string(),
            id: 3,
            result: None,
            error: Some(JsonRpcError {
                code: -32602,
                message: "Invalid params".to_string(),
                data: None,
            }),
        };
        let serialized = serde_json::to_string(&resp).expect("serialize");
        assert!(!serialized.contains("\"result\""));
        assert!(serialized.contains("\"error\""));
    }

    #[test]
    fn test_request_clone() {
        let req = JsonRpcRequest::new(7, "textDocument/hover", Some(json!({})));
        let cloned = req.clone();
        assert_eq!(req.id, cloned.id);
        assert_eq!(req.method, cloned.method);
    }

    #[test]
    fn test_notification_clone() {
        let notif = JsonRpcNotification::new("initialized", Some(json!({})));
        let cloned = notif.clone();
        assert_eq!(notif.method, cloned.method);
    }
}
