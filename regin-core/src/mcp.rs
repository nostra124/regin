//! MCP (Model Context Protocol) client (FEAT-081 / DISC-021): connects to
//! local (stdio) and remote (HTTP) MCP servers configured via SQLite,
//! registers their tools in the LLM's tool set under an `mcp_<server>_<tool>`
//! prefix, and dispatches `tools/call` to the right server.
//!
//! **Layered like every other real-I/O integration in this crate** (`lsp`,
//! `bus`, `push`): orchestration (server discovery, tool-name resolution,
//! reconnect backoff, the pool) is pure/injectable and unit-tested with
//! fakes; [`StdioMcpProcess`]/[`HttpMcpClient`] are the real, thinner wire
//! clients. Unlike FEAT-078's LSP client (no real language server available
//! in the build sandbox), MCP's wire format needs no real server binary to
//! test correctly — [`StdioMcpProcess`]'s handshake, tool discovery, and
//! tool-call round trip are all exercised end-to-end in tests over a
//! `tokio::io::duplex` pair standing in for a spawned process's stdio.
//!
//! **Simplification, documented rather than hidden**: the remote transport
//! is a stateless "one JSON-RPC request per HTTP call" client (the simple
//! subset of MCP's Streamable HTTP transport) rather than a persistent
//! SSE-streamed session — no `initialize` handshake is performed for it
//! either, since there is no persistent session to initialize.

use anyhow::{Context, Result, bail};
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio::io::{AsyncBufRead, AsyncBufReadExt, AsyncWrite, AsyncWriteExt, BufReader};
use tokio::process::{Child, Command};

use crate::db;
use crate::tools::{FunctionDef, ToolDef};

// ---------------------------------------------------------------------------
// Configuration (acceptance criterion 2)
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq)]
pub enum McpTransportConfig {
    Local { command: Vec<String> },
    Remote { url: String, headers: HashMap<String, String> },
}

#[derive(Debug, Clone, PartialEq)]
pub struct McpServerConfig {
    pub name: String,
    pub transport: McpTransportConfig,
    /// Per-tool-call timeout (acceptance criterion 6, default 30s).
    pub timeout: Duration,
}

/// Discover every configured MCP server (`mcp.<name>.type` present in
/// settings) and resolve its full config. Each server's resolution is
/// independent — a malformed server doesn't prevent discovering the others
/// (same fail-safe-per-item convention as `run_due_schedules`).
pub fn discover_configured_servers(conn: &rusqlite::Connection) -> Vec<(String, Result<McpServerConfig>)> {
    let all = db::setting_list(conn).unwrap_or_default();
    let mut names: Vec<String> = all
        .iter()
        .filter_map(|(k, _)| k.strip_prefix("mcp.").and_then(|rest| rest.strip_suffix(".type")))
        .map(str::to_string)
        .collect();
    names.sort();
    names.dedup();
    names.into_iter().map(|name| { let cfg = resolve_server_config(conn, &name); (name, cfg) }).collect()
}

fn resolve_server_config(conn: &rusqlite::Connection, name: &str) -> Result<McpServerConfig> {
    let ty = db::setting_get(conn, &format!("mcp.{name}.type"))?;
    let timeout_secs: u64 = db::setting_get(conn, &format!("mcp.{name}.timeout_secs"))
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(30);

    let transport = match ty.as_str() {
        "local" => {
            let raw = db::setting_get(conn, &format!("mcp.{name}.command"))?;
            let command: Vec<String> = serde_json::from_str(&raw)
                .with_context(|| format!("mcp.{name}.command must be a JSON array of strings, got {raw:?}"))?;
            if command.is_empty() {
                bail!("mcp.{name}.command is empty");
            }
            McpTransportConfig::Local { command }
        }
        "remote" => {
            let url = db::setting_get(conn, &format!("mcp.{name}.url"))?;
            if url.is_empty() {
                bail!("mcp.{name}.url is not set");
            }
            let headers_raw = db::setting_get(conn, &format!("mcp.{name}.headers"))?;
            let headers: HashMap<String, String> = if headers_raw.trim().is_empty() {
                HashMap::new()
            } else {
                serde_json::from_str(&headers_raw)
                    .with_context(|| format!("mcp.{name}.headers must be a JSON object, got {headers_raw:?}"))?
            };
            McpTransportConfig::Remote { url, headers }
        }
        other => bail!("mcp.{name}.type must be \"local\" or \"remote\", got {other:?}"),
    };

    Ok(McpServerConfig { name: name.to_string(), transport, timeout: Duration::from_secs(timeout_secs) })
}

// ---------------------------------------------------------------------------
// Wire types
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct McpToolDef {
    pub name: String,
    #[serde(default)]
    pub description: String,
    #[serde(rename = "inputSchema", default = "default_input_schema")]
    pub input_schema: Value,
}

fn default_input_schema() -> Value {
    json!({"type": "object", "properties": {}})
}

#[derive(Debug, Clone, PartialEq)]
pub struct McpToolCallResult {
    pub text: String,
    pub is_error: bool,
}

fn parse_tool_call_result(result: &Value) -> McpToolCallResult {
    let is_error = result.get("isError").and_then(Value::as_bool).unwrap_or(false);
    let text = result
        .get("content")
        .and_then(Value::as_array)
        .map(|items| {
            items
                .iter()
                .filter_map(|item| item.get("text").and_then(Value::as_str))
                .collect::<Vec<_>>()
                .join("\n")
        })
        .unwrap_or_default();
    McpToolCallResult { text, is_error }
}

// ---------------------------------------------------------------------------
// Client + spawner traits (acceptance criteria 1, 4)
// ---------------------------------------------------------------------------

#[async_trait]
pub trait McpClient: Send + Sync {
    async fn list_tools(&self, timeout: Duration) -> Result<Vec<McpToolDef>>;
    async fn call_tool(&self, name: &str, arguments: Value, timeout: Duration) -> Result<McpToolCallResult>;
}

#[async_trait]
pub trait McpSpawner: Send + Sync {
    async fn connect(&self, config: &McpServerConfig) -> Result<Arc<dyn McpClient>>;
}

pub struct ProcessMcpSpawner;

#[async_trait]
impl McpSpawner for ProcessMcpSpawner {
    async fn connect(&self, config: &McpServerConfig) -> Result<Arc<dyn McpClient>> {
        match &config.transport {
            McpTransportConfig::Local { command } => Ok(Arc::new(StdioMcpProcess::spawn(command).await?)),
            McpTransportConfig::Remote { url, headers } => Ok(Arc::new(HttpMcpClient::new(url.clone(), headers.clone())?)),
        }
    }
}

// ---------------------------------------------------------------------------
// Wire framing: newline-delimited JSON-RPC 2.0 (MCP stdio transport)
// ---------------------------------------------------------------------------

fn encode_request(id: u64, method: &str, params: Value) -> String {
    format!("{}\n", json!({"jsonrpc": "2.0", "id": id, "method": method, "params": params}))
}

fn encode_notification(method: &str, params: Value) -> String {
    format!("{}\n", json!({"jsonrpc": "2.0", "method": method, "params": params}))
}

async fn write_line<W: AsyncWrite + Unpin>(w: &mut W, line: &str) -> Result<()> {
    w.write_all(line.as_bytes()).await?;
    w.flush().await?;
    Ok(())
}

async fn read_json_line<R: AsyncBufRead + Unpin>(reader: &mut R) -> Result<Value> {
    loop {
        let mut line = String::new();
        let n = reader.read_line(&mut line).await?;
        if n == 0 {
            bail!("MCP stdio stream closed");
        }
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        return serde_json::from_str(trimmed).with_context(|| format!("invalid JSON-RPC line: {trimmed}"));
    }
}

// ---------------------------------------------------------------------------
// Local (stdio) client (acceptance criterion 1)
// ---------------------------------------------------------------------------

/// A local MCP server speaking JSON-RPC 2.0 over stdio (one JSON object per
/// line — unlike LSP, MCP's stdio transport has no `Content-Length` framing).
/// Request/response pairs are correlated by numeric `id` via a background
/// reader task and a `oneshot` per in-flight call.
pub struct StdioMcpProcess {
    stdin: tokio::sync::Mutex<Box<dyn AsyncWrite + Unpin + Send>>,
    pending: Arc<Mutex<HashMap<u64, tokio::sync::oneshot::Sender<Value>>>>,
    next_id: AtomicU64,
    _child: Option<Child>,
}

impl StdioMcpProcess {
    pub async fn spawn(command: &[String]) -> Result<Self> {
        let (prog, args) = command.split_first().context("empty MCP server command")?;
        let mut child = Command::new(prog)
            .args(args)
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::null())
            .kill_on_drop(true)
            .spawn()
            .with_context(|| format!("spawning MCP server {prog:?}"))?;

        let stdin = child.stdin.take().context("no stdin on spawned MCP server")?;
        let stdout = child.stdout.take().context("no stdout on spawned MCP server")?;
        Self::handshake(BufReader::new(stdout), stdin, Some(child)).await
    }

    /// Test-only entry point: the identical handshake + background reader
    /// as `spawn`, but over an arbitrary reader/writer pair (a
    /// `tokio::io::duplex` in tests standing in for a spawned process's
    /// stdio) instead of a real child process.
    async fn handshake<R, W>(mut reader: R, writer: W, child: Option<Child>) -> Result<Self>
    where
        R: AsyncBufRead + Unpin + Send + 'static,
        W: AsyncWrite + Unpin + Send + 'static,
    {
        let mut stdin: Box<dyn AsyncWrite + Unpin + Send> = Box::new(writer);

        let init_params = json!({
            "protocolVersion": "2024-11-05",
            "capabilities": {},
            "clientInfo": {"name": "regin", "version": env!("CARGO_PKG_VERSION")},
        });
        write_line(&mut stdin, &encode_request(0, "initialize", init_params)).await?;
        let _init_response = read_json_line(&mut reader).await.context("reading MCP initialize response")?;
        write_line(&mut stdin, &encode_notification("notifications/initialized", json!({}))).await?;

        let pending: Arc<Mutex<HashMap<u64, tokio::sync::oneshot::Sender<Value>>>> = Arc::new(Mutex::new(HashMap::new()));
        let pending_bg = pending.clone();
        tokio::spawn(async move {
            while let Ok(msg) = read_json_line(&mut reader).await {
                if let Some(id) = msg.get("id").and_then(Value::as_u64)
                    && let Some(tx) = pending_bg.lock().unwrap().remove(&id)
                {
                    let _ = tx.send(msg);
                }
                // Server-initiated requests/notifications this client
                // doesn't need (e.g. logging) are dropped — this is a
                // tool-calling client, not a full MCP host implementation.
            }
            // Server exited / stream closed / bad JSON: wake every
            // still-pending call immediately with a connection-closed error
            // instead of leaving it to time out.
            pending_bg.lock().unwrap().clear();
        });

        Ok(Self { stdin: tokio::sync::Mutex::new(stdin), pending, next_id: AtomicU64::new(1), _child: child })
    }

    async fn call(&self, method: &str, params: Value, timeout: Duration) -> Result<Value> {
        let id = self.next_id.fetch_add(1, Ordering::SeqCst);
        let (tx, rx) = tokio::sync::oneshot::channel();
        self.pending.lock().unwrap().insert(id, tx);

        let line = encode_request(id, method, params);
        {
            let mut w = self.stdin.lock().await;
            w.write_all(line.as_bytes()).await?;
            w.flush().await?;
        }

        match tokio::time::timeout(timeout, rx).await {
            Ok(Ok(resp)) => {
                if let Some(err) = resp.get("error") {
                    bail!("MCP error calling {method}: {err}");
                }
                Ok(resp.get("result").cloned().unwrap_or(Value::Null))
            }
            Ok(Err(_)) => {
                self.pending.lock().unwrap().remove(&id);
                bail!("MCP connection closed while waiting for a response to {method}")
            }
            Err(_) => {
                self.pending.lock().unwrap().remove(&id);
                bail!("MCP call {method} timed out after {}s", timeout.as_secs())
            }
        }
    }
}

#[async_trait]
impl McpClient for StdioMcpProcess {
    async fn list_tools(&self, timeout: Duration) -> Result<Vec<McpToolDef>> {
        let result = self.call("tools/list", json!({}), timeout).await?;
        let tools = result.get("tools").cloned().unwrap_or_else(|| Value::Array(vec![]));
        Ok(serde_json::from_value(tools).unwrap_or_default())
    }

    async fn call_tool(&self, name: &str, arguments: Value, timeout: Duration) -> Result<McpToolCallResult> {
        let result = self.call("tools/call", json!({"name": name, "arguments": arguments}), timeout).await?;
        Ok(parse_tool_call_result(&result))
    }
}

// ---------------------------------------------------------------------------
// Remote (HTTP) client (acceptance criterion 1)
// ---------------------------------------------------------------------------

/// A remote MCP server reached over plain HTTP: one JSON-RPC request per
/// tool call, no persistent session (the simple subset of MCP's Streamable
/// HTTP transport — see the module doc comment).
pub struct HttpMcpClient {
    client: reqwest::Client,
    url: String,
    headers: HashMap<String, String>,
    next_id: AtomicU64,
}

impl HttpMcpClient {
    pub fn new(url: String, headers: HashMap<String, String>) -> Result<Self> {
        Ok(Self { client: reqwest::Client::builder().build()?, url, headers, next_id: AtomicU64::new(1) })
    }

    async fn call(&self, method: &str, params: Value, timeout: Duration) -> Result<Value> {
        let id = self.next_id.fetch_add(1, Ordering::SeqCst);
        let body = json!({"jsonrpc": "2.0", "id": id, "method": method, "params": params});
        let mut req = self.client.post(&self.url).json(&body).timeout(timeout);
        for (k, v) in &self.headers {
            req = req.header(k, v);
        }
        let resp = req.send().await.context("MCP HTTP request failed")?;
        let status = resp.status();
        if !status.is_success() {
            let text = resp.text().await.unwrap_or_default();
            bail!("MCP HTTP error {status}: {text}");
        }
        let raw: Value = resp.json().await.context("Failed to parse MCP HTTP response")?;
        if let Some(err) = raw.get("error") {
            bail!("MCP error calling {method}: {err}");
        }
        Ok(raw.get("result").cloned().unwrap_or(Value::Null))
    }
}

#[async_trait]
impl McpClient for HttpMcpClient {
    async fn list_tools(&self, timeout: Duration) -> Result<Vec<McpToolDef>> {
        let result = self.call("tools/list", json!({}), timeout).await?;
        let tools = result.get("tools").cloned().unwrap_or_else(|| Value::Array(vec![]));
        Ok(serde_json::from_value(tools).unwrap_or_default())
    }

    async fn call_tool(&self, name: &str, arguments: Value, timeout: Duration) -> Result<McpToolCallResult> {
        let result = self.call("tools/call", json!({"name": name, "arguments": arguments}), timeout).await?;
        Ok(parse_tool_call_result(&result))
    }
}

// ---------------------------------------------------------------------------
// Tool-name resolution: `mcp_<server>_<tool>` (acceptance criterion 3)
// ---------------------------------------------------------------------------

pub fn is_mcp_tool_name(name: &str) -> bool {
    name.starts_with("mcp_")
}

/// Split a full `mcp_<server>_<tool>` name into `(server, tool)`, given the
/// set of currently-known server names. Picks the *longest* matching server
/// name so a server whose own name contains an underscore isn't ambiguous
/// with a shorter same-prefix server name.
fn resolve_mcp_tool_name<'a>(full_name: &str, server_names: impl Iterator<Item = &'a String>) -> Option<(String, String)> {
    let rest = full_name.strip_prefix("mcp_")?;
    let mut best: Option<(String, String)> = None;
    for name in server_names {
        if let Some(tool) = rest.strip_prefix(&format!("{name}_"))
            && best.as_ref().is_none_or(|(n, _)| n.len() < name.len())
        {
            best = Some((name.clone(), tool.to_string()));
        }
    }
    best
}

// ---------------------------------------------------------------------------
// Reconnect backoff (acceptance criterion 5)
// ---------------------------------------------------------------------------

pub const MAX_RECONNECT_ATTEMPTS: u32 = 5;

/// Exponential backoff delay for reconnect attempt number `attempt` (1-based
/// first retry): 2s, 4s, 8s, 16s, 32s, capped at attempt 5.
pub fn backoff_delay(attempt: u32) -> Duration {
    Duration::from_secs(2u64.saturating_pow(attempt.clamp(1, 5)))
}

struct ReconnectState {
    attempts: u32,
    next_attempt_at: DateTime<Utc>,
}

/// Tracks per-server reconnect attempts so a background checker knows
/// whether it's time to retry a disconnected server, and gives up after
/// [`MAX_RECONNECT_ATTEMPTS`]. Takes `now` explicitly (fake-clock testable,
/// same convention as `lsp::Debouncer`).
#[derive(Default)]
pub struct ReconnectTracker {
    state: HashMap<String, ReconnectState>,
}

impl ReconnectTracker {
    pub fn new() -> Self {
        Self::default()
    }

    /// Whether `name` should attempt a reconnect right now: true if never
    /// attempted, or the backoff window has elapsed and fewer than
    /// [`MAX_RECONNECT_ATTEMPTS`] have been made.
    pub fn should_retry(&self, name: &str, now: DateTime<Utc>) -> bool {
        match self.state.get(name) {
            None => true,
            Some(s) => s.attempts < MAX_RECONNECT_ATTEMPTS && now >= s.next_attempt_at,
        }
    }

    pub fn record_failure(&mut self, name: &str, now: DateTime<Utc>) {
        let entry = self.state.entry(name.to_string()).or_insert(ReconnectState { attempts: 0, next_attempt_at: now });
        entry.attempts += 1;
        entry.next_attempt_at = now + chrono::Duration::from_std(backoff_delay(entry.attempts)).unwrap_or_default();
    }

    pub fn record_success(&mut self, name: &str) {
        self.state.remove(name);
    }
}

// ---------------------------------------------------------------------------
// The pool: connected servers + their cached tool lists
// ---------------------------------------------------------------------------

struct ConnectedServer {
    client: Arc<dyn McpClient>,
    tools: Vec<McpToolDef>,
    timeout: Duration,
}

/// Holds every currently-connected MCP server (acceptance criteria 3, 5) and
/// the reconnect backoff state for disconnected ones.
pub struct McpContext {
    servers: Mutex<HashMap<String, ConnectedServer>>,
    spawner: Arc<dyn McpSpawner>,
    pub reconnect: Mutex<ReconnectTracker>,
}

impl McpContext {
    pub fn new(spawner: Arc<dyn McpSpawner>) -> Self {
        Self { servers: Mutex::new(HashMap::new()), spawner, reconnect: Mutex::new(ReconnectTracker::new()) }
    }

    pub fn is_connected(&self, name: &str) -> bool {
        self.servers.lock().unwrap().contains_key(name)
    }

    /// Connect to one server and cache its tool list (`tools/list`).
    /// Returns the number of tools discovered.
    pub async fn connect_one(&self, config: &McpServerConfig) -> Result<usize> {
        let client = self.spawner.connect(config).await?;
        let tools = client.list_tools(config.timeout).await?;
        let count = tools.len();
        self.servers.lock().unwrap().insert(config.name.clone(), ConnectedServer { client, tools, timeout: config.timeout });
        Ok(count)
    }

    /// Connect to every configured server (acceptance criterion 3). One
    /// server's failure doesn't stop the others.
    pub async fn connect_all(&self, configs: &[McpServerConfig]) -> Vec<(String, Result<usize>)> {
        let mut out = Vec::new();
        for config in configs {
            out.push((config.name.clone(), self.connect_one(config).await));
        }
        out
    }

    /// Tool definitions for every connected server's cached tools, prefixed
    /// `mcp_<name>_<toolName>` (acceptance criterion 3).
    pub fn tool_definitions(&self) -> Vec<ToolDef> {
        self.servers
            .lock()
            .unwrap()
            .iter()
            .flat_map(|(name, s)| {
                s.tools.iter().map(move |t| ToolDef {
                    tool_type: "function".into(),
                    function: FunctionDef {
                        name: format!("mcp_{name}_{}", t.name),
                        description: t.description.clone(),
                        parameters: t.input_schema.clone(),
                    },
                })
            })
            .collect()
    }

    /// Dispatch a `mcp_<server>_<tool>` call to its connected server
    /// (acceptance criterion 4), respecting that server's configured
    /// timeout (acceptance criterion 6).
    pub async fn dispatch(&self, full_name: &str, arguments: Value) -> Result<McpToolCallResult> {
        let (server_name, tool_name, client, timeout) = {
            let servers = self.servers.lock().unwrap();
            let (server_name, tool_name) = resolve_mcp_tool_name(full_name, servers.keys())
                .with_context(|| format!("no connected MCP server matches tool {full_name:?}"))?;
            let server = servers.get(&server_name).context("MCP server not connected")?;
            (server_name, tool_name, server.client.clone(), server.timeout)
        };
        let _ = &server_name;
        client.call_tool(&tool_name, arguments, timeout).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn conn() -> rusqlite::Connection {
        let c = rusqlite::Connection::open_in_memory().unwrap();
        db::init_schema(&c).unwrap();
        c
    }

    // --- criterion 2: configuration discovery ------------------------------

    #[test]
    fn discovers_a_local_server() {
        let c = conn();
        db::setting_set(&c, "mcp.files.type", "local").unwrap();
        db::setting_set(&c, "mcp.files.command", r#"["npx", "-y", "server"]"#).unwrap();
        let discovered = discover_configured_servers(&c);
        assert_eq!(discovered.len(), 1);
        let (name, cfg) = &discovered[0];
        assert_eq!(name, "files");
        let cfg = cfg.as_ref().unwrap();
        assert_eq!(cfg.transport, McpTransportConfig::Local { command: vec!["npx".into(), "-y".into(), "server".into()] });
        assert_eq!(cfg.timeout, Duration::from_secs(30), "default timeout");
    }

    #[test]
    fn discovers_a_remote_server_with_headers_and_a_custom_timeout() {
        let c = conn();
        db::setting_set(&c, "mcp.api.type", "remote").unwrap();
        db::setting_set(&c, "mcp.api.url", "https://example.com/mcp").unwrap();
        db::setting_set(&c, "mcp.api.headers", r#"{"Authorization": "Bearer x"}"#).unwrap();
        db::setting_set(&c, "mcp.api.timeout_secs", "10").unwrap();

        let discovered = discover_configured_servers(&c);
        let (name, cfg) = &discovered[0];
        assert_eq!(name, "api");
        let cfg = cfg.as_ref().unwrap();
        let mut expected_headers = HashMap::new();
        expected_headers.insert("Authorization".to_string(), "Bearer x".to_string());
        assert_eq!(cfg.transport, McpTransportConfig::Remote { url: "https://example.com/mcp".into(), headers: expected_headers });
        assert_eq!(cfg.timeout, Duration::from_secs(10));
    }

    #[test]
    fn a_malformed_server_does_not_prevent_discovering_others() {
        let c = conn();
        db::setting_set(&c, "mcp.broken.type", "local").unwrap();
        db::setting_set(&c, "mcp.broken.command", "not json").unwrap();
        db::setting_set(&c, "mcp.ok.type", "local").unwrap();
        db::setting_set(&c, "mcp.ok.command", r#"["echo"]"#).unwrap();

        let discovered = discover_configured_servers(&c);
        assert_eq!(discovered.len(), 2);
        let broken = discovered.iter().find(|(n, _)| n == "broken").unwrap();
        assert!(broken.1.is_err());
        let ok = discovered.iter().find(|(n, _)| n == "ok").unwrap();
        assert!(ok.1.is_ok());
    }

    #[test]
    fn an_unknown_transport_type_errors() {
        let c = conn();
        db::setting_set(&c, "mcp.weird.type", "carrier-pigeon").unwrap();
        let discovered = discover_configured_servers(&c);
        assert!(discovered[0].1.is_err());
    }

    #[test]
    fn no_configured_servers_discovers_nothing() {
        let c = conn();
        assert!(discover_configured_servers(&c).is_empty());
    }

    // --- criterion 3: mcp_<server>_<tool> resolution ------------------------

    #[test]
    fn resolves_the_longest_matching_server_name() {
        let names = ["files".to_string(), "files_extra".to_string()];
        assert_eq!(resolve_mcp_tool_name("mcp_files_read", names.iter()), Some(("files".to_string(), "read".to_string())));
        assert_eq!(resolve_mcp_tool_name("mcp_files_extra_read", names.iter()), Some(("files_extra".to_string(), "read".to_string())));
        assert_eq!(resolve_mcp_tool_name("mcp_unknown_read", names.iter()), None);
        assert_eq!(resolve_mcp_tool_name("bash", names.iter()), None, "not even prefixed with mcp_");
    }

    #[test]
    fn is_mcp_tool_name_checks_the_prefix() {
        assert!(is_mcp_tool_name("mcp_files_read"));
        assert!(!is_mcp_tool_name("bash"));
    }

    // --- criterion 5: reconnect backoff -------------------------------------

    fn t(secs: i64) -> DateTime<Utc> {
        "2026-01-01T00:00:00Z".parse::<DateTime<Utc>>().unwrap() + chrono::Duration::seconds(secs)
    }

    #[test]
    fn backoff_delay_grows_and_caps_at_five() {
        assert_eq!(backoff_delay(1), Duration::from_secs(2));
        assert_eq!(backoff_delay(2), Duration::from_secs(4));
        assert_eq!(backoff_delay(5), Duration::from_secs(32));
        assert_eq!(backoff_delay(9), Duration::from_secs(32), "clamped at attempt 5's delay");
    }

    #[test]
    fn reconnect_tracker_retries_after_backoff_then_gives_up() {
        let mut tr = ReconnectTracker::new();
        assert!(tr.should_retry("files", t(0)), "never attempted -> retry now");

        tr.record_failure("files", t(0));
        assert!(!tr.should_retry("files", t(1)), "still inside the 2s backoff window");
        assert!(tr.should_retry("files", t(2)), "backoff window elapsed");

        // Exhaust all attempts.
        for i in 1..MAX_RECONNECT_ATTEMPTS {
            tr.record_failure("files", t(100 * i as i64));
        }
        assert!(!tr.should_retry("files", t(100_000)), "gave up after max attempts, no matter how much time passes");
    }

    #[test]
    fn reconnect_tracker_success_clears_the_state() {
        let mut tr = ReconnectTracker::new();
        tr.record_failure("files", t(0));
        tr.record_success("files");
        assert!(tr.should_retry("files", t(0)), "cleared -> treated as never attempted");
    }

    // --- criterion 1: local stdio transport, handshake, discovery, calls ---

    #[tokio::test]
    async fn handshake_tools_list_and_tool_call_round_trip() {
        // acceptance criterion 1 (stdio + handshake), criterion 8 (tool
        // discovery, tool-call round trip) — a fake MCP server driven over a
        // tokio::io::duplex pair, no real process required.
        let (client_side, server_side) = tokio::io::duplex(64 * 1024);
        let (client_read, client_write) = tokio::io::split(client_side);
        let (server_read, server_write) = tokio::io::split(server_side);

        let fake_server = tokio::spawn(async move {
            let mut r = BufReader::new(server_read);
            let mut w = server_write;

            // initialize
            let req = read_json_line(&mut r).await.unwrap();
            write_line(&mut w, &format!("{}\n", json!({"jsonrpc":"2.0","id":req["id"],"result":{"protocolVersion":"2024-11-05"}}))).await.unwrap();
            // notifications/initialized
            let _ = read_json_line(&mut r).await.unwrap();

            // tools/list
            let req = read_json_line(&mut r).await.unwrap();
            write_line(&mut w, &format!("{}\n", json!({
                "jsonrpc":"2.0","id":req["id"],
                "result": {"tools": [{"name": "search", "description": "search files", "inputSchema": {"type":"object"}}]},
            }))).await.unwrap();

            // tools/call
            let req = read_json_line(&mut r).await.unwrap();
            assert_eq!(req["params"]["name"], "search");
            write_line(&mut w, &format!("{}\n", json!({
                "jsonrpc":"2.0","id":req["id"],
                "result": {"content": [{"type":"text","text":"found 3 files"}], "isError": false},
            }))).await.unwrap();
        });

        let client = StdioMcpProcess::handshake(BufReader::new(client_read), client_write, None).await.unwrap();

        let tools = client.list_tools(Duration::from_secs(5)).await.unwrap();
        assert_eq!(tools.len(), 1);
        assert_eq!(tools[0].name, "search");
        assert_eq!(tools[0].description, "search files");

        let result = client.call_tool("search", json!({"query": "x"}), Duration::from_secs(5)).await.unwrap();
        assert_eq!(result.text, "found 3 files");
        assert!(!result.is_error);

        fake_server.await.unwrap();
    }

    #[tokio::test]
    async fn a_tool_call_error_response_is_surfaced_as_an_err() {
        // criterion 8: error handling
        let (client_side, server_side) = tokio::io::duplex(64 * 1024);
        let (client_read, client_write) = tokio::io::split(client_side);
        let (server_read, server_write) = tokio::io::split(server_side);

        tokio::spawn(async move {
            let mut r = BufReader::new(server_read);
            let mut w = server_write;
            let req = read_json_line(&mut r).await.unwrap();
            write_line(&mut w, &format!("{}\n", json!({"jsonrpc":"2.0","id":req["id"],"result":{}}))).await.unwrap();
            let _ = read_json_line(&mut r).await.unwrap();
            let req = read_json_line(&mut r).await.unwrap();
            write_line(&mut w, &format!("{}\n", json!({"jsonrpc":"2.0","id":req["id"],"error":{"code":-32601,"message":"unknown tool"}}))).await.unwrap();
        });

        let client = StdioMcpProcess::handshake(BufReader::new(client_read), client_write, None).await.unwrap();
        let err = client.call_tool("nope", json!({}), Duration::from_secs(5)).await.unwrap_err();
        assert!(err.to_string().contains("unknown tool"));
    }

    #[tokio::test]
    async fn a_tool_call_times_out_when_the_server_never_responds() {
        // criterion 8: timeout handling
        let (client_side, server_side) = tokio::io::duplex(64 * 1024);
        let (client_read, client_write) = tokio::io::split(client_side);
        let (server_read, server_write) = tokio::io::split(server_side);

        tokio::spawn(async move {
            let mut r = BufReader::new(server_read);
            let mut w = server_write;
            let req = read_json_line(&mut r).await.unwrap();
            write_line(&mut w, &format!("{}\n", json!({"jsonrpc":"2.0","id":req["id"],"result":{}}))).await.unwrap();
            let _ = read_json_line(&mut r).await.unwrap();
            // Never answer the next call — leak the server task deliberately;
            // it exits when the duplex pair is dropped at test end.
            let _ = read_json_line(&mut r).await;
            std::future::pending::<()>().await;
        });

        let client = StdioMcpProcess::handshake(BufReader::new(client_read), client_write, None).await.unwrap();
        let err = client.call_tool("slow", json!({}), Duration::from_millis(20)).await.unwrap_err();
        assert!(err.to_string().contains("timed out"));
    }

    #[tokio::test]
    async fn server_crash_surfaces_as_a_connection_closed_error() {
        // criterion 8: server crash (stream closes mid-call)
        let (client_side, server_side) = tokio::io::duplex(64 * 1024);
        let (client_read, client_write) = tokio::io::split(client_side);
        let (server_read, server_write) = tokio::io::split(server_side);

        tokio::spawn(async move {
            let mut r = BufReader::new(server_read);
            let mut w = server_write;
            let req = read_json_line(&mut r).await.unwrap();
            write_line(&mut w, &format!("{}\n", json!({"jsonrpc":"2.0","id":req["id"],"result":{}}))).await.unwrap();
            let _ = read_json_line(&mut r).await.unwrap();
            let _ = read_json_line(&mut r).await;
            // Drop the writer -> the client's background reader sees EOF.
            drop(w);
        });

        let client = StdioMcpProcess::handshake(BufReader::new(client_read), client_write, None).await.unwrap();
        let err = client.call_tool("anything", json!({}), Duration::from_secs(5)).await.unwrap_err();
        assert!(err.to_string().contains("closed"), "{err}");
    }

    #[tokio::test]
    async fn invalid_json_from_the_server_does_not_wedge_the_reader() {
        // criterion 8: invalid JSON handling — a garbage line makes the
        // background reader stop (it can't recover mid-stream framing from
        // one bad line), which in turn clears every pending call with a
        // connection-closed error — a fast, clear failure, not a silent
        // wedge where the caller just hangs until its timeout.
        let (client_side, server_side) = tokio::io::duplex(64 * 1024);
        let (client_read, client_write) = tokio::io::split(client_side);
        let (server_read, server_write) = tokio::io::split(server_side);

        tokio::spawn(async move {
            let mut r = BufReader::new(server_read);
            let mut w = server_write;
            let req = read_json_line(&mut r).await.unwrap();
            write_line(&mut w, &format!("{}\n", json!({"jsonrpc":"2.0","id":req["id"],"result":{}}))).await.unwrap();
            let _ = read_json_line(&mut r).await.unwrap();
            let _ = read_json_line(&mut r).await;
            write_line(&mut w, "not valid json\n").await.unwrap();
        });

        let client = StdioMcpProcess::handshake(BufReader::new(client_read), client_write, None).await.unwrap();
        let err = client.call_tool("anything", json!({}), Duration::from_secs(5)).await.unwrap_err();
        // The background reader dies on the bad line and clears every
        // pending call immediately — the caller sees a fast, clear error
        // rather than silently hanging until the timeout.
        assert!(err.to_string().contains("closed"), "{err}");
    }

    // --- criteria 3, 4, 6: McpContext pool ----------------------------------

    struct FakeMcpClient(Vec<McpToolDef>);
    #[async_trait]
    impl McpClient for FakeMcpClient {
        async fn list_tools(&self, _timeout: Duration) -> Result<Vec<McpToolDef>> {
            Ok(self.0.clone())
        }
        async fn call_tool(&self, name: &str, arguments: Value, _timeout: Duration) -> Result<McpToolCallResult> {
            Ok(McpToolCallResult { text: format!("called {name} with {arguments}"), is_error: false })
        }
    }

    struct FakeMcpSpawner(Vec<McpToolDef>);
    #[async_trait]
    impl McpSpawner for FakeMcpSpawner {
        async fn connect(&self, _config: &McpServerConfig) -> Result<Arc<dyn McpClient>> {
            Ok(Arc::new(FakeMcpClient(self.0.clone())))
        }
    }

    fn a_tool(name: &str) -> McpToolDef {
        McpToolDef { name: name.to_string(), description: format!("{name} tool"), input_schema: default_input_schema() }
    }

    fn local_config(name: &str) -> McpServerConfig {
        McpServerConfig { name: name.to_string(), transport: McpTransportConfig::Local { command: vec!["echo".into()] }, timeout: Duration::from_secs(5) }
    }

    #[tokio::test]
    async fn connect_all_registers_prefixed_tool_definitions() {
        let ctx = McpContext::new(Arc::new(FakeMcpSpawner(vec![a_tool("search"), a_tool("read")])));
        let results = ctx.connect_all(&[local_config("files")]).await;
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].0, "files");
        assert_eq!(*results[0].1.as_ref().unwrap(), 2);

        let defs = ctx.tool_definitions();
        let names: Vec<String> = defs.iter().map(|d| d.function.name.clone()).collect();
        assert!(names.contains(&"mcp_files_search".to_string()));
        assert!(names.contains(&"mcp_files_read".to_string()));
        assert!(ctx.is_connected("files"));
    }

    #[tokio::test]
    async fn dispatch_routes_to_the_right_connected_server() {
        let ctx = McpContext::new(Arc::new(FakeMcpSpawner(vec![a_tool("search")])));
        ctx.connect_all(&[local_config("files")]).await;

        let result = ctx.dispatch("mcp_files_search", json!({"q": "x"})).await.unwrap();
        assert!(result.text.contains("called search"));
    }

    #[tokio::test]
    async fn dispatch_errors_for_an_unknown_or_disconnected_server() {
        let ctx = McpContext::new(Arc::new(FakeMcpSpawner(vec![])));
        let err = ctx.dispatch("mcp_nosuch_tool", json!({})).await.unwrap_err();
        assert!(err.to_string().contains("no connected MCP server"));
    }

    #[tokio::test]
    async fn one_server_failing_to_connect_does_not_stop_the_others() {
        struct FlakySpawner;
        #[async_trait]
        impl McpSpawner for FlakySpawner {
            async fn connect(&self, config: &McpServerConfig) -> Result<Arc<dyn McpClient>> {
                if config.name == "broken" {
                    bail!("connection refused");
                }
                Ok(Arc::new(FakeMcpClient(vec![a_tool("ok")])))
            }
        }
        let ctx = McpContext::new(Arc::new(FlakySpawner));
        let results = ctx.connect_all(&[local_config("broken"), local_config("fine")]).await;
        assert!(results[0].1.is_err());
        assert!(results[1].1.is_ok());
        assert!(!ctx.is_connected("broken"));
        assert!(ctx.is_connected("fine"));
    }
}
