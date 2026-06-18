//! FEAT-016 (DISC-004): meeting-chair behaviour. A regin role can chair a standing
//! meeting: it runs the standard agenda, collects participants' reports off the
//! bus, applies its discipline (pulling its own ITIL counts), and produces
//! **minutes** (decisions + action-items) posted back to dvalin as a structured
//! message — which dvalind records (dvalin FEAT-133).
//!
//! This module is the pure compile core: collect reports, compile minutes, build
//! the structured minutes message. The bus send + LLM-driven judgement are wired
//! by the caller.

use serde::{Deserialize, Serialize};

use crate::bus::{BusMessage, KIND_STRUCTURED};

/// A participant's report collected off the bus for the meeting.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Report {
    pub from: String,
    pub body: String,
}

/// An action item assigned out of the meeting.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ActionItem {
    pub assignee: String,
    pub description: String,
    #[serde(default)]
    pub needs_owner_approval: bool,
}

/// Compiled minutes for one convening.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Minutes {
    pub decisions: Vec<String>,
    pub action_items: Vec<ActionItem>,
}

/// Collect participant reports from the chair's inbox: messages whose body is a
/// report (we accept both free-text and a structured `{"kind":"report",...}`).
pub fn collect_reports(inbox: &[BusMessage]) -> Vec<Report> {
    inbox
        .iter()
        .filter_map(|m| {
            let body = match serde_json::from_str::<serde_json::Value>(&m.body) {
                Ok(v) if v.get("kind").and_then(|k| k.as_str()) == Some("report") => {
                    v.get("body").and_then(|b| b.as_str()).unwrap_or("").to_string()
                }
                _ => m.body.clone(),
            };
            if body.trim().is_empty() {
                None
            } else {
                Some(Report { from: m.sender.clone(), body })
            }
        })
        .collect()
}

/// Compile minutes from the agenda, collected reports, and the chair's own open
/// ITIL count (the discipline the governance layer is fed by). Deterministic:
/// each agenda item is minuted as reviewed; an `ACTION <assignee>: <desc>` line
/// in any report becomes an action-item (`!` suffix on the assignee → owner-gated).
pub fn compile(agenda: &[String], reports: &[Report], open_itil: usize) -> Minutes {
    let mut decisions: Vec<String> = agenda.iter().map(|item| format!("{item}: reviewed")).collect();
    decisions.push(format!("open ITIL items carried: {open_itil}"));

    let mut action_items = Vec::new();
    for r in reports {
        for line in r.body.lines() {
            if let Some(rest) = line.trim().strip_prefix("ACTION ")
                && let Some((assignee, desc)) = rest.split_once(':')
            {
                let assignee = assignee.trim();
                let (assignee, gated) = match assignee.strip_suffix('!') {
                    Some(a) => (a.trim(), true),
                    None => (assignee, false),
                };
                if !assignee.is_empty() {
                    action_items.push(ActionItem {
                        assignee: assignee.to_string(),
                        description: desc.trim().to_string(),
                        needs_owner_approval: gated,
                    });
                }
            }
        }
    }
    Minutes { decisions, action_items }
}

/// Build the structured `minutes` bus-message body for dvalin to record.
pub fn minutes_message_body(meeting: &str, minutes: &Minutes) -> String {
    serde_json::json!({
        "kind": "minutes",
        "meeting": meeting,
        "decisions": minutes.decisions,
        "action_items": minutes.action_items,
    })
    .to_string()
}

/// Whether a message is the chair's own (don't treat our own posts as reports).
pub fn is_structured_minutes(m: &BusMessage) -> bool {
    m.kind == KIND_STRUCTURED
        && serde_json::from_str::<serde_json::Value>(&m.body)
            .ok()
            .and_then(|v| v.get("kind").and_then(|k| k.as_str()).map(|s| s == "minutes"))
            .unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn msg(from: &str, body: &str) -> BusMessage {
        BusMessage {
            id: 1, sender: from.into(), recipient: "ceo@hq".into(),
            kind: "unstructured".into(), body: body.into(), ref_id: None, channel: None,
        }
    }

    #[test]
    fn collects_reports_from_free_text_and_structured() {
        let inbox = vec![
            msg("cfo@hq", "budget on track"),
            BusMessage { kind: KIND_STRUCTURED.into(), body: r#"{"kind":"report","body":"release green"}"#.into(), ..msg("cto@hq", "") },
            msg("cao@hq", "   "), // empty → skipped
        ];
        let reports = collect_reports(&inbox);
        assert_eq!(reports.len(), 2);
        assert_eq!(reports[0].from, "cfo@hq");
        assert_eq!(reports[1].body, "release green");
    }

    #[test]
    fn compiles_minutes_with_decisions_and_actions() {
        let agenda = vec!["incidents".to_string(), "delivery".to_string()];
        let reports = vec![
            Report { from: "cto@hq".into(), body: "all green\nACTION cto@hq: spike retry knob".into() },
            Report { from: "cfo@hq".into(), body: "ACTION cfo@hq!: approve $5k spend".into() },
        ];
        let m = compile(&agenda, &reports, 3);
        assert!(m.decisions.contains(&"incidents: reviewed".to_string()));
        assert!(m.decisions.iter().any(|d| d.contains("open ITIL items carried: 3")));
        assert_eq!(m.action_items.len(), 2);
        assert_eq!(m.action_items[0].assignee, "cto@hq");
        assert!(!m.action_items[0].needs_owner_approval);
        // the `!` suffix marks owner-gated
        assert_eq!(m.action_items[1].assignee, "cfo@hq");
        assert!(m.action_items[1].needs_owner_approval);
    }

    #[test]
    fn minutes_message_is_well_formed() {
        let m = Minutes { decisions: vec!["x".into()], action_items: vec![ActionItem { assignee: "a@b".into(), description: "do".into(), needs_owner_approval: false }] };
        let body = minutes_message_body("board", &m);
        let v: serde_json::Value = serde_json::from_str(&body).unwrap();
        assert_eq!(v["kind"], "minutes");
        assert_eq!(v["meeting"], "board");
        assert_eq!(v["action_items"][0]["assignee"], "a@b");
        // round-trips into the structured-minutes detector
        let bm = BusMessage { id: 0, sender: "ceo@hq".into(), recipient: "dvalin@hq".into(), kind: KIND_STRUCTURED.into(), body, ref_id: None, channel: None };
        assert!(is_structured_minutes(&bm));
    }
}
