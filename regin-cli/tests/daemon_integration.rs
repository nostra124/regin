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
//! instead derived as a sibling of `regin`'s in the same target directory,
//! which cargo always populates when the workspace is built together —
//! i.e. under `cargo test --workspace` (this project's own convention).
//! `cargo test -p regin-cli` in isolation, without a prior workspace
//! build, will not find it; [`regind_bin`] panics with a message pointing
//! at the fix rather than silently skipping.

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
    let p = regin_bin().with_file_name("regind");
    assert!(
        p.exists(),
        "regind binary not found at {} — run `cargo test --workspace` (or `cargo build \
         --workspace` first) so both binaries are built together; `cargo test -p regin-cli` \
         alone does not build the sibling `regind` binary.",
        p.display()
    );
    p
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
