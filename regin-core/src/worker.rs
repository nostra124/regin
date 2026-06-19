//! FEAT-012 (DISC-004): the local CLI-worker supervisor. In a cave the foreman
//! (regin) drives the pull-only coding CLIs — `claude` and `opencode` — by
//! spawning them with an injected prompt, capturing their output, and
//! classifying the outcome. This relocates dvalin's stdin/stdout supervisor loop
//! into the cave for workers that have no mailbox of their own.
//!
//! Unit-tested: the worker→argv mapping and the outcome classification.
//! NEEDS-LIVE-VERIFICATION: the actual spawn (the `claude`/`opencode` binaries
//! are not present in CI) — exercised only inside a provisioned cave.

use anyhow::{bail, Result};

/// The CLI workers a foreman can drive.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WorkerKind {
    Claude,
    Opencode,
}

impl WorkerKind {
    pub fn parse(s: &str) -> Result<WorkerKind> {
        match s.trim().to_lowercase().as_str() {
            "claude" => Ok(WorkerKind::Claude),
            "opencode" => Ok(WorkerKind::Opencode),
            other => bail!("unknown worker {other:?} (use claude|opencode)"),
        }
    }

    pub fn binary(&self) -> &'static str {
        match self {
            WorkerKind::Claude => "claude",
            WorkerKind::Opencode => "opencode",
        }
    }
}

/// The non-interactive argv for a worker run with `prompt`. Both CLIs support a
/// headless "run this prompt and exit" mode.
pub fn argv(kind: WorkerKind, prompt: &str) -> Vec<String> {
    match kind {
        // `claude -p <prompt>` runs headless and prints the result.
        WorkerKind::Claude => vec!["claude".into(), "-p".into(), prompt.to_string()],
        // `opencode run <prompt>` runs a single non-interactive turn.
        WorkerKind::Opencode => vec!["opencode".into(), "run".into(), prompt.to_string()],
    }
}

/// How a worker run turned out.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Outcome {
    /// Exited 0 with output.
    Done,
    /// Exited non-zero.
    Failed,
    /// Exited 0 but produced no usable output (a no-op — treated as a soft fail).
    Empty,
}

/// Classify an outcome from the exit success and captured output.
pub fn classify(success: bool, output: &str) -> Outcome {
    if !success {
        Outcome::Failed
    } else if output.trim().is_empty() {
        Outcome::Empty
    } else {
        Outcome::Done
    }
}

/// The result of supervising a worker run.
#[derive(Debug, Clone)]
pub struct WorkerRun {
    pub kind: WorkerKind,
    pub outcome: Outcome,
    pub output: String,
    pub exit_code: Option<i32>,
}

/// Spawn a worker with `prompt` in `cwd`, capture output, classify the outcome.
///
/// NEEDS-LIVE-VERIFICATION: requires the `claude`/`opencode` CLI in the cave. The
/// argv ([`argv`]) and classification ([`classify`]) are unit-tested; the spawn
/// is not.
pub fn run(kind: WorkerKind, prompt: &str, cwd: Option<&str>) -> WorkerRun {
    let (success, output, exit_code) = exec(&argv(kind, prompt), cwd);
    WorkerRun { kind, outcome: classify(success, &output), output, exit_code }
}

/// Spawn an argv, capturing combined stdout+stderr. Returns (success, output,
/// exit_code). A spawn failure (missing binary) is `(false, "spawn failed: …",
/// None)`. Factored out so the spawn path is testable with a known-missing binary.
fn exec(argv: &[String], cwd: Option<&str>) -> (bool, String, Option<i32>) {
    let mut cmd = std::process::Command::new(&argv[0]);
    cmd.args(&argv[1..]);
    if let Some(dir) = cwd {
        cmd.current_dir(dir);
    }
    match cmd.output() {
        Ok(o) => {
            let text = String::from_utf8_lossy(&o.stdout).into_owned()
                + &String::from_utf8_lossy(&o.stderr);
            (o.status.success(), text, o.status.code())
        }
        Err(e) => (false, format!("spawn failed: {e}"), None),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn worker_kind_parses_and_maps_to_binary() {
        assert_eq!(WorkerKind::parse("claude").unwrap().binary(), "claude");
        assert_eq!(WorkerKind::parse("OpenCode").unwrap().binary(), "opencode");
        assert!(WorkerKind::parse("cursor").is_err());
    }

    #[test]
    fn argv_is_headless_per_worker() {
        assert_eq!(argv(WorkerKind::Claude, "fix the bug"), vec!["claude", "-p", "fix the bug"]);
        assert_eq!(argv(WorkerKind::Opencode, "add tests"), vec!["opencode", "run", "add tests"]);
    }

    #[test]
    fn classify_covers_done_failed_empty() {
        assert_eq!(classify(true, "patch applied"), Outcome::Done);
        assert_eq!(classify(false, "boom"), Outcome::Failed);
        assert_eq!(classify(true, "   \n"), Outcome::Empty);
    }

    #[test]
    fn spawn_of_a_missing_binary_is_graceful_failure() {
        // a guaranteed-missing binary → spawn failure surfaced as Failed, no panic
        let (success, output, code) = exec(&["regin-no-such-binary-xyz".into()], None);
        assert!(!success);
        assert!(output.contains("spawn failed"));
        assert!(code.is_none());
        assert_eq!(classify(success, &output), Outcome::Failed);
    }
}
