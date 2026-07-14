//! Integration tests over the real `regind` + `regin` binaries (FEAT-074).
//!
//! Spawns the real `regind` binary on an isolated socket + data dir (via
//! `XDG_RUNTIME_DIR` / `XDG_DATA_HOME` / `XDG_CONFIG_HOME` env overrides —
//! the same seams `config::socket_path`/`config::data_dir` already honour
//! in production, no test-only code path), waits for it to bind its
//! socket, drives real `regin` CLI commands against it over the real
//! Unix-socket transport (covers CLI `main()` dispatch + `rpc()`), sends a
//! malformed request line directly over the socket (covers
//! `handle_connection`'s bad-request branch), then sends SIGTERM and
//! asserts a clean shutdown (socket file removed, exit code 0).
//!
//! Hermetic: every test gets its own temp XDG root (unique per test via a
//! process-wide counter), removed on drop; nothing touches the real
//! `~/.local/share/regin` or a real systemd user session.
//!
//! `CARGO_BIN_EXE_regin` is set because this crate's own `[[bin]]` produces
//! it. Cargo does not expose a workspace *sibling* crate's
//! `CARGO_BIN_EXE_*` — there is no stable artifact-dependency support on
//! this toolchain (`artifact = "bin"` requires `-Z bindeps`, nightly-only —
//! confirmed by trying it before writing this test). `regind`'s path is
//! instead derived as a sibling of `regin`'s in the same target directory.
//!
//! That sibling is **not** reliably present just because the workspace was
//! built or tested — verified the hard way: even a clean `cargo test
//! --workspace` does not build a plain `[[bin]]` target that (a) isn't a
//! declared dependency of anything and (b) belongs to a package whose only
//! *other* build need is its own `#[cfg(test)]` harness (which recompiles
//! `main.rs` under `--cfg test`, a separate artifact from the production
//! binary). `cargo llvm-cov` narrows the build footprint further still. So
//! [`regind_bin`] self-heals: if the sibling is missing, it shells out to
//! `cargo build -p regind --bin regind` targeting the exact directory
//! `regin`'s own binary landed in (derived from `CARGO_BIN_EXE_regin`, not
//! assumed) — this works unmodified under plain `cargo test` and under
//! `cargo llvm-cov` alike, since it inherits whatever coverage
//! instrumentation env vars the outer test run was invoked with.

use std::io::{BufRead, BufReader, Write};
use std::os::unix::net::UnixStream;
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

fn regin_bin() -> PathBuf {
    PathBuf::from(env!("CARGO_BIN_EXE_regin"))
}

fn regind_bin() -> PathBuf {
    let regin = regin_bin();
    let sibling = regin.with_file_name("regind");
    if sibling.exists() {
        return sibling;
    }
    let target_dir = regin
        .parent() // .../<target-dir>/debug
        .and_then(|p| p.parent()) // .../<target-dir>
        .unwrap_or_else(|| panic!("{} has no target directory ancestor", regin.display()));
    let cargo = std::env::var("CARGO").unwrap_or_else(|_| "cargo".into());
    let status = Command::new(&cargo)
        .args(["build", "-p", "regind", "--bin", "regind", "--target-dir"])
        .arg(target_dir)
        .status()
        .unwrap_or_else(|e| panic!("failed to run `{cargo} build -p regind`: {e}"));
    assert!(status.success(), "`{cargo} build -p regind` failed");
    assert!(sibling.exists(), "regind binary still missing at {} after building it", sibling.display());
    sibling
}

static SANDBOX_COUNTER: AtomicU64 = AtomicU64::new(0);

/// An isolated XDG environment (temp runtime/data/config dirs) unique per
/// test, removed on drop even on panic.
struct Sandbox {
    root: PathBuf,
}

impl Sandbox {
    fn new() -> Self {
        let n = SANDBOX_COUNTER.fetch_add(1, Ordering::SeqCst);
        let root = std::env::temp_dir().join(format!("regin-feat074-{}-{n}", std::process::id()));
        for sub in ["runtime", "data", "config"] {
            std::fs::create_dir_all(root.join(sub)).unwrap();
        }
        Self { root }
    }

    /// Build a `Command` for `bin`, pointed at this sandbox's isolated XDG dirs.
    fn command(&self, bin: &PathBuf) -> Command {
        let mut cmd = Command::new(bin);
        cmd.env("XDG_RUNTIME_DIR", self.root.join("runtime"))
            .env("XDG_DATA_HOME", self.root.join("data"))
            .env("XDG_CONFIG_HOME", self.root.join("config"))
            .env_remove("RUST_LOG");
        cmd
    }

    fn socket_path(&self) -> PathBuf {
        self.root.join("runtime").join("regin").join("regind.sock")
    }
}

impl Drop for Sandbox {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

/// Spawn the real `regind` binary in `sandbox`, output discarded.
fn spawn_regind(sandbox: &Sandbox) -> Child {
    sandbox
        .command(&regind_bin())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .expect("failed to spawn regind")
}

/// Poll the raw socket (not `regin ping`) until `regind` has bound it.
/// Deliberately does NOT drive readiness through the `regin` CLI: a `regin`
/// invocation racing the daemon's startup would hit `ensure_daemon`'s
/// fallback path (systemd-user registration, or spawning a second
/// competing `regind`) instead of its fast "already listening" check —
/// exactly the non-hermetic behaviour this test must avoid. Once this
/// returns, every subsequent `regin` command in the test hits that fast
/// path immediately.
fn wait_for_socket(sandbox: &Sandbox) {
    let sock = sandbox.socket_path();
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        if UnixStream::connect(&sock).is_ok() {
            return;
        }
        assert!(Instant::now() < deadline, "regind did not bind its socket within 10s at {}", sock.display());
        std::thread::sleep(Duration::from_millis(50));
    }
}

fn terminate_and_wait_for_exit(mut child: Child, sandbox: &Sandbox) {
    let pid = child.id();
    let status = Command::new("kill").arg("-TERM").arg(pid.to_string()).status().expect("failed to run kill(1)");
    assert!(status.success(), "failed to send SIGTERM to regind (pid {pid})");

    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        if let Some(status) = child.try_wait().unwrap() {
            assert!(status.success(), "regind did not exit cleanly after SIGTERM: {status:?}");
            break;
        }
        if Instant::now() > deadline {
            let _ = child.kill();
            panic!("regind did not shut down within 5s of SIGTERM");
        }
        std::thread::sleep(Duration::from_millis(50));
    }

    assert!(!sandbox.socket_path().exists(), "regind should remove its socket file on clean shutdown");
}

#[test]
fn daemon_lifecycle_cli_commands_bad_request_and_sigterm() {
    let sandbox = Sandbox::new();
    let daemon = spawn_regind(&sandbox);
    wait_for_socket(&sandbox);

    // --- representative CLI command set over the real socket transport ---
    // (main() argument parsing + dispatch, rpc()'s real send/receive framing)
    assert!(sandbox.command(&regin_bin()).arg("ping").status().unwrap().success(), "regin ping");

    assert!(
        sandbox.command(&regin_bin()).args(["config", "set", "kpi.reliability_floor", "0.9"]).status().unwrap().success(),
        "regin config set"
    );
    let got = sandbox.command(&regin_bin()).args(["config", "get", "kpi.reliability_floor"]).output().unwrap();
    assert!(got.status.success(), "regin config get");
    assert!(String::from_utf8_lossy(&got.stdout).contains("0.9"));

    assert!(sandbox.command(&regin_bin()).args(["config", "list"]).status().unwrap().success(), "regin config list");

    assert!(
        sandbox.command(&regin_bin()).args(["memory", "save", "fact", "the sky is blue"]).status().unwrap().success(),
        "regin memory save"
    );
    let listed = sandbox.command(&regin_bin()).args(["memory", "list"]).output().unwrap();
    assert!(listed.status.success(), "regin memory list");
    assert!(String::from_utf8_lossy(&listed.stdout).contains("the sky is blue"));

    assert!(sandbox.command(&regin_bin()).arg("mode").status().unwrap().success(), "regin mode");

    // an unknown command is a real CLI-parse failure — non-zero exit, no daemon round trip
    assert!(!sandbox.command(&regin_bin()).arg("definitely-not-a-real-subcommand").status().unwrap().success());

    // --- bad request line, sent directly over the real socket ---
    // (handle_connection's serde_json::from_str error branch)
    let mut sock = UnixStream::connect(sandbox.socket_path()).unwrap();
    sock.write_all(b"not json at all\n").unwrap();
    let mut reader = BufReader::new(sock.try_clone().unwrap());
    let mut line = String::new();
    reader.read_line(&mut line).unwrap();
    assert!(line.contains("\"type\":\"error\""), "got {line:?}");
    assert!(line.contains("Bad request"), "got {line:?}");

    // the connection survives the bad line — handle_connection `continue`s
    // its read loop rather than closing on a parse error.
    sock.write_all(b"{\"type\":\"ping\"}\n").unwrap();
    line.clear();
    reader.read_line(&mut line).unwrap();
    assert!(line.contains("\"type\":\"pong\""), "got {line:?}");
    drop(reader);
    drop(sock);

    // --- SIGTERM shutdown ---
    terminate_and_wait_for_exit(daemon, &sandbox);
}

#[test]
fn two_sandboxes_run_independent_daemons_without_colliding() {
    // Confirms the isolation itself: two regind instances, two sockets,
    // no shared state — a regression guard on the Sandbox helper.
    let a = Sandbox::new();
    let b = Sandbox::new();
    assert_ne!(a.socket_path(), b.socket_path());

    let daemon_a = spawn_regind(&a);
    let daemon_b = spawn_regind(&b);
    wait_for_socket(&a);
    wait_for_socket(&b);

    assert!(sandbox_ping(&a));
    assert!(sandbox_ping(&b));

    terminate_and_wait_for_exit(daemon_a, &a);
    terminate_and_wait_for_exit(daemon_b, &b);
}

fn sandbox_ping(sandbox: &Sandbox) -> bool {
    sandbox.command(&regin_bin()).arg("ping").status().unwrap().success()
}

/// Same self-heal strategy as [`regind_bin`], but built with `--features
/// webui` into a distinct target-dir (`<target-dir>-webui`) so it doesn't
/// fight the plain build for the same output path.
fn regind_bin_with_webui() -> PathBuf {
    let regin = regin_bin();
    let target_dir = regin
        .parent()
        .and_then(|p| p.parent())
        .unwrap_or_else(|| panic!("{} has no target directory ancestor", regin.display()))
        .with_file_name(format!(
            "{}-webui",
            regin.parent().and_then(|p| p.parent()).and_then(|p| p.file_name()).and_then(|n| n.to_str()).unwrap_or("target")
        ));
    let sibling = target_dir.join("debug").join("regind");
    if !sibling.exists() {
        let cargo = std::env::var("CARGO").unwrap_or_else(|_| "cargo".into());
        let status = Command::new(&cargo)
            .args(["build", "-p", "regind", "--bin", "regind", "--features", "webui", "--target-dir"])
            .arg(&target_dir)
            .status()
            .unwrap_or_else(|e| panic!("failed to run `{cargo} build -p regind --features webui`: {e}"));
        assert!(status.success(), "`{cargo} build -p regind --features webui` failed");
    }
    assert!(sibling.exists(), "webui-enabled regind binary still missing at {} after building it", sibling.display());
    sibling
}

/// Acceptance criterion 14: `regin webui enable --port <N>` makes the web
/// UI reachable over real HTTP in the *same* running daemon (no restart
/// needed — see the `Request::WebuiEnable` dispatch arm's comment in
/// `regind/src/main.rs`), and `regin webui disable` flips `webui.enabled`
/// back off (verified via `regin webui status`; actually tearing down an
/// already-bound listener is out of v1's scope — same "next restart"
/// convention documented on the `Request::WebuiDisable` response).
#[test]
fn webui_enable_serves_health_over_real_http() {
    let sandbox = Sandbox::new();
    let daemon = spawn_regind_bin(&sandbox, &regind_bin_with_webui());
    wait_for_socket(&sandbox);

    let port = free_tcp_port();
    let enabled = sandbox.command(&regin_bin()).args(["webui", "enable", "--port", &port.to_string()]).output().unwrap();
    assert!(enabled.status.success(), "regin webui enable: {}", String::from_utf8_lossy(&enabled.stderr));

    let status_line = wait_for_http_ok(port, "/regin/api/health", Duration::from_secs(10));
    assert!(status_line.contains(" 200 "), "got {status_line:?}");

    let disabled = sandbox.command(&regin_bin()).args(["webui", "disable"]).status().unwrap();
    assert!(disabled.success(), "regin webui disable");
    let status = sandbox.command(&regin_bin()).args(["webui", "status"]).output().unwrap();
    assert!(status.status.success());
    assert!(String::from_utf8_lossy(&status.stdout).contains("disabled") || String::from_utf8_lossy(&status.stdout).contains("enabled: false"));

    terminate_and_wait_for_exit(daemon, &sandbox);
}

/// Same as [`spawn_regind`] but for an arbitrary binary path (used to run
/// the separately-built `--features webui` binary).
fn spawn_regind_bin(sandbox: &Sandbox, bin: &PathBuf) -> Child {
    sandbox.command(bin).stdout(Stdio::null()).stderr(Stdio::null()).spawn().expect("failed to spawn regind")
}

fn free_tcp_port() -> u16 {
    std::net::TcpListener::bind("127.0.0.1:0").unwrap().local_addr().unwrap().port()
}

/// Polls a bare HTTP/1.0 GET over a raw `TcpStream` until it gets a
/// response (no `reqwest` dev-dependency needed for one status-line
/// check) — connection refused just means the listener hasn't bound yet,
/// which is expected while `Request::WebuiEnable`'s spawned `maybe_start`
/// task is still starting up.
fn wait_for_http_ok(port: u16, path: &str, timeout: Duration) -> String {
    let deadline = Instant::now() + timeout;
    loop {
        if let Ok(mut stream) = std::net::TcpStream::connect(("127.0.0.1", port)) {
            let _ = stream.write_all(format!("GET {path} HTTP/1.0\r\nHost: 127.0.0.1\r\nConnection: close\r\n\r\n").as_bytes());
            let mut reader = BufReader::new(stream);
            let mut status_line = String::new();
            if reader.read_line(&mut status_line).is_ok() && !status_line.is_empty() {
                return status_line;
            }
        }
        assert!(Instant::now() < deadline, "webui never became reachable on 127.0.0.1:{port}{path} within {timeout:?}");
        std::thread::sleep(Duration::from_millis(50));
    }
}
