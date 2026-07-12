//! Pure `Response -> String` render functions (FEAT-070 / DISC-020).
//!
//! Each function here takes already-unwrapped response data and returns the
//! text block `regin` prints for it. Keeping these pure (no I/O, no daemon
//! calls) makes them directly unit-testable; `main.rs`'s `cmd_*` wrappers do
//! nothing but fetch data via a [`crate::transport::Transport`] and print the
//! result of one of these functions.
//!
//! This intentionally drops the old per-segment ANSI colouring in favour of
//! plain text — a deliberate simplification so every command's output is a
//! single comparable `String` in tests.

use regin_core::audit::Finding;
use regin_core::desired::{Assertion, DesiredInfo, DesiredState};
use regin_core::filters::FilterRule;
use regin_core::greeting::Greeting;
use regin_core::kpi::{KpiSummary, Objective};
use regin_core::promotion::DerivedCheck;
use regin_core::protocol::Response;
use regin_core::soul::ValueEntry;
use regin_core::types::{
    Change, Incident, Memory, Principle, Problem, ProblemHypothesis, Schedule, SkillInfo, TaskRun,
};

/// Short id prefix used throughout the CLI's list views.
pub fn sid(id: &str) -> &str {
    &id[..id.len().min(8)]
}

pub fn fmt_secs(secs: i64) -> String {
    if secs < 60 {
        format!("{secs}s")
    } else if secs < 3600 {
        format!("{}m", secs / 60)
    } else if secs < 86400 {
        format!("{}h{}m", secs / 3600, (secs % 3600) / 60)
    } else {
        format!("{}d{}h", secs / 86400, (secs % 86400) / 3600)
    }
}

pub fn render_ok(message: &str) -> String {
    format!("✓ {message}\n")
}

pub fn render_ping(up: bool) -> String {
    if up {
        "regind is running ✓\n".to_string()
    } else {
        "regind is not reachable\n".to_string()
    }
}

pub fn render_task_list(skills: &[SkillInfo]) -> String {
    if skills.is_empty() {
        return "No tasks found.\n  Add skills to ~/.config/regin/skills/ or /usr/share/regin/skills/\n".to_string();
    }
    let mut out = format!("Tasks ({}):\n", skills.len());
    for s in skills {
        out += &format!("  {:<20}[{:<6}] {}\n", s.name, s.source, s.description);
    }
    out
}

pub fn render_task_show(name: &str, description: &str, prompt: &str, files: &[String]) -> String {
    let mut out = format!("Task: {name}\n  {description}\n\n— prompt —\n{prompt}\n");
    if !files.is_empty() {
        out += &format!("Supporting files ({}):\n", files.len());
        for f in files {
            out += &format!("  • {f}\n");
        }
    }
    out
}

pub fn render_schedules(schedules: &[Schedule]) -> String {
    if schedules.is_empty() {
        return "No active schedules.\n".to_string();
    }
    let mut out = format!("Schedules ({}):\n", schedules.len());
    for s in schedules {
        out += &format!("  {:<20}  {:<10}  next: {}", s.skill, s.interval, s.next_run);
        if let Some(last) = &s.last_run {
            out += &format!("  last: {last}");
        }
        out += "\n";
    }
    out
}

pub fn render_runs(runs: &[TaskRun]) -> String {
    if runs.is_empty() {
        return "No task runs found.\n".to_string();
    }
    let mut out = format!("Task runs ({}):\n", runs.len());
    for r in runs {
        out += &format!("  {} | {:<20} | {}\n", r.started_at, r.skill_name, r.status);
        if let Some(first) = r.output.lines().next() {
            let preview: String = first.chars().take(100).collect();
            out += &format!("    {preview}\n");
        }
    }
    out
}

pub fn render_config_entries(entries: &[(String, String)]) -> String {
    let mut out = "Settings:\n".to_string();
    for (key, value) in entries {
        let display = if key.contains("api_key") && value.len() > 8 {
            format!("{}…{}", &value[..4], &value[value.len() - 4..])
        } else if key.contains("api_key") && !value.is_empty() {
            "****".into()
        } else {
            value.clone()
        };
        out += &format!("  {key:<25}{display}\n");
    }
    out
}

pub fn render_config_value(key: &str, value: &str) -> String {
    if key.contains("api_key") && value.len() > 8 {
        format!("{}…{}\n", &value[..4], &value[value.len() - 4..])
    } else {
        format!("{value}\n")
    }
}

pub fn render_memory_list(memories: &[Memory]) -> String {
    if memories.is_empty() {
        return "No memories stored.\n  Save one with: regin memory save <category> '<content>'\n".to_string();
    }
    let mut out = format!("Memories ({}):\n", memories.len());
    for m in memories {
        out += &format!("  {}  [{:<10}] {}\n", sid(&m.id), m.category, m.content);
        if m.source == "reflection" {
            out += &format!("            ⟳ reflection · strength {}\n", m.strength);
        }
    }
    out
}

pub fn render_memory_search(memories: &[Memory]) -> String {
    if memories.is_empty() {
        return "No matching memories.\n".to_string();
    }
    let mut out = String::new();
    for m in memories {
        out += &format!("  {}  [{:<10}] {}\n", sid(&m.id), m.category, m.content);
    }
    out
}

pub fn render_memory_info(
    identity_id: &str,
    name: &str,
    host: &str,
    schema_version: &str,
    memory_count: i64,
    created_at: &str,
) -> String {
    format!(
        "Identity:    {identity_id}\nName:        {name}\nHost:        {host}\nSchema:      {schema_version}\nMemories:    {memory_count}\nCreated:     {created_at}\n"
    )
}

pub fn render_reflect_stats(episodes: u32, reinforced: u32, created: u32, decayed: u32) -> String {
    format!(
        "✓ Reflection: {episodes} episodes → {reinforced} reinforced, {created} new, {decayed} decayed\n"
    )
}

pub fn render_memory_export(path: &str) -> String {
    format!("✓ Exported identity to {path}\n")
}

pub fn render_task_created(name: &str, path: &str, shadows_system: bool) -> String {
    let mut out = format!("✓ Created skill '{name}' at {path}\n");
    if shadows_system {
        out += &format!("  note: this user skill shadows a system skill named '{name}'\n");
    }
    out
}

pub fn render_context_show(repo_key: Option<&str>, content: Option<&str>) -> String {
    let mut out = match repo_key {
        Some(k) => format!("repo: {k}\n"),
        None => "(no repo resolved for the current directory)\n".to_string(),
    };
    out += &match content {
        Some(c) => format!("{c}\n"),
        None => "  (no context stored — set one with: regin context set '<text>')\n".to_string(),
    };
    out
}

pub fn render_incidents(incidents: &[Incident]) -> String {
    if incidents.is_empty() {
        return "No incidents.\n".to_string();
    }
    let mut out = String::new();
    for i in incidents {
        out += &format!("  {}  [{:<13}] {:<8} {}\n", sid(&i.id), i.status, i.severity, i.title);
        if !i.description.is_empty() {
            out += &format!("            {}\n", i.description);
        }
        if let Some(w) = &i.workaround {
            out += &format!("            workaround: {w}\n");
        }
        if let Some(r) = &i.resolution {
            out += &format!("            resolution: {r}\n");
        }
    }
    out
}

pub fn render_changes(changes: &[Change]) -> String {
    if changes.is_empty() {
        return "No changes.\n".to_string();
    }
    let mut out = String::new();
    for c in changes {
        out += &format!("  {}  [{:<8}] {}\n", sid(&c.id), c.status, c.title);
        if let Some(inc) = &c.incident_id {
            out += &format!("            incident: {}\n", sid(inc));
        }
        if let Some(p) = &c.problem_id {
            out += &format!("            problem: {}\n", sid(p));
        }
        if c.before.is_some() || c.after.is_some() {
            out += &format!(
                "            {} -> {}\n",
                c.before.as_deref().unwrap_or("?"),
                c.after.as_deref().unwrap_or("?")
            );
        }
        if let Some(by) = &c.approved_by {
            out += &format!("            approved by {by}\n");
        }
    }
    out
}

pub fn render_problems(problems: &[Problem]) -> String {
    if problems.is_empty() {
        return "No problems.\n".to_string();
    }
    let mut out = String::new();
    for p in problems {
        out += &format!("  {}  [{:<11}] {}\n", sid(&p.id), p.status, p.title);
        if let Some(rc) = &p.root_cause {
            out += &format!("            root cause: {rc}\n");
        }
    }
    out
}

pub fn render_hypotheses(hypotheses: &[ProblemHypothesis]) -> String {
    if hypotheses.is_empty() {
        return "No hypotheses.\n".to_string();
    }
    let mut out = String::new();
    for h in hypotheses {
        out += &format!("  {}  [{:<10}] {}\n", sid(&h.id), h.status, h.text);
    }
    out
}

pub fn render_desired_list(items: &[DesiredInfo]) -> String {
    if items.is_empty() {
        return "No desired-state domains. Add files under ~/.config/regin/desired/<domain>.md\n".to_string();
    }
    let mut out = String::new();
    for d in items {
        out += &format!("  {:<16}[{}] {} assertion(s)", d.domain, d.source, d.assertions);
        if let Some(rt) = d.recurrence_threshold {
            out += &format!(", recurrence>={rt}");
        }
        out += "\n";
        for c in &d.conflicts {
            out += &format!("        conflict: {c}\n");
        }
    }
    out
}

pub fn render_desired_show(state: &DesiredState) -> String {
    let mut out = format!("{} [{}] {}\n", state.domain, state.source, state.path.display());
    if let Some(rt) = state.recurrence_threshold {
        out += &format!("recurrence threshold: {rt}\n");
    }
    if !state.intent.is_empty() {
        out += &format!("\n{}\n", state.intent);
    }
    if !state.assertions.is_empty() {
        out += "\nassertions:\n";
        for a in &state.assertions {
            out += &render_assertion(a);
            out += "\n";
        }
    }
    out
}

fn render_assertion(a: &Assertion) -> String {
    match &a.description {
        Some(d) => format!("  {a}  — {d}"),
        None => format!("  {a}"),
    }
}

pub fn render_mode(mode: &str, configured: bool, last_ok: Option<&str>, failures: u32) -> String {
    format!(
        "effective mode: {mode}\n  bus configured: {configured}\n  last reachable: {}\n  consecutive failures: {failures}\n",
        last_ok.unwrap_or("never")
    )
}

pub fn render_posture(
    posture: &str,
    allow_auto: bool,
    change_successes: i64,
    change_failures: i64,
    change_success_rate: f64,
    promotion_error_rate: f64,
) -> String {
    let note = if posture == "conservative" {
        "  safe fixes still route to approval until trust is earned"
    } else {
        "  safe, reversible fixes may auto-apply"
    };
    format!(
        "autonomy posture: {posture}\n  master switch (posture.allow_auto): {allow_auto}\n  change outcomes: {change_successes} ok / {change_failures} failed ({:.0}% success)\n  promotion error rate: {:.0}%\n{note}\n",
        change_success_rate * 100.0,
        promotion_error_rate * 100.0,
    )
}

pub fn render_greeting(g: &Greeting) -> String {
    let mut out = format!("{}\n", g.health_line());
    if !g.has_actions() {
        return out;
    }
    if !g.pending_changes.is_empty() {
        out += "changes awaiting approval:\n";
        for a in &g.pending_changes {
            out += &format!("  {}  {}\n", sid(&a.id), a.title);
        }
    }
    if !g.decision_problems.is_empty() {
        out += "problems needing a decision:\n";
        for a in &g.decision_problems {
            out += &format!("  {}  {}\n", sid(&a.id), a.title);
        }
    }
    out
}

pub fn render_filters(rules: &[FilterRule]) -> String {
    if rules.is_empty() {
        return "No notice filters. Add rule files under ~/.config/regin/filters/*.toml\n".to_string();
    }
    let mut out = String::new();
    for r in rules {
        out += &format!("  {:<20}[{}] contains {:?}", r.name, r.source, r.contains);
        if let Some(d) = &r.domain {
            out += &format!(" (domain: {d})");
        }
        out += "\n";
    }
    out
}

pub fn render_metrics(summary: &KpiSummary, objective: &Objective, days: Option<u32>) -> String {
    let verdict = if objective.meets_floor { "MEETS floor" } else { "BELOW floor" };
    format!(
        "CSI metrics — last {} days\n\n\
         Objective (minimize cost s.t. reliability >= floor)\n\
         \x20 reliability {:.0}% (floor {:.0}%)  {verdict}\n\
         \x20 LLM cost: ${:.2}\n\n\
         Reliability / quality\n\
         \x20 incidents: {} opened, {} resolved, {} open\n\
         \x20 time in deviation: {}\n\
         \x20 MTTR: {}\n\
         \x20 recurring problems: {}\n\n\
         Automation / autonomy\n\
         \x20 remediations: {} auto, {} approved, {} escalated\n\
         \x20 automation ratio: {:.0}%   autonomy ratio: {:.0}%\n\n\
         Cost / efficiency\n\
         \x20 LLM spend: ${:.2}   avoided: ${:.2}   notices filtered: {}\n\n\
         Learning / health\n\
         \x20 promotions: {}   errors: {}   error rate: {:.0}%\n",
        days.unwrap_or(30),
        objective.reliability * 100.0,
        objective.reliability_floor * 100.0,
        objective.cost_llm_usd,
        summary.incidents_opened,
        summary.incidents_resolved,
        summary.open_incidents,
        fmt_secs(summary.time_in_deviation_secs),
        summary.mttr_secs.map(fmt_secs).unwrap_or_else(|| "n/a".to_string()),
        summary.recurring_problems,
        summary.remediations_auto,
        summary.remediations_approved,
        summary.remediations_escalated,
        summary.automation_ratio * 100.0,
        summary.autonomy_ratio * 100.0,
        summary.cost_llm_usd,
        summary.cost_avoided_usd,
        summary.notice_filter_saved,
        summary.promotions,
        summary.promotion_errors,
        summary.promotion_error_rate * 100.0,
    )
}

pub fn render_audit(findings: &[Finding], trimmed: bool, opened: usize) -> String {
    let mut out = String::new();
    if trimmed {
        out += "(audit trimmed to stay within budget)\n";
    }
    if findings.is_empty() {
        out += "Self-audit clean — no findings.\n";
        return out;
    }
    for f in findings {
        out += &format!("  [{}] {}\n", f.area, f.message);
    }
    out += &format!("{opened} new problem(s) filed for review.\n");
    out
}

// ---------------------------------------------------------------------------
// Streaming events (chat turns, task-exec tool activity) — FEAT-070
// ---------------------------------------------------------------------------

/// Fold one event from a `chat_send` stream into the accumulated reply text
/// and (once seen) the conversation id. Pure logic, separated from the
/// terminal colouring done by the caller.
pub fn apply_chat_event(resp: &Response, full: &mut String, conv_id: &mut String) {
    match resp {
        Response::StreamChunk { token } => full.push_str(token),
        Response::StreamDone { conversation_id } => conv_id.clone_from(conversation_id),
        _ => {}
    }
}

pub fn render_tool_call(name: &str, arguments: &str) -> String {
    let preview: String = arguments.chars().take(120).collect();
    format!("▶ {name} {preview}")
}

pub fn render_tool_result(name: &str, success: bool, output: &str, max_lines: usize) -> String {
    let icon = if success { "✓" } else { "✗" };
    let preview: String = output.lines().take(max_lines).collect::<Vec<_>>().join("\n    ");
    if preview.is_empty() {
        format!("  {icon} {name}")
    } else {
        format!("  {icon} {name}\n    {preview}")
    }
}

pub fn render_task_result(run: &TaskRun) -> String {
    format!("\nStatus: {}\n\n{}\n", run.status, run.output)
}

// ---------------------------------------------------------------------------
// Soul configurator + value catalog (FEAT-030)
// ---------------------------------------------------------------------------

pub fn render_soul_values_list(version: &str, values: &[ValueEntry]) -> String {
    let mut out = format!("Value catalog (v{version}, {} entries):\n", values.len());
    for v in values {
        out += &format!("  {:<20}[{:<14}] {}\n", v.id, v.tradition, v.name);
    }
    out
}

pub fn render_soul_value_detail(v: &ValueEntry) -> String {
    format!("{} ({})\ntradition: {}\n{}\n", v.name, v.id, v.tradition, v.description)
}

pub fn render_soul_charter(core_ids: &[String], persona_overlay: &[String], grounding: &[String]) -> String {
    let mut out = String::new();
    out += &format!("core charter ({}):\n", core_ids.len());
    if core_ids.is_empty() {
        out += "  (empty — seed it with: regin soul charter set <id>...)\n";
    } else {
        for id in core_ids {
            out += &format!("  {id}\n");
        }
    }
    if !persona_overlay.is_empty() {
        out += &format!("persona overlay ({}):\n", persona_overlay.len());
        for id in persona_overlay {
            out += &format!("  {id}\n");
        }
    }
    out += &format!("grounding — what the Soul reads ({}):\n", grounding.len());
    for id in grounding {
        out += &format!("  {id}\n");
    }
    out
}

pub fn render_soul_charter_proposal(role: &str, proposed: &[String]) -> String {
    let mut out = format!("proposed starting values for role '{role}':\n");
    for id in proposed {
        out += &format!("  {id}\n");
    }
    out += &format!("\nreview, then confirm with: regin soul charter set {}\n", proposed.join(" "));
    out
}

pub fn render_soul_charter_written(added: &[String]) -> String {
    if added.is_empty() {
        "✓ no new values (already in the core charter)\n".to_string()
    } else {
        format!("✓ added to the core charter: {}\n", added.join(", "))
    }
}

// ---------------------------------------------------------------------------
// Principle derivation & ratification (FEAT-031)
// ---------------------------------------------------------------------------

pub fn render_soul_principles(principles: &[Principle]) -> String {
    if principles.is_empty() {
        return "no principles\n".to_string();
    }
    let mut out = format!("principles ({}):\n", principles.len());
    for p in principles {
        out += &format!("  {} [{:<9}] ({}) {}\n", sid(&p.id), p.status, p.source, p.content);
        if !p.evidence.is_empty() {
            out += &format!("      evidence: {} deliberation(s)\n", p.evidence.len());
        }
    }
    out
}

pub fn render_soul_principle_ratified(p: &Principle) -> String {
    format!("✓ ratified {} — now active: {}\n", sid(&p.id), p.content)
}

pub fn render_soul_principle_rejected(p: &Principle) -> String {
    format!("✓ retired {}: {}\n", sid(&p.id), p.content)
}

pub fn render_checks(checks: &[DerivedCheck]) -> String {
    if checks.is_empty() {
        return "No derived checks yet. regin promotes stable LLM verdicts into cheap checks over time.\n".to_string();
    }
    let mut out = String::new();
    for c in checks {
        out += &format!("  {:<16}{}  [{}]\n", c.domain, c.description, c.signature);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use regin_core::desired::{AssertOp, AssertValue, DesiredSource};
    use regin_core::filters::FilterSource;
    use regin_core::kpi::{KpiSummary, Objective};

    fn incident(status: &str) -> Incident {
        Incident {
            id: "incident-0001".into(),
            title: "disk full".into(),
            description: "".into(),
            severity: "high".into(),
            status: status.into(),
            source: "monitor".into(),
            skill_name: None,
            workaround: None,
            opened_at: "now".into(),
            updated_at: "now".into(),
            resolved_at: None,
            resolution: None,
        }
    }

    #[test]
    fn render_ok_has_checkmark() {
        assert_eq!(render_ok("saved"), "✓ saved\n");
    }

    #[test]
    fn render_ping_reports_state() {
        assert_eq!(render_ping(true), "regind is running ✓\n");
        assert_eq!(render_ping(false), "regind is not reachable\n");
    }

    #[test]
    fn render_task_list_empty() {
        let out = render_task_list(&[]);
        assert!(out.contains("No tasks found"));
    }

    #[test]
    fn render_task_list_shows_name_source_description() {
        let skills = vec![SkillInfo { name: "disk-usage".into(), description: "check disk".into(), source: "user".into() }];
        let out = render_task_list(&skills);
        assert!(out.contains("disk-usage"));
        assert!(out.contains("user"));
        assert!(out.contains("check disk"));
    }

    #[test]
    fn render_task_show_lists_files() {
        let out = render_task_show("disk-usage", "desc", "prompt text", &["notes.md".to_string()]);
        assert!(out.contains("Task: disk-usage"));
        assert!(out.contains("prompt text"));
        assert!(out.contains("notes.md"));
    }

    #[test]
    fn render_schedules_empty_and_nonempty() {
        assert!(render_schedules(&[]).contains("No active schedules"));
        let s = vec![Schedule {
            id: "1".into(),
            skill: "disk-usage".into(),
            interval: "daily".into(),
            next_run: "tomorrow".into(),
            last_run: Some("yesterday".into()),
        }];
        let out = render_schedules(&s);
        assert!(out.contains("disk-usage"));
        assert!(out.contains("last: yesterday"));
    }

    #[test]
    fn render_runs_shows_status_and_preview() {
        let runs = vec![TaskRun {
            id: "1".into(),
            skill_name: "disk-usage".into(),
            status: "success".into(),
            output: "line one\nline two".into(),
            started_at: "t0".into(),
            finished_at: "t1".into(),
        }];
        let out = render_runs(&runs);
        assert!(out.contains("success"));
        assert!(out.contains("line one"));
        assert!(!out.contains("line two"));
    }

    #[test]
    fn render_config_entries_masks_api_key() {
        let entries = vec![("mimir.api_key".to_string(), "abcdefgh1234".to_string())];
        let out = render_config_entries(&entries);
        assert!(out.contains("abcd…1234"));
        assert!(!out.contains("abcdefgh1234"));
    }

    #[test]
    fn render_config_entries_short_api_key_is_starred() {
        let entries = vec![("mimir.api_key".to_string(), "short".to_string())];
        let out = render_config_entries(&entries);
        assert!(out.contains("****"));
    }

    #[test]
    fn render_config_value_masks_api_key() {
        assert_eq!(render_config_value("mimir.api_key", "abcdefgh1234"), "abcd…1234\n");
        assert_eq!(render_config_value("mimir.model", "gpt-4o"), "gpt-4o\n");
    }

    #[test]
    fn render_memory_list_flags_reflection_source() {
        let memories = vec![Memory {
            id: "mem-0001".into(),
            category: "fact".into(),
            content: "runs Ubuntu".into(),
            created_at: "now".into(),
            updated_at: "now".into(),
            strength: 3,
            last_seen: None,
            source: "reflection".into(),
        }];
        let out = render_memory_list(&memories);
        assert!(out.contains("runs Ubuntu"));
        assert!(out.contains("reflection"));
        assert!(out.contains("strength 3"));
    }

    #[test]
    fn render_memory_search_empty() {
        assert!(render_memory_search(&[]).contains("No matching memories"));
    }

    #[test]
    fn render_memory_info_lists_all_fields() {
        let out = render_memory_info("id1", "regin", "host1", "3", 42, "2026-01-01");
        assert!(out.contains("id1"));
        assert!(out.contains("host1"));
        assert!(out.contains("42"));
    }

    #[test]
    fn render_reflect_stats_summarises_counts() {
        let out = render_reflect_stats(10, 3, 2, 1);
        assert!(out.contains("10 episodes"));
        assert!(out.contains("3 reinforced"));
        assert!(out.contains("2 new"));
        assert!(out.contains("1 decayed"));
    }

    #[test]
    fn render_task_created_notes_shadowing() {
        let out = render_task_created("disk-usage", "/path/skill.md", true);
        assert!(out.contains("Created skill 'disk-usage'"));
        assert!(out.contains("shadows a system skill"));
        let out2 = render_task_created("disk-usage", "/path/skill.md", false);
        assert!(!out2.contains("shadows"));
    }

    #[test]
    fn render_context_show_no_repo_no_content() {
        let out = render_context_show(None, None);
        assert!(out.contains("no repo resolved"));
        assert!(out.contains("no context stored"));
    }

    #[test]
    fn render_context_show_with_repo_and_content() {
        let out = render_context_show(Some("/repo"), Some("notes"));
        assert!(out.contains("repo: /repo"));
        assert!(out.contains("notes"));
    }

    #[test]
    fn render_incidents_includes_workaround_and_resolution() {
        let mut i = incident("blocked");
        i.workaround = Some("restarted service".into());
        let out = render_incidents(&[i]);
        assert!(out.contains("blocked"));
        assert!(out.contains("restarted service"));

        let mut i2 = incident("resolved");
        i2.resolution = Some("cleared temp files".into());
        let out2 = render_incidents(&[i2]);
        assert!(out2.contains("cleared temp files"));
    }

    #[test]
    fn render_incidents_empty() {
        assert_eq!(render_incidents(&[]), "No incidents.\n");
    }

    #[test]
    fn render_changes_shows_linkage_and_approval() {
        let c = Change {
            id: "change-0001".into(),
            title: "bump disk".into(),
            description: "".into(),
            status: "applied".into(),
            incident_id: Some("incident-0001".into()),
            problem_id: None,
            before: Some("80%".into()),
            after: Some("40%".into()),
            approved_by: Some("rene".into()),
            approved_at: Some("now".into()),
            created_at: "now".into(),
            applied_at: Some("now".into()),
        };
        let out = render_changes(&[c]);
        assert!(out.contains("incident: incident"));
        assert!(out.contains("80% -> 40%"));
        assert!(out.contains("approved by rene"));
    }

    #[test]
    fn render_problems_shows_root_cause() {
        let p = Problem {
            id: "problem-0001".into(),
            title: "recurring disk full".into(),
            description: "".into(),
            status: "known_error".into(),
            root_cause: Some("log rotation misconfigured".into()),
            created_at: "now".into(),
            updated_at: "now".into(),
            closed_at: None,
        };
        let out = render_problems(&[p]);
        assert!(out.contains("log rotation misconfigured"));
    }

    #[test]
    fn render_hypotheses_lists_text_and_status() {
        let h = ProblemHypothesis {
            id: "hyp-0001".into(),
            problem_id: "problem-0001".into(),
            text: "cron misfires".into(),
            status: "validating".into(),
            created_at: "now".into(),
            updated_at: "now".into(),
        };
        let out = render_hypotheses(&[h]);
        assert!(out.contains("cron misfires"));
        assert!(out.contains("validating"));
    }

    #[test]
    fn render_desired_list_flags_conflicts_and_recurrence() {
        let items = vec![DesiredInfo {
            domain: "disk".into(),
            source: DesiredSource::User,
            assertions: 2,
            recurrence_threshold: Some(3),
            conflicts: vec!["ambiguous free-space target".into()],
        }];
        let out = render_desired_list(&items);
        assert!(out.contains("2 assertion(s)"));
        assert!(out.contains("recurrence>=3"));
        assert!(out.contains("conflict: ambiguous"));
    }

    #[test]
    fn render_desired_show_includes_intent_and_assertions() {
        let state = DesiredState {
            domain: "disk".into(),
            intent: "keep disk usage low".into(),
            assertions: vec![Assertion {
                key: "free_pct".into(),
                op: AssertOp::Ge,
                value: AssertValue::Num(10.0),
                description: Some("at least 10% free".into()),
            }],
            recurrence_threshold: Some(3),
            cadence: None,
            source: DesiredSource::System,
            path: "/etc/regin/desired/disk.md".into(),
        };
        let out = render_desired_show(&state);
        assert!(out.contains("keep disk usage low"));
        assert!(out.contains("free_pct"));
        assert!(out.contains("at least 10% free"));
        assert!(out.contains("recurrence threshold: 3"));
    }

    #[test]
    fn render_mode_shows_last_reachable_never_when_unset() {
        let out = render_mode("standalone", false, None, 2);
        assert!(out.contains("standalone"));
        assert!(out.contains("never"));
        assert!(out.contains("consecutive failures: 2"));
    }

    #[test]
    fn render_posture_notes_conservative_gate() {
        let out = render_posture("conservative", false, 5, 1, 0.8333, 0.0);
        assert!(out.contains("still route to approval"));
        let out2 = render_posture("trusted", true, 5, 1, 0.8333, 0.0);
        assert!(out2.contains("may auto-apply"));
    }

    #[test]
    fn render_greeting_no_actions_is_just_health_line() {
        let g = Greeting {
            mode: "standalone".into(),
            open_incidents: 0,
            open_problems: 0,
            pending_changes: vec![],
            decision_problems: vec![],
        };
        let out = render_greeting(&g);
        assert_eq!(out.trim_end(), g.health_line());
    }

    #[test]
    fn render_filters_shows_domain_when_set() {
        let rules = vec![FilterRule {
            name: "known-noise".into(),
            contains: "connection reset".into(),
            domain: Some("network".into()),
            source: FilterSource::User,
        }];
        let out = render_filters(&rules);
        assert!(out.contains("domain: network"));
    }

    #[test]
    fn render_metrics_shows_floor_verdict() {
        let summary = KpiSummary {
            since: "2026-01-01".into(),
            incidents_opened: 1,
            incidents_resolved: 1,
            open_incidents: 0,
            time_in_deviation_secs: 120,
            mttr_secs: Some(90),
            recurring_problems: 0,
            remediations_auto: 1,
            remediations_approved: 0,
            remediations_escalated: 0,
            automation_ratio: 1.0,
            autonomy_ratio: 1.0,
            cost_llm_usd: 0.5,
            cost_avoided_usd: 0.1,
            notice_filter_saved: 2,
            promotions: 0,
            promotion_errors: 0,
            promotion_error_rate: 0.0,
            change_successes: 1,
            change_failures: 0,
            change_success_rate: 1.0,
        };
        let objective = Objective { reliability: 0.99, reliability_floor: 0.95, meets_floor: true, cost_llm_usd: 0.5 };
        let out = render_metrics(&summary, &objective, Some(7));
        assert!(out.contains("last 7 days"));
        assert!(out.contains("MEETS floor"));
        assert!(out.contains("MTTR: 1m"));
    }

    #[test]
    fn render_audit_reports_findings_or_clean() {
        assert!(render_audit(&[], false, 0).contains("clean"));
        let findings = vec![Finding { area: "disk".into(), message: "usage rising".into() }];
        let out = render_audit(&findings, true, 1);
        assert!(out.contains("trimmed"));
        assert!(out.contains("usage rising"));
        assert!(out.contains("1 new problem"));
    }

    #[test]
    fn render_checks_lists_domain_and_signature() {
        let checks = vec![DerivedCheck {
            id: "check-1".into(),
            domain: "disk".into(),
            description: "free space >= 10%".into(),
            signature: "disk.free_pct.gte.10".into(),
            status: "active".into(),
            created_at: "now".into(),
            demoted_at: None,
            demote_reason: None,
        }];
        let out = render_checks(&checks);
        assert!(out.contains("free space >= 10%"));
        assert!(out.contains("disk.free_pct.gte.10"));
    }

    #[test]
    fn fmt_secs_buckets() {
        assert_eq!(fmt_secs(30), "30s");
        assert_eq!(fmt_secs(90), "1m");
        assert_eq!(fmt_secs(3661), "1h1m");
        assert_eq!(fmt_secs(90000), "1d1h");
    }

    #[test]
    fn render_soul_values_list_shows_id_tradition_and_name() {
        let values = vec![ValueEntry {
            id: "integrity".into(),
            name: "Integrity".into(),
            description: "Never fabricate.".into(),
            tradition: "agent-operational".into(),
        }];
        let out = render_soul_values_list("1", &values);
        assert!(out.contains("v1"));
        assert!(out.contains("integrity"));
        assert!(out.contains("agent-operational"));
        assert!(out.contains("Integrity"));
    }

    #[test]
    fn render_soul_value_detail_includes_description() {
        let v = ValueEntry {
            id: "prudence".into(),
            name: "Prudence".into(),
            description: "Practical wisdom.".into(),
            tradition: "cardinal".into(),
        };
        let out = render_soul_value_detail(&v);
        assert!(out.contains("Prudence"));
        assert!(out.contains("Practical wisdom."));
    }

    #[test]
    fn render_soul_charter_shows_empty_core_hint() {
        let out = render_soul_charter(&[], &[], &[]);
        assert!(out.contains("seed it with"));
    }

    #[test]
    fn render_soul_charter_lists_core_overlay_and_grounding() {
        let core = vec!["integrity".to_string()];
        let overlay = vec!["prudence".to_string()];
        let grounding = vec!["integrity".to_string(), "prudence".to_string()];
        let out = render_soul_charter(&core, &overlay, &grounding);
        assert!(out.contains("core charter (1)"));
        assert!(out.contains("persona overlay (1)"));
        assert!(out.contains("grounding"));
    }

    #[test]
    fn render_soul_charter_proposal_shows_confirm_command() {
        let out = render_soul_charter_proposal("cfo", &["prudence".to_string(), "integrity".to_string()]);
        assert!(out.contains("role 'cfo'"));
        assert!(out.contains("regin soul charter set prudence integrity"));
    }

    #[test]
    fn render_soul_charter_written_reports_added_or_nothing_new() {
        assert!(render_soul_charter_written(&[]).contains("no new values"));
        assert!(render_soul_charter_written(&["integrity".to_string()]).contains("integrity"));
    }

    fn sample_principle(status: &str) -> Principle {
        Principle {
            id: "11111111-2222-3333-4444-555555555555".into(),
            content: "recurring failures — be more careful".into(),
            status: status.into(),
            source: "reflection".into(),
            evidence: vec!["ep1".into(), "ep2".into()],
            created_at: "2026-01-01T00:00:00Z".into(),
            updated_at: "2026-01-01T00:00:00Z".into(),
        }
    }

    #[test]
    fn render_soul_principles_empty_says_so() {
        assert!(render_soul_principles(&[]).contains("no principles"));
    }

    #[test]
    fn render_soul_principles_shows_status_source_content_and_evidence_count() {
        let out = render_soul_principles(&[sample_principle("candidate")]);
        assert!(out.contains("candidate"));
        assert!(out.contains("reflection"));
        assert!(out.contains("recurring failures"));
        assert!(out.contains("2 deliberation"));
    }

    #[test]
    fn render_soul_principle_ratified_reports_active() {
        assert!(render_soul_principle_ratified(&sample_principle("active")).contains("active"));
    }

    #[test]
    fn render_soul_principle_rejected_reports_retired() {
        assert!(render_soul_principle_rejected(&sample_principle("retired")).contains("retired"));
    }

    #[test]
    fn sid_truncates_to_eight_chars() {
        assert_eq!(sid("abcdefghijkl"), "abcdefgh");
        assert_eq!(sid("short"), "short");
    }

    #[test]
    fn apply_chat_event_accumulates_tokens_and_captures_conversation_id() {
        let mut full = String::new();
        let mut conv_id = String::new();
        apply_chat_event(&Response::StreamChunk { token: "hel".into() }, &mut full, &mut conv_id);
        apply_chat_event(&Response::StreamChunk { token: "lo".into() }, &mut full, &mut conv_id);
        apply_chat_event(&Response::ToolCallEvent { name: "bash".into(), arguments: "{}".into() }, &mut full, &mut conv_id);
        apply_chat_event(&Response::StreamDone { conversation_id: "c1".into() }, &mut full, &mut conv_id);
        assert_eq!(full, "hello");
        assert_eq!(conv_id, "c1");
    }

    #[test]
    fn render_tool_call_truncates_long_arguments() {
        let args = "x".repeat(200);
        let out = render_tool_call("bash", &args);
        assert!(out.starts_with("▶ bash "));
        assert_eq!(out.len(), "▶ bash ".len() + 120);
    }

    #[test]
    fn render_tool_result_shows_icon_and_preview_lines() {
        let ok = render_tool_result("bash", true, "line1\nline2\nline3", 2);
        assert!(ok.starts_with("  ✓ bash"));
        assert!(ok.contains("line1"));
        assert!(ok.contains("line2"));
        assert!(!ok.contains("line3"));

        let failed = render_tool_result("bash", false, "", 2);
        assert_eq!(failed, "  ✗ bash");
    }

    #[test]
    fn render_task_result_includes_status_and_output() {
        let run = TaskRun {
            id: "1".into(),
            skill_name: "disk-usage".into(),
            status: "success".into(),
            output: "42% used".into(),
            started_at: "t0".into(),
            finished_at: "t1".into(),
        };
        let out = render_task_result(&run);
        assert!(out.contains("Status: success"));
        assert!(out.contains("42% used"));
    }
}
