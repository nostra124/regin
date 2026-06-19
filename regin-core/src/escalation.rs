//! FEAT-015 (DISC-003 layer A): the escalation bridge. When regin's problem
//! management decides "this needs a code/config change", it escalates the problem
//! to the dvalin development plane as a **structured bus message** requesting a
//! BUG or FEAT. dvalin (MILESTONE-1.3.0) turns the message into a ticket and
//! reports the ticket id back; regin correlates the reply by the escalation ref.
//!
//! This module is the payload builder — pure and unit-tested. The send (over the
//! bus client) and the recording of the reply are wired by the caller.

use anyhow::{bail, Result};
use serde::{Deserialize, Serialize};

/// What kind of dvalin ticket an escalation asks for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EscalationKind {
    Bug,
    Feat,
}

impl EscalationKind {
    pub fn parse(s: &str) -> Result<EscalationKind> {
        match s.trim().to_lowercase().as_str() {
            "bug" => Ok(EscalationKind::Bug),
            "feat" | "feature" => Ok(EscalationKind::Feat),
            other => bail!("unknown escalation kind {other:?} (use bug|feat)"),
        }
    }

    pub fn as_str(&self) -> &'static str {
        match self {
            EscalationKind::Bug => "bug",
            EscalationKind::Feat => "feat",
        }
    }
}

/// The structured escalation payload (the body of a `structured` bus message).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Escalation {
    /// Marks this structured message as an escalation for dvalin's router.
    #[serde(default = "escalation_tag")]
    pub kind: String,
    /// The dvalin ticket type requested.
    pub ticket: String,
    /// The originating regin problem id (for the verify→close loop).
    pub problem_id: String,
    pub title: String,
    pub description: String,
    /// Where dvalin should report the created ticket id back to.
    pub reply_to: String,
    /// Correlation ref echoed on the reply.
    pub ref_id: String,
}

fn escalation_tag() -> String {
    "escalation".to_string()
}

/// The correlation ref regin assigns to an escalation of `problem_id`.
pub fn correlation_ref(problem_id: &str) -> String {
    format!("ESC-{problem_id}")
}

/// Build an escalation payload for a problem.
pub fn build(
    problem_id: &str,
    title: &str,
    description: &str,
    kind: EscalationKind,
    reply_to: &str,
) -> Escalation {
    Escalation {
        kind: escalation_tag(),
        ticket: kind.as_str().to_string(),
        problem_id: problem_id.to_string(),
        title: title.to_string(),
        description: description.to_string(),
        reply_to: reply_to.to_string(),
        ref_id: correlation_ref(problem_id),
    }
}

/// Serialize an escalation as a structured bus-message body.
pub fn body(e: &Escalation) -> Result<String> {
    Ok(serde_json::to_string(e)?)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn kind_parses() {
        assert_eq!(EscalationKind::parse("bug").unwrap(), EscalationKind::Bug);
        assert_eq!(EscalationKind::parse("Feature").unwrap(), EscalationKind::Feat);
        assert!(EscalationKind::parse("epic").is_err());
    }

    #[test]
    fn builds_a_tagged_payload_with_correlation_ref() {
        let e = build("P-3", "auth flakes", "root cause: token clock skew", EscalationKind::Bug, "cio@hq");
        assert_eq!(e.kind, "escalation");
        assert_eq!(e.ticket, "bug");
        assert_eq!(e.problem_id, "P-3");
        assert_eq!(e.reply_to, "cio@hq");
        assert_eq!(e.ref_id, "ESC-P-3");
        assert_eq!(correlation_ref("P-3"), "ESC-P-3");
    }

    #[test]
    fn body_round_trips() {
        let e = build("P-1", "t", "d", EscalationKind::Feat, "cto@hq");
        let back: Escalation = serde_json::from_str(&body(&e).unwrap()).unwrap();
        assert_eq!(back, e);
        assert_eq!(back.ticket, "feat");
    }
}
