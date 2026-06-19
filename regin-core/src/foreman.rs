//! FEAT-013 (DISC-004): foreman mode. The foreman receives **cave-task**
//! messages on the bus (FEAT-010), drives a local CLI worker (FEAT-012), and
//! reports a structured **handover** back up the bus. A failed/empty worker run
//! also drafts an ITIL incident — the discipline boundary that turns a broken
//! in-cave step into tracked operations work.
//!
//! This module is the pure decision core: parse the task, and given a worker
//! result, plan the handover + any incident. The actual worker spawn, bus send,
//! and incident persistence are wired by the caller (CLI/daemon), so the policy
//! here is fully unit-testable.

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

use crate::bus::{BusMessage, KIND_STRUCTURED};
use crate::worker::{Outcome, WorkerKind, WorkerRun};

/// A structured cave-task: develop/run something via a local worker, then report
/// back to `reply_to`. Carried as the JSON body of a `structured` bus message.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CaveTask {
    /// The instruction handed to the worker.
    pub task: String,
    /// Which CLI worker to drive (claude|opencode). Defaults to claude.
    #[serde(default = "default_worker")]
    pub worker: String,
    /// Where to run (the repo workspace inside the cave).
    #[serde(default)]
    pub cwd: Option<String>,
    /// Who to send the handover to. Defaults to the message sender.
    #[serde(default)]
    pub reply_to: Option<String>,
    /// Correlation ref echoed back on the handover.
    #[serde(default)]
    pub ref_id: Option<String>,
}

fn default_worker() -> String {
    "claude".to_string()
}

impl CaveTask {
    /// Parse a cave-task from a structured bus message. Non-structured or
    /// non-task messages return `None` (not every inbox message is a cave-task).
    pub fn from_message(m: &BusMessage) -> Option<CaveTask> {
        if m.kind != KIND_STRUCTURED {
            return None;
        }
        serde_json::from_str::<CaveTask>(&m.body).ok().filter(|t| !t.task.trim().is_empty())
    }

    pub fn worker_kind(&self) -> Result<WorkerKind> {
        WorkerKind::parse(&self.worker).with_context(|| format!("cave-task worker {:?}", self.worker))
    }
}

/// The structured handover reported back after a worker run.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Handover {
    pub ref_id: Option<String>,
    /// `done` | `failed` | `empty`.
    pub outcome: String,
    pub summary: String,
    pub exit_code: Option<i32>,
}

/// A drafted incident to open when in-cave work breaks (the discipline boundary).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IncidentDraft {
    pub title: String,
    pub description: String,
    pub severity: String,
}

fn outcome_str(o: Outcome) -> &'static str {
    match o {
        Outcome::Done => "done",
        Outcome::Failed => "failed",
        Outcome::Empty => "empty",
    }
}

/// Given a task and its worker run, plan the handover message body and (when the
/// run did not cleanly succeed) an incident to open.
pub fn plan_handover(task: &CaveTask, run: &WorkerRun) -> (Handover, Option<IncidentDraft>) {
    let outcome = outcome_str(run.outcome);
    // Keep the summary bounded — the handover is a status, not a transcript.
    let summary: String = run.output.trim().chars().take(500).collect();
    let handover = Handover {
        ref_id: task.ref_id.clone(),
        outcome: outcome.to_string(),
        summary: summary.clone(),
        exit_code: run.exit_code,
    };
    let incident = match run.outcome {
        Outcome::Done => None,
        Outcome::Failed | Outcome::Empty => Some(IncidentDraft {
            title: format!("cave-task {} worker {}", task.ref_id.as_deref().unwrap_or("(no-ref)"), outcome),
            description: format!(
                "worker={} outcome={} exit={:?}\ntask: {}\noutput:\n{}",
                task.worker, outcome, run.exit_code, task.task, summary
            ),
            severity: "medium".to_string(),
        }),
    };
    (handover, incident)
}

/// Serialize a handover as a structured bus-message body.
pub fn handover_body(h: &Handover) -> Result<String> {
    Ok(serde_json::to_string(h)?)
}

/// Who the handover goes to: the task's explicit `reply_to`, else the original
/// message sender.
pub fn handover_recipient(task: &CaveTask, original: &BusMessage) -> String {
    task.reply_to.clone().unwrap_or_else(|| original.sender.clone())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn structured(body: &str) -> BusMessage {
        BusMessage {
            id: 1,
            sender: "ceo@hq".into(),
            recipient: "regin@cave-a".into(),
            kind: KIND_STRUCTURED.into(),
            body: body.into(),
            ref_id: None,
            channel: None,
        }
    }

    #[test]
    fn parses_a_structured_cave_task() {
        let m = structured(r#"{"task":"fix the bug","worker":"opencode","ref_id":"T-9"}"#);
        let t = CaveTask::from_message(&m).unwrap();
        assert_eq!(t.task, "fix the bug");
        assert_eq!(t.worker, "opencode");
        assert_eq!(t.ref_id.as_deref(), Some("T-9"));
        assert_eq!(t.worker_kind().unwrap(), WorkerKind::Opencode);
        // default worker when omitted
        let t2 = CaveTask::from_message(&structured(r#"{"task":"x"}"#)).unwrap();
        assert_eq!(t2.worker, "claude");
    }

    #[test]
    fn non_task_messages_are_ignored() {
        // unstructured message
        let mut m = structured(r#"{"task":"x"}"#);
        m.kind = "unstructured".into();
        assert!(CaveTask::from_message(&m).is_none());
        // structured but not a task
        assert!(CaveTask::from_message(&structured(r#"{"hello":"world"}"#)).is_none());
        // empty task
        assert!(CaveTask::from_message(&structured(r#"{"task":"   "}"#)).is_none());
    }

    fn run(outcome: Outcome, output: &str, code: Option<i32>) -> WorkerRun {
        WorkerRun { kind: WorkerKind::Claude, outcome, output: output.into(), exit_code: code }
    }

    #[test]
    fn done_run_handover_has_no_incident() {
        let t = CaveTask::from_message(&structured(r#"{"task":"x","ref_id":"T-1"}"#)).unwrap();
        let (h, inc) = plan_handover(&t, &run(Outcome::Done, "patch applied", Some(0)));
        assert_eq!(h.outcome, "done");
        assert_eq!(h.ref_id.as_deref(), Some("T-1"));
        assert_eq!(h.summary, "patch applied");
        assert!(inc.is_none());
    }

    #[test]
    fn failed_run_drafts_an_incident() {
        let t = CaveTask::from_message(&structured(r#"{"task":"build","ref_id":"T-2"}"#)).unwrap();
        let (h, inc) = plan_handover(&t, &run(Outcome::Failed, "error: boom", Some(1)));
        assert_eq!(h.outcome, "failed");
        let inc = inc.expect("failure must draft an incident");
        assert!(inc.title.contains("T-2") && inc.title.contains("failed"));
        assert!(inc.description.contains("boom"));
        assert_eq!(inc.severity, "medium");
        // empty also drafts an incident
        let (_, inc2) = plan_handover(&t, &run(Outcome::Empty, "", Some(0)));
        assert!(inc2.is_some());
    }

    #[test]
    fn handover_recipient_prefers_reply_to_then_sender() {
        let m = structured(r#"{"task":"x","reply_to":"cto@hq"}"#);
        let t = CaveTask::from_message(&m).unwrap();
        assert_eq!(handover_recipient(&t, &m), "cto@hq");
        let m2 = structured(r#"{"task":"x"}"#);
        let t2 = CaveTask::from_message(&m2).unwrap();
        assert_eq!(handover_recipient(&t2, &m2), "ceo@hq", "falls back to sender");
    }

    #[test]
    fn handover_body_round_trips() {
        let h = Handover { ref_id: Some("T-1".into()), outcome: "done".into(), summary: "ok".into(), exit_code: Some(0) };
        let body = handover_body(&h).unwrap();
        let back: Handover = serde_json::from_str(&body).unwrap();
        assert_eq!(back, h);
    }
}
