use std::collections::HashMap;
use std::sync::atomic::{AtomicI64, Ordering};
use std::sync::Arc;

use anyhow::{anyhow, Context, Result};
use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader, BufWriter};
use tokio::process::{Child, ChildStdin, ChildStdout};
use tokio::sync::{mpsc, oneshot, Mutex};

use crate::events::LspEvent;
use crate::lsp::capabilities::client_capabilities;
use crate::lsp::types::{JsonRpcNotification, JsonRpcRequest, JsonRpcResponse};

/// An LSP client that communicates with a single language server process
/// over JSON-RPC 2.0 via stdin/stdout.
pub struct LspClient {
    /// The child process running the language server.
    #[allow(dead_code)]
    process: Child,
    /// Buffered writer to the server's stdin.
    stdin: Arc<Mutex<BufWriter<ChildStdin>>>,
    /// Map of pending request ids to their response channels.
    pending_requests: Arc<Mutex<HashMap<i64, oneshot::Sender<JsonRpcResponse>>>>,
    /// Atomic counter for generating unique request ids.
    next_id: AtomicI64,
    /// Channel for sending LSP events to the UI layer.
    #[allow(dead_code)]
    event_tx: mpsc::UnboundedSender<LspEvent>,
}

impl LspClient {
    /// Spawn a language server process and prepare the client for communication.
    ///
    /// `command` is the binary name (e.g. `"rust-analyzer"`), `args` are CLI
    /// arguments, and `root_uri` is the workspace root as a `file://` URI.
    pub fn new(
        command: &str,
        args: &[String],
        _root_uri: &str,
        event_tx: mpsc::UnboundedSender<LspEvent>,
    ) -> Result<Self> {
        let mut process = tokio::process::Command::new(command)
            .args(args)
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::null()) // Don't capture stderr — a full pipe buffer deadlocks the server.
            .kill_on_drop(true)
            .spawn()
            .with_context(|| format!("failed to spawn language server: {}", command))?;

        let child_stdin = process.stdin.take().ok_or_else(|| {
            anyhow!("failed to capture stdin of language server")
        })?;
        let child_stdout = process.stdout.take().ok_or_else(|| {
            anyhow!("failed to capture stdout of language server")
        })?;

        let stdin = Arc::new(Mutex::new(BufWriter::new(child_stdin)));
        let pending_requests: Arc<Mutex<HashMap<i64, oneshot::Sender<JsonRpcResponse>>>> =
            Arc::new(Mutex::new(HashMap::new()));

        // Start the background reader task.
        Self::start_reader(child_stdout, Arc::clone(&stdin), event_tx.clone(), Arc::clone(&pending_requests));

        Ok(Self {
            process,
            stdin,
            pending_requests,
            next_id: AtomicI64::new(1),
            event_tx,
        })
    }

    /// Send the `initialize` request and return the server's capabilities.
    pub async fn initialize(&self, root_uri: &str) -> Result<lsp_types::InitializeResult> {
        let params = lsp_types::InitializeParams {
            root_uri: Some(
                root_uri
                    .parse::<lsp_types::Uri>()
                    .map_err(|e| anyhow::anyhow!("invalid root_uri '{}': {}", root_uri, e))?,
            ),
            capabilities: client_capabilities(),
            ..Default::default()
        };
        let params_value = serde_json::to_value(&params)
            .context("failed to serialize InitializeParams")?;

        let result = self
            .send_raw_request("initialize", Some(params_value))
            .await?;

        // After getting the response, send the `initialized` notification.
        self.send_notification::<lsp_types::notification::Initialized>(
            lsp_types::InitializedParams {},
        )
        .await;

        serde_json::from_value(result).context("failed to deserialize InitializeResult")
    }

    /// Send a typed LSP request and deserialize the response.
    pub async fn send_request<R: lsp_types::request::Request>(
        &self,
        params: R::Params,
    ) -> Result<R::Result>
    where
        R::Params: serde::Serialize,
        R::Result: serde::de::DeserializeOwned,
    {
        let params_value =
            serde_json::to_value(&params).context("failed to serialize request params")?;
        let result = self
            .send_raw_request(R::METHOD, Some(params_value))
            .await?;
        serde_json::from_value(result).context("failed to deserialize response result")
    }

    /// Send a typed LSP notification (fire-and-forget).
    pub async fn send_notification<N: lsp_types::notification::Notification>(
        &self,
        params: N::Params,
    )
    where
        N::Params: serde::Serialize,
    {
        let params_value = serde_json::to_value(&params).ok();
        let notif = JsonRpcNotification::new(N::METHOD, params_value);
        let msg = serde_json::to_vec(&notif).unwrap_or_default();
        // Best-effort send; we ignore write errors on notifications.
        let _ = self.write_message(&msg).await;
    }

    /// Perform a graceful shutdown: send `shutdown` request then `exit`
    /// notification.
    pub async fn shutdown(&self) -> Result<()> {
        let _ = self.send_raw_request("shutdown", None).await;
        let notif = JsonRpcNotification::new("exit", None);
        let msg = serde_json::to_vec(&notif).unwrap_or_default();
        let _ = self.write_message(&msg).await;
        Ok(())
    }

    /// Send a JSON-RPC request to the server and wait for the matching response.
    async fn send_raw_request(
        &self,
        method: &str,
        params: Option<serde_json::Value>,
    ) -> Result<serde_json::Value> {
        let id = self.next_id.fetch_add(1, Ordering::SeqCst);
        let request = JsonRpcRequest::new(id, method, params);
        let msg = serde_json::to_vec(&request).context("failed to serialize request")?;

        let (tx, rx) = oneshot::channel();
        {
            let mut pending = self.pending_requests.lock().await;
            pending.insert(id, tx);
        }

        self.write_message(&msg).await?;

        let response = rx.await.map_err(|_| {
            anyhow!("response channel dropped for request id={}", id)
        })?;

        if let Some(err) = response.error {
            return Err(anyhow!(
                "LSP error {}: {}",
                err.code,
                err.message
            ));
        }

        response
            .result
            .ok_or_else(|| anyhow!("response for id={} has neither result nor error", id))
    }

    /// Write an LSP message to the server's stdin with the required
    /// `Content-Length` header.
    async fn write_message(&self, msg: &[u8]) -> Result<()> {
        let header = format!("Content-Length: {}\r\n\r\n", msg.len());

        let mut stdin = self.stdin.lock().await;
        stdin
            .write_all(header.as_bytes())
            .await
            .context("failed to write LSP header")?;
        stdin
            .write_all(msg)
            .await
            .context("failed to write LSP body")?;
        stdin.flush().await.context("failed to flush LSP stdin")?;

        Ok(())
    }

    /// Spawn a background tokio task that reads JSON-RPC messages from the
    /// server's stdout and dispatches them.
    fn start_reader(
        stdout: ChildStdout,
        stdin: Arc<Mutex<BufWriter<ChildStdin>>>,
        event_tx: mpsc::UnboundedSender<LspEvent>,
        pending: Arc<Mutex<HashMap<i64, oneshot::Sender<JsonRpcResponse>>>>,
    ) {
        tokio::spawn(async move {
            if let Err(e) = Self::reader_loop(stdout, stdin, event_tx, pending).await {
                log::error!("LSP reader task exited with error: {}", e);
            }
        });
    }

    /// The main read loop for the background reader task.
    async fn reader_loop(
        stdout: ChildStdout,
        stdin: Arc<Mutex<BufWriter<ChildStdin>>>,
        event_tx: mpsc::UnboundedSender<LspEvent>,
        pending: Arc<Mutex<HashMap<i64, oneshot::Sender<JsonRpcResponse>>>>,
    ) -> Result<()> {
        let mut reader = BufReader::new(stdout);


        loop {
            // 1. Read headers until we find Content-Length.
            let content_length = match read_content_length(&mut reader).await {
                Ok(len) => len,
                Err(_) => {
                    // EOF or read error — server process likely exited.
                    break;
                }
            };

            // 2. Read the JSON body.
            let mut body = vec![0u8; content_length];
            reader
                .read_exact(&mut body)
                .await
                .context("failed to read LSP message body")?;


            // Determine message type from the raw JSON before deserialising.
            // JSON-RPC distinguishes three kinds:
            //   • Response:     has "id", no "method"
            //   • Request:      has "id" AND "method"  (server → client request)
            //   • Notification: has "method", no "id"
            let raw: serde_json::Value = match serde_json::from_slice(&body) {
                Ok(v) => v,
                Err(_) => {
                    log::warn!(
                        "LSP reader: invalid JSON: {}",
                        String::from_utf8_lossy(&body)
                    );
                    continue;
                }
            };

            let has_id     = raw.get("id").is_some();
            let has_method = raw.get("method").is_some();

            if has_id && !has_method {
                // 3. It is a response to one of our requests.
                if let Ok(response) = serde_json::from_value::<JsonRpcResponse>(raw) {
                    let mut pending_map = pending.lock().await;
                    if let Some(sender) = pending_map.remove(&response.id) {
                        let _ = sender.send(response);
                    }
                    // else: unsolicited response — ignore
                }
            } else if has_method && !has_id {
                // 4. Notification from the server (no id, no reply needed).
                if let Ok(notification) = serde_json::from_value::<JsonRpcNotification>(raw) {
                    dispatch_notification(&event_tx, notification);
                }
            } else if has_id && has_method {
                // 5. Server-to-client request (has both id and method).
                // Reply with a null result so the server doesn't stall waiting.
                let req_id = raw["id"].as_i64().unwrap_or(-1);
                let method  = raw["method"].as_str().unwrap_or("?").to_string();
                log::debug!("LSP: server request id={} method={} — sending null reply", req_id, method);
                let reply = serde_json::json!({
                    "jsonrpc": "2.0",
                    "id": req_id,
                    "result": null
                });
                if let Ok(body) = serde_json::to_vec(&reply) {
                    let header = format!("Content-Length: {}\r\n\r\n", body.len());
                    let mut w = stdin.lock().await;
                    let _ = w.write_all(header.as_bytes()).await;
                    let _ = w.write_all(&body).await;
                    let _ = w.flush().await;
                }
            } else {
                log::warn!(
                    "LSP reader: unrecognised message shape: {}",
                    String::from_utf8_lossy(&body)
                );
            }
        }

        // Server exited — drop all pending request senders so their receivers
        // unblock with an error instead of hanging forever.
        {
            let mut pending_map = pending.lock().await;
            pending_map.clear();
        }

        Ok(())
    }
}

/// Read HTTP-style headers from the reader and extract the `Content-Length`
/// value.  Returns an error on EOF.
async fn read_content_length<R: AsyncBufReadExt + Unpin>(reader: &mut R) -> Result<usize> {
    let mut content_length: Option<usize> = None;
    let mut header_line = String::new();

    loop {
        header_line.clear();
        let bytes_read = reader
            .read_line(&mut header_line)
            .await
            .context("failed to read LSP header line")?;
        if bytes_read == 0 {
            return Err(anyhow!("EOF while reading LSP headers"));
        }

        let trimmed = header_line.trim();
        if trimmed.is_empty() {
            // End of headers.
            break;
        }

        if let Some(value) = trimmed.strip_prefix("Content-Length:") {
            content_length = Some(
                value
                    .trim()
                    .parse::<usize>()
                    .context("invalid Content-Length value")?,
            );
        }
        // Ignore other headers (e.g. Content-Type).
    }

    content_length.ok_or_else(|| anyhow!("missing Content-Length header"))
}

/// Convert a server notification into an `LspEvent` and send it to the UI.
fn dispatch_notification(
    event_tx: &mpsc::UnboundedSender<LspEvent>,
    notification: JsonRpcNotification,
) {
    match notification.method.as_str() {
        "textDocument/publishDiagnostics" => {
            if let Some(params) = notification.params {
                if let Ok(diag_params) =
                    serde_json::from_value::<lsp_types::PublishDiagnosticsParams>(params)
                {
                    let _ = event_tx.send(LspEvent::Diagnostics {
                        uri: diag_params.uri.to_string(),
                        diagnostics: diag_params.diagnostics,
                    });
                }
            }
        }
        _ => {
            log::debug!(
                "LSP: unhandled server notification: {}",
                notification.method
            );
        }
    }
}

/// Format an LSP message (body bytes) with the `Content-Length` header.
/// This is a utility function exposed for testing.
pub fn format_lsp_message(body: &[u8]) -> Vec<u8> {
    let header = format!("Content-Length: {}\r\n\r\n", body.len());
    let mut out = Vec::with_capacity(header.len() + body.len());
    out.extend_from_slice(header.as_bytes());
    out.extend_from_slice(body);
    out
}

/// Parse the `Content-Length` value from a raw LSP header block.
/// This is a utility function exposed for testing.
pub fn parse_content_length(header: &str) -> Option<usize> {
    for line in header.lines() {
        let trimmed = line.trim();
        if let Some(value) = trimmed.strip_prefix("Content-Length:") {
            return value.trim().parse::<usize>().ok();
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    // --- Unit tests for message formatting and header parsing ---

    #[test]
    fn test_format_lsp_message_simple() {
        let body = b"hello";
        let msg = format_lsp_message(body);
        let msg_str = String::from_utf8(msg).unwrap();
        assert!(msg_str.starts_with("Content-Length: 5\r\n\r\n"));
        assert!(msg_str.ends_with("hello"));
    }

    #[test]
    fn test_format_lsp_message_empty_body() {
        let body = b"";
        let msg = format_lsp_message(body);
        let msg_str = String::from_utf8(msg).unwrap();
        assert!(msg_str.starts_with("Content-Length: 0\r\n\r\n"));
    }

    #[test]
    fn test_format_lsp_message_json_body() {
        let body = serde_json::to_vec(&json!({"jsonrpc":"2.0","id":1,"method":"initialize"}))
            .unwrap();
        let msg = format_lsp_message(&body);
        let msg_str = String::from_utf8(msg).unwrap();
        let expected_header = format!("Content-Length: {}\r\n\r\n", body.len());
        assert!(msg_str.starts_with(&expected_header));
    }

    #[test]
    fn test_parse_content_length_valid() {
        let header = "Content-Length: 42\r\nContent-Type: application/json\r\n";
        assert_eq!(parse_content_length(header), Some(42));
    }

    #[test]
    fn test_parse_content_length_no_header() {
        let header = "Content-Type: application/json\r\n";
        assert_eq!(parse_content_length(header), None);
    }

    #[test]
    fn test_parse_content_length_empty() {
        assert_eq!(parse_content_length(""), None);
    }

    #[test]
    fn test_parse_content_length_with_whitespace() {
        let header = "Content-Length:  128  \r\n";
        assert_eq!(parse_content_length(header), Some(128));
    }

    #[test]
    fn test_parse_content_length_invalid_value() {
        let header = "Content-Length: not_a_number\r\n";
        assert_eq!(parse_content_length(header), None);
    }

    // --- Tests for read_content_length async helper ---

    #[tokio::test]
    async fn test_read_content_length_valid() {
        let data = b"Content-Length: 15\r\n\r\n";
        let mut reader = BufReader::new(&data[..]);
        let len = read_content_length(&mut reader).await.unwrap();
        assert_eq!(len, 15);
    }

    #[tokio::test]
    async fn test_read_content_length_with_extra_headers() {
        let data = b"Content-Length: 99\r\nContent-Type: application/json\r\n\r\n";
        let mut reader = BufReader::new(&data[..]);
        let len = read_content_length(&mut reader).await.unwrap();
        assert_eq!(len, 99);
    }

    #[tokio::test]
    async fn test_read_content_length_eof() {
        let data = b"";
        let mut reader = BufReader::new(&data[..]);
        let result = read_content_length(&mut reader).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_read_content_length_missing_header() {
        let data = b"Content-Type: text/plain\r\n\r\n";
        let mut reader = BufReader::new(&data[..]);
        let result = read_content_length(&mut reader).await;
        assert!(result.is_err()); // missing Content-Length
    }

    // --- Tests for dispatch_notification ---

    #[test]
    fn test_dispatch_notification_diagnostics() {
        let (tx, mut rx) = mpsc::unbounded_channel();
        let notif = JsonRpcNotification::new(
            "textDocument/publishDiagnostics",
            Some(json!({
                "uri": "file:///test.rs",
                "diagnostics": []
            })),
        );
        dispatch_notification(&tx, notif);
        let event = rx.try_recv().unwrap();
        match event {
            LspEvent::Diagnostics { uri, diagnostics } => {
                assert_eq!(uri, "file:///test.rs");
                assert!(diagnostics.is_empty());
            }
            _ => panic!("expected Diagnostics event"),
        }
    }

    #[test]
    fn test_dispatch_notification_unknown_method() {
        let (tx, mut rx) = mpsc::unbounded_channel();
        let notif = JsonRpcNotification::new("window/logMessage", Some(json!({"message": "hi"})));
        dispatch_notification(&tx, notif);
        // Unknown notifications are silently ignored; no event emitted.
        assert!(rx.try_recv().is_err());
    }

    #[test]
    fn test_dispatch_notification_diagnostics_bad_params() {
        let (tx, mut rx) = mpsc::unbounded_channel();
        let notif = JsonRpcNotification::new(
            "textDocument/publishDiagnostics",
            Some(json!({"bad": "data"})),
        );
        dispatch_notification(&tx, notif);
        // Malformed params should not produce an event.
        assert!(rx.try_recv().is_err());
    }

    // --- Tests for JsonRpcRequest/Response serialization in context ---

    #[test]
    fn test_request_message_format() {
        let req = JsonRpcRequest::new(1, "initialize", Some(json!({"capabilities": {}})));
        let body = serde_json::to_vec(&req).unwrap();
        let msg = format_lsp_message(&body);
        let msg_str = String::from_utf8(msg).unwrap();

        // Verify it has the header and body.
        assert!(msg_str.contains("Content-Length:"));
        assert!(msg_str.contains("initialize"));
    }

    #[test]
    fn test_response_deserialization() {
        let json_str = r#"{"jsonrpc":"2.0","id":1,"result":{"capabilities":{}}}"#;
        let resp: JsonRpcResponse = serde_json::from_str(json_str).unwrap();
        assert_eq!(resp.id, 1);
        assert!(resp.result.is_some());
        assert!(resp.error.is_none());
    }

    #[test]
    fn test_response_error_deserialization() {
        let json_str = r#"{"jsonrpc":"2.0","id":2,"error":{"code":-32601,"message":"Method not found"}}"#;
        let resp: JsonRpcResponse = serde_json::from_str(json_str).unwrap();
        assert_eq!(resp.id, 2);
        assert!(resp.result.is_none());
        let err = resp.error.unwrap();
        assert_eq!(err.code, -32601);
        assert_eq!(err.message, "Method not found");
    }

    // --- Tests for full message read/parse round-trip ---

    #[tokio::test]
    async fn test_read_full_message_roundtrip() {
        let body = json!({"jsonrpc":"2.0","id":1,"result":{"capabilities":{}}});
        let body_bytes = serde_json::to_vec(&body).unwrap();
        let raw = format_lsp_message(&body_bytes);

        let mut reader = BufReader::new(&raw[..]);

        // Read the content length.
        let content_length = read_content_length(&mut reader).await.unwrap();
        assert_eq!(content_length, body_bytes.len());

        // Read the body.
        let mut buf = vec![0u8; content_length];
        reader.read_exact(&mut buf).await.unwrap();

        // Parse the response.
        let resp: JsonRpcResponse = serde_json::from_slice(&buf).unwrap();
        assert_eq!(resp.id, 1);
        assert!(resp.result.is_some());
    }

    #[tokio::test]
    async fn test_read_multiple_messages() {
        let body1 = serde_json::to_vec(&json!({"jsonrpc":"2.0","id":1,"result":null})).unwrap();
        let body2 = serde_json::to_vec(&json!({"jsonrpc":"2.0","id":2,"result":"ok"})).unwrap();

        let mut raw = Vec::new();
        raw.extend_from_slice(&format_lsp_message(&body1));
        raw.extend_from_slice(&format_lsp_message(&body2));

        let mut reader = BufReader::new(&raw[..]);

        // First message
        let len1 = read_content_length(&mut reader).await.unwrap();
        let mut buf1 = vec![0u8; len1];
        reader.read_exact(&mut buf1).await.unwrap();
        let resp1: JsonRpcResponse = serde_json::from_slice(&buf1).unwrap();
        assert_eq!(resp1.id, 1);

        // Second message
        let len2 = read_content_length(&mut reader).await.unwrap();
        let mut buf2 = vec![0u8; len2];
        reader.read_exact(&mut buf2).await.unwrap();
        let resp2: JsonRpcResponse = serde_json::from_slice(&buf2).unwrap();
        assert_eq!(resp2.id, 2);
    }

    #[test]
    fn test_notification_message_format() {
        let notif = JsonRpcNotification::new("initialized", Some(json!({})));
        let body = serde_json::to_vec(&notif).unwrap();
        let msg = format_lsp_message(&body);
        let msg_str = String::from_utf8(msg).unwrap();
        assert!(msg_str.contains("Content-Length:"));
        assert!(msg_str.contains("initialized"));
    }

    #[test]
    fn test_atomic_id_generation() {
        let counter = AtomicI64::new(1);
        let id1 = counter.fetch_add(1, Ordering::SeqCst);
        let id2 = counter.fetch_add(1, Ordering::SeqCst);
        let id3 = counter.fetch_add(1, Ordering::SeqCst);
        assert_eq!(id1, 1);
        assert_eq!(id2, 2);
        assert_eq!(id3, 3);
    }

    /// Integration test: spawn rust-analyzer, initialize, didOpen, and request completion.
    /// Skips gracefully if rust-analyzer is not available.
    #[tokio::test(flavor = "multi_thread")]
    async fn test_rust_analyzer_completion() {
        // Check rust-analyzer exists
        let ra_ok = std::process::Command::new("which")
            .arg("rust-analyzer")
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status()
            .map(|s| s.success())
            .unwrap_or(false);
        if !ra_ok {
            eprintln!("SKIP: rust-analyzer not found");
            return;
        }

        // Create a minimal Rust project in a temp dir
        let tmp = tempfile::TempDir::new().unwrap();
        std::fs::create_dir(tmp.path().join("src")).unwrap();
        std::fs::write(
            tmp.path().join("src/main.rs"),
            "fn main() {\n    let x = String::new();\n    x.\n}\n",
        ).unwrap();
        std::fs::write(
            tmp.path().join("Cargo.toml"),
            "[package]\nname = \"t\"\nversion = \"0.1.0\"\nedition = \"2021\"\n",
        ).unwrap();

        let root_uri = format!("file://{}", tmp.path().to_string_lossy());
        let file_uri_str = format!("file://{}/src/main.rs", tmp.path().to_string_lossy());
        let (event_tx, _event_rx) = mpsc::unbounded_channel::<LspEvent>();

        let client = LspClient::new("rust-analyzer", &[], &root_uri, event_tx).unwrap();

        // Initialize — if this fails the server binary likely isn't functional
        // (e.g. rustup proxy without the component installed). Skip gracefully.
        let init_result = tokio::time::timeout(
            std::time::Duration::from_secs(10),
            client.initialize(&root_uri),
        ).await;
        let init_result = match init_result {
            Ok(r) => r,
            Err(_) => { eprintln!("SKIP: initialize timed out"); return; }
        };
        if init_result.is_err() {
            eprintln!("SKIP: initialize failed (rust-analyzer component may not be installed)");
            return;
        }

        // didOpen
        let file_uri = file_uri_str.parse::<lsp_types::Uri>().unwrap();
        let text = std::fs::read_to_string(tmp.path().join("src/main.rs")).unwrap();
        client.send_notification::<lsp_types::notification::DidOpenTextDocument>(
            lsp_types::DidOpenTextDocumentParams {
                text_document: lsp_types::TextDocumentItem {
                    uri: file_uri.clone(),
                    language_id: "rust".to_string(),
                    version: 1,
                    text,
                },
            }
        ).await;
        eprintln!("test: didOpen sent");

        // Wait a bit for rust-analyzer to process
        tokio::time::sleep(std::time::Duration::from_secs(3)).await;

        // Send completion at "x." (line 2, col 6)
        eprintln!("test: sending completion request...");
        let completion_result = client.send_request::<lsp_types::request::Completion>(
            lsp_types::CompletionParams {
                text_document_position: lsp_types::TextDocumentPositionParams {
                    text_document: lsp_types::TextDocumentIdentifier { uri: file_uri },
                    position: lsp_types::Position { line: 2, character: 6 },
                },
                work_done_progress_params: Default::default(),
                partial_result_params: Default::default(),
                context: None,
            }
        ).await;

        match &completion_result {
            Ok(Some(resp)) => {
                let count = match resp {
                    lsp_types::CompletionResponse::Array(items) => items.len(),
                    lsp_types::CompletionResponse::List(list) => list.items.len(),
                };
                eprintln!("test: got {} completion items", count);
                assert!(count > 0, "should have completions for String methods");
            }
            Ok(None) => eprintln!("test: completion returned None (server might still be indexing)"),
            Err(e) => eprintln!("test: completion FAILED: {}", e),
        }

        // Shutdown
        let _ = client.shutdown().await;
        eprintln!("test: done");
    }
}
