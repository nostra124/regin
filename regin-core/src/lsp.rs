//! LSP diagnostics feedback loop (FEAT-078 / DISC-021).
//!
//! A minimal Language Server Protocol client: message framing over stdio
//! (`Content-Length` headers, JSON-RPC 2.0 bodies), `textDocument/
//! publishDiagnostics` parsing, per-file debouncing, and a pool of spawned
//! language-server processes recycled after an idle timeout.
//!
//! **Layered like every other real-I/O integration in this crate** (`bus`,
//! `push`, `llm`): the orchestration (debounce, pool eviction, language
//! detection, command resolution) is pure/injectable and unit-tested with
//! fakes; [`ProcessLspClient`] — the part that actually spawns
//! rust-analyzer/typescript-language-server and speaks JSON-RPC over its
//! stdio — is real but thin, exercised by one integration test against the
//! genuine `rust-analyzer` binary rather than a battery of unit tests.
//!
//! **Simplifications, documented rather than hidden**: the `initialize`
//! handshake assumes the language server's very first stdout message is the
//! `initialize` response (true in practice for a freshly spawned server);
//! after that, an unbounded background task only ever looks for
//! `textDocument/publishDiagnostics` notifications and silently drops any
//! other server-initiated request or notification (e.g.
//! `window/workDoneProgress/create`) — this client is a diagnostics feed,
//! not a full editor-side LSP implementation.

use anyhow::{Context, Result, bail};
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader};
use tokio::process::{Child, Command};

use crate::db;

// ---------------------------------------------------------------------------
// Wire framing (acceptance criterion 1)
// ---------------------------------------------------------------------------

/// Frame a JSON-RPC body with its `Content-Length` header, ready to write to
/// a language server's stdin.
pub fn encode_message(body: &str) -> Vec<u8> {
    let mut out = format!("Content-Length: {}\r\n\r\n", body.len()).into_bytes();
    out.extend_from_slice(body.as_bytes());
    out
}

/// Read one framed JSON-RPC message from an LSP stream: headers up to a
/// blank line, then exactly `Content-Length` bytes of body.
pub async fn read_message<R: AsyncBufRead + Unpin>(reader: &mut R) -> Result<String> {
    let mut content_length: Option<usize> = None;
    loop {
        let mut line = String::new();
        let n = reader.read_line(&mut line).await?;
        if n == 0 {
            bail!("LSP stream closed while reading headers");
        }
        let line = line.trim_end_matches(['\r', '\n']);
        if line.is_empty() {
            break;
        }
        if let Some(v) = line.strip_prefix("Content-Length:") {
            content_length = Some(v.trim().parse().context("bad Content-Length header")?);
        }
    }
    let len = content_length.context("LSP message missing Content-Length header")?;
    let mut buf = vec![0u8; len];
    reader.read_exact(&mut buf).await?;
    Ok(String::from_utf8(buf)?)
}

use tokio::io::AsyncBufRead;

// ---------------------------------------------------------------------------
// Diagnostics
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Position {
    pub line: u32,
    pub character: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Range {
    pub start: Position,
    pub end: Position,
}

/// LSP's diagnostic severity ladder (1 = Error .. 4 = Hint). Declaration
/// order intentionally does *not* imply `Ord` here — LSP severities aren't
/// compared numerically anywhere in this module, only displayed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Severity {
    Error,
    Warning,
    Information,
    Hint,
}

impl Severity {
    fn from_lsp_number(n: u64) -> Severity {
        match n {
            1 => Severity::Error,
            2 => Severity::Warning,
            3 => Severity::Information,
            _ => Severity::Hint,
        }
    }

    pub fn as_str(&self) -> &'static str {
        match self {
            Severity::Error => "error",
            Severity::Warning => "warning",
            Severity::Information => "information",
            Severity::Hint => "hint",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Diagnostic {
    pub range: Range,
    pub severity: Severity,
    pub message: String,
    pub source: Option<String>,
}

fn uri_to_path(uri: &str) -> String {
    uri.strip_prefix("file://").unwrap_or(uri).to_string()
}

fn path_to_uri(path: &str) -> String {
    let p = std::path::Path::new(path);
    if p.is_absolute() {
        format!("file://{path}")
    } else {
        match std::env::current_dir() {
            Ok(cwd) => format!("file://{}", cwd.join(p).display()),
            Err(_) => format!("file://{path}"),
        }
    }
}

/// Parse a `textDocument/publishDiagnostics` notification body into
/// `(path, diagnostics)`. `None` for anything else (a different
/// notification, a request, a response, malformed JSON).
pub fn parse_publish_diagnostics(json: &str) -> Option<(String, Vec<Diagnostic>)> {
    let v: Value = serde_json::from_str(json).ok()?;
    if v.get("method")?.as_str()? != "textDocument/publishDiagnostics" {
        return None;
    }
    let params = v.get("params")?;
    let path = uri_to_path(params.get("uri")?.as_str()?);
    let diagnostics = params
        .get("diagnostics")?
        .as_array()?
        .iter()
        .filter_map(|d| {
            let range: Range = serde_json::from_value(d.get("range")?.clone()).ok()?;
            let severity = d.get("severity").and_then(Value::as_u64).map(Severity::from_lsp_number).unwrap_or(Severity::Error);
            let message = d.get("message")?.as_str()?.to_string();
            let source = d.get("source").and_then(Value::as_str).map(str::to_string);
            Some(Diagnostic { range, severity, message, source })
        })
        .collect();
    Some((path, diagnostics))
}

/// Render diagnostics as the text appended to a tool result.
pub fn render_diagnostics(path: &str, diagnostics: &[Diagnostic]) -> String {
    if diagnostics.is_empty() {
        return format!("\n\n[lsp] {path}: no diagnostics");
    }
    let mut out = format!("\n\n[lsp] {path}: {} diagnostic(s)", diagnostics.len());
    for d in diagnostics {
        out += &format!(
            "\n  {}:{}: {} [{}]{}",
            d.range.start.line + 1,
            d.range.start.character + 1,
            d.message,
            d.severity.as_str(),
            d.source.as_deref().map(|s| format!(" ({s})")).unwrap_or_default(),
        );
    }
    out
}

// ---------------------------------------------------------------------------
// Language detection + server command resolution (acceptance criteria 1, 6)
// ---------------------------------------------------------------------------

/// Detect a language from a file's extension. `None` means no language
/// server is known for it.
pub fn detect_language(path: &str) -> Option<&'static str> {
    let ext = std::path::Path::new(path).extension()?.to_str()?;
    match ext {
        "rs" => Some("rust"),
        "ts" | "tsx" => Some("typescript"),
        "js" | "jsx" | "mjs" | "cjs" => Some("javascript"),
        _ => None,
    }
}

/// The built-in command for a language, if regin ships a default for it.
pub fn default_command(language: &str) -> Option<Vec<String>> {
    match language {
        "rust" => Some(vec!["rust-analyzer".to_string()]),
        "typescript" | "javascript" => Some(vec!["typescript-language-server".to_string(), "--stdio".to_string()]),
        _ => None,
    }
}

/// Resolve the command to spawn for `language`: a `lsp.<language>.command`
/// setting override (space-separated argv), falling back to
/// [`default_command`]. `None` means no server is available for this
/// language, built-in or configured.
pub fn resolve_command(conn: &rusqlite::Connection, language: &str) -> Result<Option<Vec<String>>> {
    let key = format!("lsp.{language}.command");
    let configured = db::setting_get(conn, &key)?;
    if !configured.trim().is_empty() {
        return Ok(Some(configured.split_whitespace().map(str::to_string).collect()));
    }
    Ok(default_command(language))
}

// ---------------------------------------------------------------------------
// Debounce (acceptance criterion 3)
// ---------------------------------------------------------------------------

/// Per-file debounce: collapses rapid successive automatic diagnostics
/// triggers within `window` of each other.
#[derive(Debug, Default)]
pub struct Debouncer {
    last_run: HashMap<String, DateTime<Utc>>,
}

impl Debouncer {
    pub fn new() -> Self {
        Self::default()
    }

    /// Whether a diagnostics run for `path` should happen at `now`. If it
    /// should, `now` is recorded as the new last-run time in the same call
    /// — the caller never needs a separate "mark as run" step.
    pub fn should_run(&mut self, path: &str, now: DateTime<Utc>, window: chrono::Duration) -> bool {
        match self.last_run.get(path) {
            Some(last) if now - *last < window => false,
            _ => {
                self.last_run.insert(path.to_string(), now);
                true
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Client trait + pool (acceptance criterion 5)
// ---------------------------------------------------------------------------

/// A running language server's diagnostics interface. Injectable so pool
/// and orchestration logic never need to spawn a real process in tests.
#[async_trait]
pub trait LspClient: Send + Sync {
    async fn diagnostics(&self, path: &str) -> Result<Vec<Diagnostic>>;
}

struct PooledServer {
    client: Arc<dyn LspClient>,
    last_used: DateTime<Utc>,
}

/// Spawned language servers, keyed by `"<language>@<workspace_root>"` so
/// different projects (or different languages) each get their own process.
#[derive(Default)]
pub struct LspPool {
    servers: HashMap<String, PooledServer>,
}

fn pool_key(language: &str, workspace_root: &str) -> String {
    format!("{language}@{workspace_root}")
}

impl LspPool {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn insert(&mut self, language: &str, workspace_root: &str, client: Arc<dyn LspClient>, now: DateTime<Utc>) {
        self.servers.insert(pool_key(language, workspace_root), PooledServer { client, last_used: now });
    }

    /// A pooled client for `language`/`workspace_root`, if one is running —
    /// touches its last-used time so it doesn't look idle.
    pub fn get(&mut self, language: &str, workspace_root: &str, now: DateTime<Utc>) -> Option<Arc<dyn LspClient>> {
        let entry = self.servers.get_mut(&pool_key(language, workspace_root))?;
        entry.last_used = now;
        Some(entry.client.clone())
    }

    /// Evict every server idle for at least `timeout` as of `now`. Returns
    /// the pool keys evicted (for logging/tests).
    pub fn evict_idle(&mut self, now: DateTime<Utc>, timeout: chrono::Duration) -> Vec<String> {
        let idle: Vec<String> = self.servers.iter().filter(|(_, s)| now - s.last_used >= timeout).map(|(k, _)| k.clone()).collect();
        for key in &idle {
            self.servers.remove(key);
        }
        idle
    }

    pub fn len(&self) -> usize {
        self.servers.len()
    }

    pub fn is_empty(&self) -> bool {
        self.servers.is_empty()
    }
}

/// Spawns a real [`LspClient`] for a language + command. Injectable so pool
/// tests never spawn a real process; [`ProcessLspSpawner`] is the
/// production implementation.
#[async_trait]
pub trait LspSpawner: Send + Sync {
    async fn spawn(&self, command: &[String], workspace_root: &str) -> Result<Arc<dyn LspClient>>;
}

/// Get a pooled client for `language`/`workspace_root`, spawning one via
/// `spawner` if none is running yet.
pub async fn get_or_spawn_client(
    pool: &Mutex<LspPool>,
    spawner: &dyn LspSpawner,
    language: &str,
    command: &[String],
    workspace_root: &str,
    now: DateTime<Utc>,
) -> Result<Arc<dyn LspClient>> {
    if let Some(client) = pool.lock().unwrap().get(language, workspace_root, now) {
        return Ok(client);
    }
    let client = spawner.spawn(command, workspace_root).await?;
    pool.lock().unwrap().insert(language, workspace_root, client.clone(), now);
    Ok(client)
}

// ---------------------------------------------------------------------------
// Orchestration: what tools.rs actually calls
// ---------------------------------------------------------------------------

/// Everything a daemon session needs to run diagnostics across the whole
/// process's lifetime — held once, not per-call.
pub struct LspContext {
    pub pool: Mutex<LspPool>,
    pub debouncer: Mutex<Debouncer>,
    pub spawner: Arc<dyn LspSpawner>,
}

impl LspContext {
    pub fn new(spawner: Arc<dyn LspSpawner>) -> Self {
        Self { pool: Mutex::new(LspPool::new()), debouncer: Mutex::new(Debouncer::new()), spawner }
    }
}

/// What a diagnostics request resolved to, computed synchronously from
/// settings + debounce state. Split out from the async spawn/fetch step
/// specifically so a caller never has to hold a `Connection` lock across an
/// `.await` — `rusqlite::Connection` isn't `Sync`, so a `MutexGuard` over it
/// isn't `Send`, and can't be live across an await point in an async
/// context (the same hazard the scoped-lock comments elsewhere in this
/// file work around).
#[derive(Debug, Clone, PartialEq)]
pub enum DiagnosticsPlan {
    /// LSP disabled, no language known for this path, or (automatic
    /// triggers only) still inside the debounce window.
    Skip,
    Run { language: &'static str, command: Vec<String>, idle_timeout: chrono::Duration },
}

/// Decide what a diagnostics request for `path` should do (acceptance
/// criteria 2, 4, 7) — pure/synchronous, safe to compute while holding a
/// `Connection` lock. `respect_debounce` is `true` for the automatic
/// post-edit trigger and `false` for the on-demand `diagnostics` tool.
pub fn plan_diagnostics(
    conn: &rusqlite::Connection,
    ctx: &LspContext,
    path: &str,
    now: DateTime<Utc>,
    respect_debounce: bool,
) -> Result<DiagnosticsPlan> {
    if db::setting_get(conn, "lsp.enabled")? != "true" {
        return Ok(DiagnosticsPlan::Skip);
    }
    let Some(language) = detect_language(path) else {
        return Ok(DiagnosticsPlan::Skip);
    };
    if respect_debounce {
        let debounce_ms: i64 = db::setting_get(conn, "lsp.debounce_ms")?.parse().unwrap_or(500);
        if !ctx.debouncer.lock().unwrap().should_run(path, now, chrono::Duration::milliseconds(debounce_ms)) {
            return Ok(DiagnosticsPlan::Skip);
        }
    }
    let Some(command) = resolve_command(conn, language)? else {
        return Ok(DiagnosticsPlan::Skip);
    };
    let idle_timeout_secs: i64 = db::setting_get(conn, "lsp.idle_timeout_secs")?.parse().unwrap_or(300);
    Ok(DiagnosticsPlan::Run { language, command, idle_timeout: chrono::Duration::seconds(idle_timeout_secs) })
}

/// Carry out a [`DiagnosticsPlan`] — the async half, needing no
/// `Connection` at all. `Ok(None)` for [`DiagnosticsPlan::Skip`].
pub async fn run_diagnostics_plan(
    ctx: &LspContext,
    plan: DiagnosticsPlan,
    path: &str,
    workspace_root: &str,
    now: DateTime<Utc>,
) -> Result<Option<Vec<Diagnostic>>> {
    let DiagnosticsPlan::Run { language, command, idle_timeout } = plan else {
        return Ok(None);
    };
    ctx.pool.lock().unwrap().evict_idle(now, idle_timeout);
    let client = get_or_spawn_client(&ctx.pool, ctx.spawner.as_ref(), language, &command, workspace_root, now).await?;
    Ok(Some(client.diagnostics(path).await?))
}

/// Convenience: plan then run in one call, for callers that already hold
/// (and can release before returning) a `&Connection` outside of any async
/// boundary — e.g. every test in this module. Production callers (`tools.rs`)
/// call `plan_diagnostics`/`run_diagnostics_plan` separately so the
/// connection lock is released before the async half starts.
pub async fn fetch_diagnostics(
    conn: &rusqlite::Connection,
    ctx: &LspContext,
    path: &str,
    workspace_root: &str,
    now: DateTime<Utc>,
    respect_debounce: bool,
) -> Result<Option<Vec<Diagnostic>>> {
    let plan = plan_diagnostics(conn, ctx, path, now, respect_debounce)?;
    run_diagnostics_plan(ctx, plan, path, workspace_root, now).await
}

// ---------------------------------------------------------------------------
// The real client (thin — see the module doc comment's simplifications)
// ---------------------------------------------------------------------------

/// A real, spawned language server process speaking LSP over stdio. Only
/// ever issues one real request (`initialize`, id `1`) — everything after
/// that is fire-and-forget notifications, so there's no request-id counter
/// or response dispatcher to maintain.
pub struct ProcessLspClient {
    stdin: tokio::sync::Mutex<tokio::process::ChildStdin>,
    diagnostics: Arc<Mutex<HashMap<String, Vec<Diagnostic>>>>,
    open_versions: Mutex<HashMap<String, i64>>,
    _child: Child,
}

impl ProcessLspClient {
    /// Spawn `command`, perform the `initialize`/`initialized` handshake,
    /// and start a background task that watches for `publishDiagnostics`
    /// notifications for the rest of the process's life.
    pub async fn spawn(command: &[String], workspace_root: &str) -> Result<Self> {
        let (prog, args) = command.split_first().context("empty LSP command")?;
        let mut child = Command::new(prog)
            .args(args)
            .current_dir(workspace_root)
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::null())
            .kill_on_drop(true)
            .spawn()
            .with_context(|| format!("spawning LSP server {prog:?}"))?;

        let mut stdin = child.stdin.take().context("no stdin on spawned LSP server")?;
        let stdout = child.stdout.take().context("no stdout on spawned LSP server")?;
        let mut reader = BufReader::new(stdout);

        let root_uri = path_to_uri(workspace_root);
        let init_request = json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "initialize",
            "params": {
                "processId": std::process::id(),
                "rootUri": root_uri,
                "capabilities": {},
            },
        });
        stdin.write_all(&encode_message(&init_request.to_string())).await?;
        stdin.flush().await?;

        // The handshake's response is assumed to be the server's first
        // message (documented simplification, module doc comment).
        let _init_response = read_message(&mut reader).await.context("reading LSP initialize response")?;

        let initialized = json!({"jsonrpc": "2.0", "method": "initialized", "params": {}});
        stdin.write_all(&encode_message(&initialized.to_string())).await?;
        stdin.flush().await?;

        let diagnostics: Arc<Mutex<HashMap<String, Vec<Diagnostic>>>> = Arc::new(Mutex::new(HashMap::new()));
        let diagnostics_bg = diagnostics.clone();
        tokio::spawn(async move {
            while let Ok(msg) = read_message(&mut reader).await {
                if let Some((path, diags)) = parse_publish_diagnostics(&msg) {
                    diagnostics_bg.lock().unwrap().insert(path, diags);
                }
            }
        });

        Ok(Self {
            stdin: tokio::sync::Mutex::new(stdin),
            diagnostics,
            open_versions: Mutex::new(HashMap::new()),
            _child: child,
        })
    }

    async fn notify_did_open_or_change(&self, path: &str) -> Result<()> {
        let text = std::fs::read_to_string(path).with_context(|| format!("reading {path} to send to the language server"))?;
        let uri = path_to_uri(path);
        // Scoped so the (non-Send) `MutexGuard` is dropped well before the
        // `self.stdin.lock().await` below — it must never be live across
        // an await point for this future to stay `Send`.
        let (is_first_open, version) = {
            let mut versions = self.open_versions.lock().unwrap();
            let entry = versions.entry(path.to_string()).or_insert(0);
            let is_first_open = *entry == 0;
            *entry += 1;
            (is_first_open, *entry)
        };

        let language_id = detect_language(path).unwrap_or("plaintext");
        let notification = if is_first_open {
            json!({
                "jsonrpc": "2.0",
                "method": "textDocument/didOpen",
                "params": {"textDocument": {"uri": uri, "languageId": language_id, "version": version, "text": text}},
            })
        } else {
            json!({
                "jsonrpc": "2.0",
                "method": "textDocument/didChange",
                "params": {
                    "textDocument": {"uri": uri, "version": version},
                    "contentChanges": [{"text": text}],
                },
            })
        };

        let mut stdin = self.stdin.lock().await;
        stdin.write_all(&encode_message(&notification.to_string())).await?;
        stdin.flush().await?;
        Ok(())
    }
}

#[async_trait]
impl LspClient for ProcessLspClient {
    async fn diagnostics(&self, path: &str) -> Result<Vec<Diagnostic>> {
        self.notify_did_open_or_change(path).await?;

        // Diagnostics arrive asynchronously as a notification once the
        // server finishes analysing the change — poll briefly rather than
        // blocking indefinitely.
        let deadline = tokio::time::Instant::now() + tokio::time::Duration::from_secs(10);
        loop {
            // Scoped so the `MutexGuard` never overlaps the `.await` below.
            let found = { self.diagnostics.lock().unwrap().get(path).cloned() };
            if let Some(diags) = found {
                return Ok(diags);
            }
            if tokio::time::Instant::now() >= deadline {
                return Ok(Vec::new());
            }
            tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
        }
    }
}

/// Production [`LspSpawner`]: spawns a real process per call.
pub struct ProcessLspSpawner;

#[async_trait]
impl LspSpawner for ProcessLspSpawner {
    async fn spawn(&self, command: &[String], workspace_root: &str) -> Result<Arc<dyn LspClient>> {
        Ok(Arc::new(ProcessLspClient::spawn(command, workspace_root).await?))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // --- acceptance criterion 1: framing --------------------------------

    #[tokio::test]
    async fn encode_then_read_message_round_trips() {
        let body = r#"{"jsonrpc":"2.0","id":1,"method":"initialize"}"#;
        let framed = encode_message(body);
        let mut reader = BufReader::new(std::io::Cursor::new(framed));
        let read_back = read_message(&mut reader).await.unwrap();
        assert_eq!(read_back, body);
    }

    #[tokio::test]
    async fn read_message_handles_multiple_messages_back_to_back() {
        let mut buf = Vec::new();
        buf.extend(encode_message("first"));
        buf.extend(encode_message("second"));
        let mut reader = BufReader::new(std::io::Cursor::new(buf));
        assert_eq!(read_message(&mut reader).await.unwrap(), "first");
        assert_eq!(read_message(&mut reader).await.unwrap(), "second");
    }

    #[tokio::test]
    async fn read_message_errors_on_a_closed_stream() {
        let mut reader = BufReader::new(std::io::Cursor::new(Vec::<u8>::new()));
        assert!(read_message(&mut reader).await.is_err());
    }

    #[tokio::test]
    async fn read_message_errors_without_a_content_length_header() {
        let mut reader = BufReader::new(std::io::Cursor::new(b"\r\n".to_vec()));
        assert!(read_message(&mut reader).await.is_err());
    }

    // --- diagnostics parsing ---------------------------------------------

    #[test]
    fn parse_publish_diagnostics_extracts_path_and_entries() {
        let json = r#"{
            "jsonrpc": "2.0",
            "method": "textDocument/publishDiagnostics",
            "params": {
                "uri": "file:///repo/src/main.rs",
                "diagnostics": [
                    {"range": {"start": {"line": 4, "character": 8}, "end": {"line": 4, "character": 12}},
                     "severity": 1, "message": "expected `;`", "source": "rustc"}
                ]
            }
        }"#;
        let (path, diags) = parse_publish_diagnostics(json).unwrap();
        assert_eq!(path, "/repo/src/main.rs");
        assert_eq!(diags.len(), 1);
        assert_eq!(diags[0].severity, Severity::Error);
        assert_eq!(diags[0].message, "expected `;`");
        assert_eq!(diags[0].source.as_deref(), Some("rustc"));
        assert_eq!(diags[0].range.start, Position { line: 4, character: 8 });
    }

    #[test]
    fn parse_publish_diagnostics_ignores_other_methods() {
        let json = r#"{"jsonrpc":"2.0","method":"window/logMessage","params":{}}"#;
        assert!(parse_publish_diagnostics(json).is_none());
    }

    #[test]
    fn parse_publish_diagnostics_ignores_malformed_json() {
        assert!(parse_publish_diagnostics("not json").is_none());
    }

    #[test]
    fn parse_publish_diagnostics_defaults_severity_when_absent() {
        let json = r#"{
            "jsonrpc": "2.0", "method": "textDocument/publishDiagnostics",
            "params": {"uri": "file:///a.rs", "diagnostics": [
                {"range": {"start": {"line": 0, "character": 0}, "end": {"line": 0, "character": 1}}, "message": "m"}
            ]}
        }"#;
        let (_, diags) = parse_publish_diagnostics(json).unwrap();
        assert_eq!(diags[0].severity, Severity::Error);
    }

    #[test]
    fn render_diagnostics_reports_clean_and_dirty_files() {
        assert!(render_diagnostics("f.rs", &[]).contains("no diagnostics"));
        let d = Diagnostic {
            range: Range { start: Position { line: 0, character: 0 }, end: Position { line: 0, character: 1 } },
            severity: Severity::Warning,
            message: "unused variable".into(),
            source: Some("clippy".into()),
        };
        let out = render_diagnostics("f.rs", &[d]);
        assert!(out.contains("unused variable"));
        assert!(out.contains("warning"));
        assert!(out.contains("clippy"));
    }

    // --- acceptance criteria 1, 6: language detection + command resolution -

    #[test]
    fn detect_language_covers_rust_and_typescript_family() {
        assert_eq!(detect_language("src/main.rs"), Some("rust"));
        assert_eq!(detect_language("app.ts"), Some("typescript"));
        assert_eq!(detect_language("app.tsx"), Some("typescript"));
        assert_eq!(detect_language("app.js"), Some("javascript"));
        assert_eq!(detect_language("README.md"), None);
        assert_eq!(detect_language("no-extension"), None);
    }

    #[test]
    fn default_command_known_and_unknown_languages() {
        assert_eq!(default_command("rust"), Some(vec!["rust-analyzer".to_string()]));
        assert!(default_command("python").is_none());
    }

    fn conn() -> rusqlite::Connection {
        let c = rusqlite::Connection::open_in_memory().unwrap();
        db::init_schema(&c).unwrap();
        c
    }

    #[test]
    fn resolve_command_prefers_a_configured_override() {
        let c = conn();
        assert_eq!(resolve_command(&c, "rust").unwrap(), Some(vec!["rust-analyzer".to_string()]));
        db::setting_set(&c, "lsp.rust.command", "my-analyzer --custom-flag").unwrap();
        assert_eq!(resolve_command(&c, "rust").unwrap(), Some(vec!["my-analyzer".to_string(), "--custom-flag".to_string()]));
    }

    #[test]
    fn resolve_command_is_none_for_an_unconfigured_unknown_language() {
        let c = conn();
        assert_eq!(resolve_command(&c, "python").unwrap(), None);
        db::setting_set(&c, "lsp.python.command", "pylsp").unwrap();
        assert_eq!(resolve_command(&c, "python").unwrap(), Some(vec!["pylsp".to_string()]));
    }

    // --- acceptance criterion 3: debounce ---------------------------------

    fn t(minute: u32, second: u32) -> DateTime<Utc> {
        "2026-01-01T00:00:00Z".parse::<DateTime<Utc>>().unwrap() + chrono::Duration::minutes(minute as i64) + chrono::Duration::seconds(second as i64)
    }

    #[test]
    fn debounce_suppresses_reruns_within_the_window_then_allows_after() {
        let mut d = Debouncer::new();
        assert!(d.should_run("f.rs", t(0, 0), chrono::Duration::milliseconds(500)));
        assert!(!d.should_run("f.rs", t(0, 0), chrono::Duration::milliseconds(500)), "same instant, still within window");
        assert!(d.should_run("f.rs", t(0, 1), chrono::Duration::milliseconds(500)), "1s later clears a 500ms window");
    }

    #[test]
    fn debounce_is_per_file() {
        let mut d = Debouncer::new();
        assert!(d.should_run("a.rs", t(0, 0), chrono::Duration::seconds(1)));
        assert!(d.should_run("b.rs", t(0, 0), chrono::Duration::seconds(1)), "a different file isn't debounced by a.rs's run");
    }

    // --- acceptance criterion 5: process lifecycle (pool eviction) -------

    struct FixedLspClient(Vec<Diagnostic>);
    #[async_trait]
    impl LspClient for FixedLspClient {
        async fn diagnostics(&self, _path: &str) -> Result<Vec<Diagnostic>> {
            Ok(self.0.clone())
        }
    }

    #[test]
    fn pool_get_returns_none_when_nothing_is_spawned() {
        let mut pool = LspPool::new();
        assert!(pool.get("rust", "/repo", t(0, 0)).is_none());
    }

    #[test]
    fn pool_insert_then_get_returns_the_same_client_and_touches_last_used() {
        let mut pool = LspPool::new();
        let client: Arc<dyn LspClient> = Arc::new(FixedLspClient(vec![]));
        pool.insert("rust", "/repo", client, t(0, 0));
        assert!(pool.get("rust", "/repo", t(0, 30)).is_some());
        // still alive just under a 1-minute idle timeout measured from the touch
        assert!(pool.evict_idle(t(0, 50), chrono::Duration::minutes(1)).is_empty());
    }

    #[test]
    fn pool_keys_by_language_and_workspace_root_separately() {
        let mut pool = LspPool::new();
        pool.insert("rust", "/repo-a", Arc::new(FixedLspClient(vec![])), t(0, 0));
        assert!(pool.get("rust", "/repo-b", t(0, 0)).is_none(), "different workspace root, different server");
        assert!(pool.get("typescript", "/repo-a", t(0, 0)).is_none(), "different language, different server");
        assert!(pool.get("rust", "/repo-a", t(0, 0)).is_some());
    }

    #[test]
    fn evict_idle_removes_only_servers_past_the_timeout() {
        let mut pool = LspPool::new();
        pool.insert("rust", "/repo", Arc::new(FixedLspClient(vec![])), t(0, 0));
        pool.insert("typescript", "/repo", Arc::new(FixedLspClient(vec![])), t(4, 0));

        // 5 minutes later: rust (idle 5m) evicted, typescript (idle 1m) survives.
        let evicted = pool.evict_idle(t(5, 0), chrono::Duration::minutes(5));
        assert_eq!(evicted, vec!["rust@/repo".to_string()]);
        assert_eq!(pool.len(), 1);
        assert!(pool.get("typescript", "/repo", t(5, 0)).is_some());
    }

    struct SpyingSpawner {
        calls: std::sync::Mutex<Vec<(Vec<String>, String)>>,
    }
    #[async_trait]
    impl LspSpawner for SpyingSpawner {
        async fn spawn(&self, command: &[String], workspace_root: &str) -> Result<Arc<dyn LspClient>> {
            self.calls.lock().unwrap().push((command.to_vec(), workspace_root.to_string()));
            Ok(Arc::new(FixedLspClient(vec![])))
        }
    }

    #[tokio::test]
    async fn get_or_spawn_client_spawns_once_then_reuses_the_pooled_client() {
        let pool = Mutex::new(LspPool::new());
        let spawner = SpyingSpawner { calls: std::sync::Mutex::new(vec![]) };
        let cmd = vec!["rust-analyzer".to_string()];

        get_or_spawn_client(&pool, &spawner, "rust", &cmd, "/repo", t(0, 0)).await.unwrap();
        get_or_spawn_client(&pool, &spawner, "rust", &cmd, "/repo", t(0, 1)).await.unwrap();

        assert_eq!(spawner.calls.lock().unwrap().len(), 1, "second call reuses the pooled client");
    }

    // --- fetch_diagnostics orchestration (acceptance criteria 2, 4, 7) ---

    #[tokio::test]
    async fn fetch_diagnostics_is_a_noop_when_lsp_is_disabled() {
        let c = conn();
        let ctx = LspContext::new(Arc::new(SpyingSpawner { calls: std::sync::Mutex::new(vec![]) }));
        let result = fetch_diagnostics(&c, &ctx, "src/main.rs", "/repo", t(0, 0), true).await.unwrap();
        assert!(result.is_none(), "lsp.enabled defaults to false");
    }

    #[tokio::test]
    async fn fetch_diagnostics_is_a_noop_for_an_unknown_language() {
        let c = conn();
        db::setting_set(&c, "lsp.enabled", "true").unwrap();
        let ctx = LspContext::new(Arc::new(SpyingSpawner { calls: std::sync::Mutex::new(vec![]) }));
        let result = fetch_diagnostics(&c, &ctx, "README.md", "/repo", t(0, 0), true).await.unwrap();
        assert!(result.is_none());
    }

    #[tokio::test]
    async fn fetch_diagnostics_spawns_and_returns_results_when_enabled() {
        let c = conn();
        db::setting_set(&c, "lsp.enabled", "true").unwrap();
        let ctx = LspContext::new(Arc::new(SpyingSpawner { calls: std::sync::Mutex::new(vec![]) }));
        let result = fetch_diagnostics(&c, &ctx, "src/main.rs", "/repo", t(0, 0), true).await.unwrap();
        assert_eq!(result, Some(vec![]));
    }

    #[tokio::test]
    async fn fetch_diagnostics_respects_the_debounce_window_only_when_asked() {
        let c = conn();
        db::setting_set(&c, "lsp.enabled", "true").unwrap();
        let ctx = LspContext::new(Arc::new(SpyingSpawner { calls: std::sync::Mutex::new(vec![]) }));

        assert!(fetch_diagnostics(&c, &ctx, "src/main.rs", "/repo", t(0, 0), true).await.unwrap().is_some());
        assert!(
            fetch_diagnostics(&c, &ctx, "src/main.rs", "/repo", t(0, 0), true).await.unwrap().is_none(),
            "second automatic call within the window is debounced"
        );
        assert!(
            fetch_diagnostics(&c, &ctx, "src/main.rs", "/repo", t(0, 0), false).await.unwrap().is_some(),
            "on-demand calls skip the debounce gate"
        );
    }

    // --- plan_diagnostics / run_diagnostics_plan: the split tools.rs actually
    // uses, so a Connection lock never has to span an await -----------------

    #[test]
    fn plan_diagnostics_is_pure_and_synchronous_and_skips_when_disabled() {
        let c = conn();
        let ctx = LspContext::new(Arc::new(SpyingSpawner { calls: std::sync::Mutex::new(vec![]) }));
        let plan = plan_diagnostics(&c, &ctx, "src/main.rs", t(0, 0), true).unwrap();
        assert_eq!(plan, DiagnosticsPlan::Skip);
    }

    #[test]
    fn plan_diagnostics_resolves_a_run_plan_when_enabled_and_known() {
        let c = conn();
        db::setting_set(&c, "lsp.enabled", "true").unwrap();
        let ctx = LspContext::new(Arc::new(SpyingSpawner { calls: std::sync::Mutex::new(vec![]) }));
        let plan = plan_diagnostics(&c, &ctx, "src/main.rs", t(0, 0), true).unwrap();
        assert_eq!(plan, DiagnosticsPlan::Run { language: "rust", command: vec!["rust-analyzer".to_string()], idle_timeout: chrono::Duration::seconds(300) });
    }

    #[tokio::test]
    async fn run_diagnostics_plan_is_a_noop_for_skip_and_spawns_for_run() {
        let ctx = LspContext::new(Arc::new(SpyingSpawner { calls: std::sync::Mutex::new(vec![]) }));

        assert_eq!(run_diagnostics_plan(&ctx, DiagnosticsPlan::Skip, "src/main.rs", "/repo", t(0, 0)).await.unwrap(), None);

        let plan = DiagnosticsPlan::Run { language: "rust", command: vec!["rust-analyzer".to_string()], idle_timeout: chrono::Duration::seconds(300) };
        let result = run_diagnostics_plan(&ctx, plan, "src/main.rs", "/repo", t(0, 0)).await.unwrap();
        assert_eq!(result, Some(vec![]));
        assert_eq!(ctx.pool.lock().unwrap().len(), 1, "the server is pooled for reuse");
    }
}
