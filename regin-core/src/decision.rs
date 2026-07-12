//! Dual-mode agent loop: act vs deliberate (FEAT-028 / DISC-018).
//!
//! Vocabulary: **Persona** = the role/hat (FEAT-011); **Mind** = the reasoning
//! that plans/decides; **Soul** = the values-grounded gate (FEAT-029); **Body**
//! = tool execution.
//!
//! Two modes:
//! - **act** — `Mind -> Body`, today's single LLM turn + direct tool dispatch,
//!   unchanged, and the default.
//! - **deliberate** — `Mind <-> Soul -> Body`: the Mind produces a read-only
//!   [`Plan`] (no side effects — see [`Planner`]), the Soul gate votes on it,
//!   and only an approved plan reaches the [`Executor`] (the Body). A `revise`
//!   verdict feeds the Soul's reaction back to the Mind for up to
//!   `max_rounds`; a `veto` or exhausted rounds default-denies and signals the
//!   caller to escalate (FEAT-015).
//!
//! Like [`crate::remediation`] and [`crate::safelane`], this is a **pure-ish
//! engine**: mode selection and the deliberate pipeline are fully
//! unit-testable with fakes/spies here. Wiring it into `regind`'s live chat
//! loop (`agentic_chat`) is deliberately out of scope for this ticket — the
//! [`SoulGate`] here is [`PassthroughSoulGate`], a stub that always approves
//! until FEAT-029 lands the real values-grounded gate; wiring a
//! stub-that-always-approves into production would add risk (a new code path
//! in the live loop) for zero behavioural benefit. `act` mode — today's
//! `chat_turn` path — is untouched by this module, so it is unchanged by
//! construction.

use anyhow::Result;
use async_trait::async_trait;
use serde::{Deserialize, Serialize};

use crate::persona::Persona;
use crate::tools::ToolCall;

/// regin's two decision modes (DISC-018).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Mode {
    /// `Mind -> Body`: one LLM turn, tool calls dispatched directly.
    Act,
    /// `Mind <-> Soul -> Body`: a read-only plan is Soul-gated before any
    /// tool call runs.
    Deliberate,
}

/// The action regin is about to take, described for mode selection. Mirrors
/// the DISC-009 blast-radius/reversibility inputs so both planes reuse one
/// mental model.
#[derive(Debug, Clone, Copy, Default)]
pub struct ContemplatedAction {
    /// A concrete backout/undo exists.
    pub reversible: bool,
    /// Destroys or overwrites state.
    pub destructive: bool,
    /// Visible outside the host (sends a message, calls an external API, ...).
    pub outward_facing: bool,
    /// Time-critical — biases toward `act` even when otherwise risky.
    pub urgent: bool,
}

/// Classifies a contemplated action's risk into a mode. Pluggable so tests
/// can supply a fake instead of depending on the real heuristic (acceptance
/// criterion 1).
pub trait RiskClassifier: Send + Sync {
    fn classify(&self, action: &ContemplatedAction) -> Mode;
}

/// The default classifier: irreversible, destructive, or outward-facing
/// actions need a values check before they run (`Deliberate`); read-only /
/// reversible ones stay fast (`Act`). High urgency overrides everything else
/// — a fire needs fighting, not a committee.
pub struct DefaultRiskClassifier;

impl RiskClassifier for DefaultRiskClassifier {
    fn classify(&self, action: &ContemplatedAction) -> Mode {
        if action.urgent {
            return Mode::Act;
        }
        let risky = !action.reversible || action.destructive || action.outward_facing;
        if risky { Mode::Deliberate } else { Mode::Act }
    }
}

fn parse_mode(s: &str) -> Option<Mode> {
    match s {
        "act" => Some(Mode::Act),
        "deliberate" => Some(Mode::Deliberate),
        _ => None,
    }
}

/// Select the mode for a contemplated action. A Persona's `default_mode`
/// override wins outright — a role may be pinned to one mode; otherwise the
/// classifier decides.
pub fn select_mode(
    action: &ContemplatedAction,
    classifier: &dyn RiskClassifier,
    persona: Option<&Persona>,
) -> Mode {
    if let Some(m) = persona.and_then(|p| p.default_mode.as_deref()).and_then(parse_mode) {
        return m;
    }
    classifier.classify(action)
}

// ---------------------------------------------------------------------------
// Deliberate pipeline
// ---------------------------------------------------------------------------

/// A read-only plan produced by the Mind before any side effect runs.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Plan {
    pub intent_summary: String,
    pub steps: Vec<String>,
    pub intended_tool_calls: Vec<ToolCall>,
}

/// Produces a [`Plan`] with **no side effects**. The production
/// implementation asks the LLM for a plan without dispatching any tool
/// (planning uses a read-only LLM turn); tests supply a fake.
#[async_trait]
pub trait Planner: Send + Sync {
    async fn plan(&self, revision_feedback: Option<&str>) -> Result<Plan>;
}

/// The Soul's verdict on a [`Plan`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SoulVerdict {
    Approve,
    Revise,
    Veto,
}

/// Votes on a [`Plan`]'s acceptability. FEAT-029 supplies the real
/// values-grounded gate; see [`PassthroughSoulGate`] for the stub used until
/// then.
pub trait SoulGate: Send + Sync {
    /// The verdict plus a one-line reaction — fed back to the Mind on
    /// `Revise`, recorded as the escalation reason on `Veto`.
    fn evaluate(&self, plan: &Plan) -> (SoulVerdict, String);
}

/// Stub gate used until FEAT-029 lands the real values-grounded Soul: always
/// approves. Exists so FEAT-028's pipeline can be built and fully tested
/// without a forward dependency on FEAT-029.
pub struct PassthroughSoulGate;

impl SoulGate for PassthroughSoulGate {
    fn evaluate(&self, _plan: &Plan) -> (SoulVerdict, String) {
        (SoulVerdict::Approve, "stub Soul gate (FEAT-029 not yet landed): auto-approved".to_string())
    }
}

/// Carries out an approved [`Plan`]'s intended tool calls (the Body). Each
/// call still passes the Persona ceiling + DISC-009 lanes
/// (`guardrail::check_tool_call`) upstream of this trait — it only executes
/// already-gated calls.
pub trait Executor: Send + Sync {
    fn execute(&mut self, plan: &Plan);
}

/// Outcome of running the deliberate pipeline.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DeliberateOutcome {
    /// The plan was approved and handed to the executor.
    Executed,
    /// The Soul vetoed, or `max_rounds` was reached without approval —
    /// default-deny. The caller escalates (FEAT-015) with `reason`.
    DeniedAndEscalated { reason: String },
}

/// Run the deliberate pipeline: the Mind plans read-only, the Soul gates the
/// plan, and only an approved plan reaches the executor. `Revise` feeds the
/// Soul's reaction back to the Mind for up to `max_rounds` (minimum 1); a
/// `Veto` or exhausted rounds default-denies.
pub async fn run_deliberate(
    planner: &dyn Planner,
    soul: &dyn SoulGate,
    executor: &mut dyn Executor,
    max_rounds: u32,
) -> Result<DeliberateOutcome> {
    let mut feedback: Option<String> = None;
    for _round in 0..max_rounds.max(1) {
        let plan = planner.plan(feedback.as_deref()).await?;
        match soul.evaluate(&plan) {
            (SoulVerdict::Approve, _) => {
                executor.execute(&plan);
                return Ok(DeliberateOutcome::Executed);
            }
            (SoulVerdict::Veto, reason) => {
                return Ok(DeliberateOutcome::DeniedAndEscalated { reason });
            }
            (SoulVerdict::Revise, reason) => {
                feedback = Some(reason);
            }
        }
    }
    Ok(DeliberateOutcome::DeniedAndEscalated {
        reason: format!("max_rounds ({max_rounds}) reached without Soul approval"),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU32, Ordering};
    use std::sync::Mutex;

    // --- Mode selection (acceptance criteria 1, 4) ---

    #[test]
    fn irreversible_destructive_or_outward_actions_go_deliberate() {
        let c = DefaultRiskClassifier;
        assert_eq!(c.classify(&ContemplatedAction { reversible: false, ..Default::default() }), Mode::Deliberate);
        assert_eq!(c.classify(&ContemplatedAction { destructive: true, ..Default::default() }), Mode::Deliberate);
        assert_eq!(c.classify(&ContemplatedAction { outward_facing: true, ..Default::default() }), Mode::Deliberate);
    }

    #[test]
    fn reversible_read_only_actions_stay_in_act_mode() {
        let c = DefaultRiskClassifier;
        let action = ContemplatedAction { reversible: true, destructive: false, outward_facing: false, urgent: false };
        assert_eq!(c.classify(&action), Mode::Act);
    }

    #[test]
    fn urgency_overrides_an_otherwise_risky_classification() {
        let c = DefaultRiskClassifier;
        let action = ContemplatedAction { reversible: false, destructive: true, outward_facing: true, urgent: true };
        assert_eq!(c.classify(&action), Mode::Act);
    }

    /// A fake risk classifier, per acceptance criterion 1 ("unit-tested with
    /// a fake risk classifier") — ignores the action entirely and always
    /// returns a canned mode.
    struct FakeClassifier(Mode);
    impl RiskClassifier for FakeClassifier {
        fn classify(&self, _action: &ContemplatedAction) -> Mode {
            self.0
        }
    }

    #[test]
    fn select_mode_uses_the_classifier_when_no_persona_override() {
        let risky = ContemplatedAction { destructive: true, ..Default::default() };
        assert_eq!(select_mode(&risky, &FakeClassifier(Mode::Act), None), Mode::Act);
        assert_eq!(select_mode(&risky, &FakeClassifier(Mode::Deliberate), None), Mode::Deliberate);
    }

    #[test]
    fn persona_default_mode_overrides_the_classifier() {
        let safe = ContemplatedAction::default();
        let p = Persona::from_toml("role = \"auditor\"\ndefault_mode = \"deliberate\"\n").unwrap();
        // Classifier says Act, but the persona pins deliberate.
        assert_eq!(select_mode(&safe, &FakeClassifier(Mode::Act), Some(&p)), Mode::Deliberate);

        let p2 = Persona::from_toml("role = \"firefighter\"\ndefault_mode = \"act\"\n").unwrap();
        let risky = ContemplatedAction { destructive: true, ..Default::default() };
        assert_eq!(select_mode(&risky, &FakeClassifier(Mode::Deliberate), Some(&p2)), Mode::Act);
    }

    #[test]
    fn persona_without_an_override_falls_back_to_the_classifier() {
        let p = Persona::from_toml("role = \"ops\"\n").unwrap();
        let safe = ContemplatedAction::default();
        assert_eq!(select_mode(&safe, &FakeClassifier(Mode::Deliberate), Some(&p)), Mode::Deliberate);
    }

    // --- Deliberate pipeline (acceptance criteria 2, 3, 4) ---

    struct FixedPlanner {
        calls: AtomicU32,
    }
    impl FixedPlanner {
        fn new() -> Self {
            Self { calls: AtomicU32::new(0) }
        }
    }
    #[async_trait]
    impl Planner for FixedPlanner {
        async fn plan(&self, revision_feedback: Option<&str>) -> Result<Plan> {
            let n = self.calls.fetch_add(1, Ordering::SeqCst);
            Ok(Plan {
                intent_summary: format!("round {n}, feedback={revision_feedback:?}"),
                steps: vec!["do the thing".into()],
                intended_tool_calls: vec![],
            })
        }
    }

    struct FixedVerdictSoul(SoulVerdict);
    impl SoulGate for FixedVerdictSoul {
        fn evaluate(&self, _plan: &Plan) -> (SoulVerdict, String) {
            (self.0, "canned verdict".to_string())
        }
    }

    /// A spy executor that records every plan it was handed — the "zero
    /// executions during planning" and "executed exactly once" assertions.
    #[derive(Default)]
    struct SpyExecutor {
        executed: Mutex<Vec<Plan>>,
    }
    impl Executor for SpyExecutor {
        fn execute(&mut self, plan: &Plan) {
            self.executed.get_mut().unwrap().push(plan.clone());
        }
    }

    #[tokio::test]
    async fn approved_plan_executes_exactly_once() {
        let planner = FixedPlanner::new();
        let soul = FixedVerdictSoul(SoulVerdict::Approve);
        let mut executor = SpyExecutor::default();

        let outcome = run_deliberate(&planner, &soul, &mut executor, 3).await.unwrap();
        assert_eq!(outcome, DeliberateOutcome::Executed);
        assert_eq!(executor.executed.lock().unwrap().len(), 1, "executed exactly once");
        assert_eq!(planner.calls.load(Ordering::SeqCst), 1, "one planning round, no re-plan needed");
    }

    #[tokio::test]
    async fn planning_performs_no_side_effects_before_approval() {
        // A verdict of Veto means the plan is never handed to the executor —
        // proving the planning phase itself is side-effect-free.
        let planner = FixedPlanner::new();
        let soul = FixedVerdictSoul(SoulVerdict::Veto);
        let mut executor = SpyExecutor::default();

        run_deliberate(&planner, &soul, &mut executor, 3).await.unwrap();
        assert!(executor.executed.lock().unwrap().is_empty(), "zero tool executions from planning alone");
    }

    #[tokio::test]
    async fn veto_denies_and_escalates_without_executing() {
        let planner = FixedPlanner::new();
        let soul = FixedVerdictSoul(SoulVerdict::Veto);
        let mut executor = SpyExecutor::default();

        let outcome = run_deliberate(&planner, &soul, &mut executor, 3).await.unwrap();
        match outcome {
            DeliberateOutcome::DeniedAndEscalated { reason } => assert_eq!(reason, "canned verdict"),
            other => panic!("expected DeniedAndEscalated, got {other:?}"),
        }
        assert!(executor.executed.lock().unwrap().is_empty());
    }

    #[tokio::test]
    async fn max_rounds_exhausted_without_approval_denies_and_escalates() {
        let planner = FixedPlanner::new();
        let soul = FixedVerdictSoul(SoulVerdict::Revise); // never approves
        let mut executor = SpyExecutor::default();

        let outcome = run_deliberate(&planner, &soul, &mut executor, 3).await.unwrap();
        assert!(matches!(outcome, DeliberateOutcome::DeniedAndEscalated { .. }));
        assert_eq!(planner.calls.load(Ordering::SeqCst), 3, "max_rounds honoured — exactly 3 planning attempts");
        assert!(executor.executed.lock().unwrap().is_empty());
    }

    #[tokio::test]
    async fn revise_feeds_the_souls_reaction_back_into_the_next_plan() {
        struct FeedbackCapturingPlanner {
            seen: Mutex<Vec<Option<String>>>,
        }
        #[async_trait]
        impl Planner for FeedbackCapturingPlanner {
            async fn plan(&self, revision_feedback: Option<&str>) -> Result<Plan> {
                self.seen.lock().unwrap().push(revision_feedback.map(str::to_string));
                Ok(Plan::default())
            }
        }
        // Revise once, then approve.
        struct RevisesOnceSoul(AtomicU32);
        impl SoulGate for RevisesOnceSoul {
            fn evaluate(&self, _plan: &Plan) -> (SoulVerdict, String) {
                if self.0.fetch_add(1, Ordering::SeqCst) == 0 {
                    (SoulVerdict::Revise, "narrow the blast radius".to_string())
                } else {
                    (SoulVerdict::Approve, "ok now".to_string())
                }
            }
        }

        let planner = FeedbackCapturingPlanner { seen: Mutex::new(Vec::new()) };
        let soul = RevisesOnceSoul(AtomicU32::new(0));
        let mut executor = SpyExecutor::default();

        let outcome = run_deliberate(&planner, &soul, &mut executor, 3).await.unwrap();
        assert_eq!(outcome, DeliberateOutcome::Executed);
        let seen = planner.seen.lock().unwrap();
        assert_eq!(seen.as_slice(), [None, Some("narrow the blast radius".to_string())]);
    }

    #[test]
    fn passthrough_soul_gate_always_approves() {
        let (verdict, _) = PassthroughSoulGate.evaluate(&Plan::default());
        assert_eq!(verdict, SoulVerdict::Approve);
    }

    // --- Act mode unchanged (acceptance criterion 5) ---

    #[test]
    fn act_mode_selection_never_invokes_the_deliberate_pipeline() {
        // This module doesn't touch `agentic_chat` at all (see the module
        // doc comment) — `act` mode is unchanged by construction. What *is*
        // this module's contract: given a low-risk action, select_mode
        // returns Act, and a caller wired to that contract would skip
        // run_deliberate entirely (no Plan, no Soul, no executor call).
        let safe = ContemplatedAction { reversible: true, ..Default::default() };
        assert_eq!(select_mode(&safe, &DefaultRiskClassifier, None), Mode::Act);
    }
}
