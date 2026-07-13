//! Daemon RPC transport (FEAT-070 / DISC-020).
//!
//! `regin-cli` talks to `regind` over a Unix socket with a simple
//! one-request-per-line JSON protocol. The [`Transport`] trait abstracts that
//! round-trip so command logic in `main.rs` can be unit-tested against a
//! [`FakeTransport`] without a running daemon; [`SocketTransport`] is the real
//! production implementation.

use std::process::Command;

use anyhow::{anyhow, Context, Result};
use regin_core::{config, db, protocol::{Request, Response}};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::UnixStream;

/// Responses that end a request/response exchange for [`Transport::request_stream`].
fn is_terminal(resp: &Response) -> bool {
    matches!(resp, Response::StreamDone { .. } | Response::TaskResult { .. } | Response::Error { .. })
}

/// Abstraction over the daemon RPC channel.
pub trait Transport {
    /// A single request/response round-trip. An `Error` response is turned
    /// into `Err` (matching the historic `rpc()` behaviour).
    async fn request(&self, req: &Request) -> Result<Response>;

    /// Send one request and invoke `on_event` for every response as it
    /// arrives, until (and including) a terminal one (`StreamDone` /
    /// `TaskResult` / `Error`). Used by multi-event commands (chat turns,
    /// task-exec tool events) — the callback is what keeps production output
    /// live (each event prints as it's read off the socket) while still
    /// letting `FakeTransport` replay a canned sequence for tests.
    async fn request_stream<F: FnMut(&Response) + Send>(&self, req: &Request, on_event: F) -> Result<Vec<Response>>;
}

/// Production transport: connects to `regind` over its Unix socket,
/// auto-starting the daemon on first use.
pub struct SocketTransport;

impl Transport for SocketTransport {
    async fn request(&self, req: &Request) -> Result<Response> {
        let (mut w, mut r) = connect_daemon().await?;
        send_req(&mut w, req).await?;
        let resp = read_resp(&mut r).await?;
        if let Response::Error { ref message } = resp {
            return Err(anyhow!("{message}"));
        }
        Ok(resp)
    }

    async fn request_stream<F: FnMut(&Response) + Send>(&self, req: &Request, mut on_event: F) -> Result<Vec<Response>> {
        let (mut w, mut r) = connect_daemon().await?;
        send_req(&mut w, req).await?;
        let mut out = Vec::new();
        loop {
            let resp = read_resp(&mut r).await?;
            on_event(&resp);
            // FEAT-080: an `ask`-level permission request pauses the daemon's
            // tool loop mid-stream. Answer it inline (a simple Y/n prompt —
            // acceptance criterion 6's "without breaking the streaming chat
            // display" just means not tearing down the in-progress stream,
            // which a plain synchronous stdin prompt satisfies without
            // pulling in a TUI dependency) over a *separate* connection, so
            // this stream's own reader keeps waiting for the daemon's next
            // event exactly as before. Not treated as terminal.
            if let Response::PermissionRequest { request_id, tool, detail } = &resp {
                let allow = prompt_permission(tool, detail);
                let _ = self.request(&Request::PermissionResponse { request_id: request_id.clone(), allow }).await;
                out.push(resp);
                continue;
            }
            let done = is_terminal(&resp);
            out.push(resp);
            if done {
                break;
            }
        }
        Ok(out)
    }
}

/// Blocking inline Y/n prompt for an `ask`-level permission request
/// (acceptance criterion 6). Defaults to deny on EOF/read error/anything
/// other than an explicit "y" — an ambiguous answer must not silently allow.
fn prompt_permission(tool: &str, detail: &str) -> bool {
    use std::io::Write;
    print!("\n[permission] allow '{tool}' — {detail}? [y/N] ");
    let _ = std::io::stdout().flush();
    let mut line = String::new();
    if std::io::stdin().read_line(&mut line).is_err() {
        return false;
    }
    matches!(line.trim().to_lowercase().as_str(), "y" | "yes")
}

// ---------------------------------------------------------------------------
// Socket helpers
// ---------------------------------------------------------------------------

pub async fn connect_daemon() -> Result<(
    tokio::io::WriteHalf<UnixStream>,
    BufReader<tokio::io::ReadHalf<UnixStream>>,
)> {
    ensure_daemon().await?;
    let sock = config::socket_path()?;
    let stream = UnixStream::connect(&sock)
        .await
        .with_context(|| format!("Cannot connect to regind at {}", sock.display()))?;
    let (r, w) = tokio::io::split(stream);
    Ok((w, BufReader::new(r)))
}

pub async fn send_req(w: &mut tokio::io::WriteHalf<UnixStream>, req: &Request) -> Result<()> {
    let mut line = serde_json::to_string(req)?;
    line.push('\n');
    w.write_all(line.as_bytes()).await?;
    Ok(())
}

pub async fn read_resp(r: &mut BufReader<tokio::io::ReadHalf<UnixStream>>) -> Result<Response> {
    let mut line = String::new();
    let n = r.read_line(&mut line).await?;
    if n == 0 {
        return Err(anyhow!("Connection closed"));
    }
    Ok(serde_json::from_str(&line)?)
}

// ---------------------------------------------------------------------------
// Daemon auto-start
// ---------------------------------------------------------------------------

pub async fn ensure_daemon() -> Result<()> {
    let sock = config::socket_path()?;
    if UnixStream::connect(&sock).await.is_ok() {
        return Ok(());
    }

    // BUG-001: prefer registering the persistent systemd *user* service so regind
    // survives logout/reboot, instead of a loose transient process. Honour an
    // opt-out (daemon.auto_register = false) and fall back to a transient spawn
    // when systemd-user is unavailable (e.g. minimal containers).
    let auto_register = read_local_setting("daemon.auto_register")
        .map(|v| v != "false")
        .unwrap_or(true);

    if auto_register && systemd_user_available() {
        eprintln!("Registering regind as a user service...");
        if install_regind_service().is_ok() {
            let _ = set_local_setting("daemon.enabled", "true");
            for _ in 0..50 {
                tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
                if UnixStream::connect(&sock).await.is_ok() {
                    return Ok(());
                }
            }
        }
        // fall through to a transient spawn if the service did not come up
    }

    eprintln!("Starting regind...");
    let regind = regind_bin();
    if regind.exists() {
        let _ = Command::new(&regind).spawn();
    } else {
        let _ = Command::new("regind").spawn();
    }
    for _ in 0..30 {
        tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
        if UnixStream::connect(&sock).await.is_ok() {
            return Ok(());
        }
    }
    Err(anyhow!("Failed to start regind. Run it manually or check logs."))
}

/// Path to the bundled `regind` binary next to this executable.
pub fn regind_bin() -> std::path::PathBuf {
    std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(|d| d.join("regind")))
        .unwrap_or_else(|| "regind".into())
}

/// Whether a systemd *user* manager is reachable (so we can install a service).
pub fn systemd_user_available() -> bool {
    Command::new("systemctl")
        .args(["--user", "show-environment"])
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

/// Read a setting directly from the SQLite store (used before the daemon is up).
pub fn read_local_setting(key: &str) -> Option<String> {
    let path = config::db_path().ok()?;
    let conn = db::init_db(&path).ok()?;
    db::setting_get(&conn, key).ok()
}

/// Write a setting directly to the SQLite store (used before the daemon is up).
pub fn set_local_setting(key: &str, value: &str) -> Result<()> {
    let path = config::db_path()?;
    let conn = db::init_db(&path)?;
    db::setting_set(&conn, key, value)?;
    Ok(())
}

/// Install + enable the regind systemd user service with lingering, so it
/// survives logout and starts at boot.
pub fn install_regind_service() -> Result<()> {
    let unit_dir = config::user_systemd_dir()?;
    let unit_path = config::regind_service_path()?;
    let regind = regind_bin();
    let regind_str = if regind.exists() {
        regind.to_string_lossy().to_string()
    } else {
        which_cmd("regind").unwrap_or_else(|| "/usr/bin/regind".into())
    };
    std::fs::create_dir_all(&unit_dir)?;
    std::fs::write(&unit_path, config::regind_service_unit(&regind_str))?;
    let _ = Command::new("loginctl").args(["enable-linger"]).status();
    let _ = Command::new("systemctl").args(["--user", "daemon-reload"]).status();
    let _ = Command::new("systemctl").args(["--user", "enable", "--now", "regind"]).status();
    Ok(())
}

pub fn which_cmd(name: &str) -> Option<String> {
    Command::new("which")
        .arg(name)
        .output()
        .ok()
        .and_then(|o| String::from_utf8(o.stdout).ok())
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
}

// ---------------------------------------------------------------------------
// Test double
// ---------------------------------------------------------------------------

#[cfg(test)]
pub mod fake {
    use super::*;
    use std::cell::RefCell;
    use std::collections::VecDeque;

    /// One queued reply: either a single `request()` answer or a whole
    /// `request_stream()` sequence.
    pub enum FakeReply {
        One(Response),
        Stream(Vec<Response>),
    }

    /// A canned-response [`Transport`] for unit tests. Replies are consumed
    /// in FIFO order; every request sent is recorded for assertions.
    #[derive(Default)]
    pub struct FakeTransport {
        replies: RefCell<VecDeque<FakeReply>>,
        pub sent: RefCell<Vec<Request>>,
    }

    impl FakeTransport {
        pub fn new() -> Self {
            Self::default()
        }

        /// Queue a plain `request()` reply.
        pub fn push(&self, resp: Response) -> &Self {
            self.replies.borrow_mut().push_back(FakeReply::One(resp));
            self
        }

        /// Queue a `request_stream()` reply (a full event sequence).
        pub fn push_stream(&self, resps: Vec<Response>) -> &Self {
            self.replies.borrow_mut().push_back(FakeReply::Stream(resps));
            self
        }

        pub fn sent(&self) -> Vec<Request> {
            self.sent.borrow().clone()
        }
    }

    impl Transport for FakeTransport {
        async fn request(&self, req: &Request) -> Result<Response> {
            self.sent.borrow_mut().push(req.clone());
            match self.replies.borrow_mut().pop_front() {
                Some(FakeReply::One(Response::Error { message })) => Err(anyhow!("{message}")),
                Some(FakeReply::One(r)) => Ok(r),
                Some(FakeReply::Stream(_)) => {
                    panic!("FakeTransport: queued a stream reply but request() (non-streaming) was called")
                }
                None => panic!("FakeTransport: no queued response for {req:?}"),
            }
        }

        async fn request_stream<F: FnMut(&Response) + Send>(&self, req: &Request, mut on_event: F) -> Result<Vec<Response>> {
            self.sent.borrow_mut().push(req.clone());
            let events = match self.replies.borrow_mut().pop_front() {
                Some(FakeReply::Stream(v)) => v,
                Some(FakeReply::One(r)) => vec![r],
                None => panic!("FakeTransport: no queued response for {req:?}"),
            };
            for e in &events {
                on_event(e);
            }
            Ok(events)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::fake::FakeTransport;
    use super::*;

    #[tokio::test]
    async fn fake_transport_replays_queued_response() {
        let t = FakeTransport::new();
        t.push(Response::Pong);
        let resp = t.request(&Request::Ping).await.unwrap();
        assert!(matches!(resp, Response::Pong));
        assert_eq!(t.sent().len(), 1);
    }

    #[tokio::test]
    async fn fake_transport_turns_error_response_into_err() {
        let t = FakeTransport::new();
        t.push(Response::Error { message: "nope".into() });
        let err = t.request(&Request::Ping).await.unwrap_err();
        assert_eq!(err.to_string(), "nope");
    }

    #[tokio::test]
    async fn fake_transport_stream_replays_full_sequence() {
        let t = FakeTransport::new();
        t.push_stream(vec![
            Response::StreamChunk { token: "hi".into() },
            Response::StreamDone { conversation_id: "c1".into() },
        ]);
        let mut seen = 0;
        let events = t.request_stream(&Request::ChatNew, |_| seen += 1).await.unwrap();
        assert_eq!(events.len(), 2);
        assert_eq!(seen, 2);
        assert!(is_terminal(events.last().unwrap()));
    }

    #[tokio::test]
    #[should_panic(expected = "no queued response")]
    async fn fake_transport_panics_on_exhausted_queue() {
        let t = FakeTransport::new();
        let _ = t.request(&Request::Ping).await;
    }

    #[tokio::test]
    #[should_panic(expected = "no queued response")]
    async fn fake_transport_stream_panics_on_exhausted_queue() {
        let t = FakeTransport::new();
        let _ = t.request_stream(&Request::ChatNew, |_| {}).await;
    }
}
