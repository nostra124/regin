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
//! loop (`agentic_chat`) is deliberately out of scope for FEAT-028/029 — the
//! [`SoulGate`] trait has both a stub ([`PassthroughSoulGate`], always
//! approves) and the real values-grounded gate ([`LlmSoulGate`], FEAT-029);
//! wiring either into production would add a new code path in the live loop
//! with no caller yet exercising it, so that integration is left to a future
//! ticket. `act` mode — today's `chat_turn` path — is untouched by this
//! module, so it is unchanged by construction.
//!
//! **The Soul is deliberately starved (FEAT-029).** [`LlmSoulGate`] sends the
//! LLM only [`Plan::intent_summary`] and the active values grounding — never
//! the Mind's step-by-step reasoning, its intended tool calls, or any
//! environment detail. That starvation is what makes the vote a *feeling*,
//! not a second round of logic the Mind could out-argue.

use anyhow::{anyhow, Result};
use async_trait::async_trait;
use serde::{Deserialize, Serialize};

use crate::llm::LlmClient;
use crate::persona::Persona;
use crate::tools::ToolCall;
use crate::types::ChatMessage;

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
    /// Identifies this plan for deliberation capture (FEAT-032); production
    /// Planners should stamp a fresh id per plan (e.g. a uuid).
    pub id: String,
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

/// Votes on a [`Plan`]'s acceptability. An LLM call (the real [`LlmSoulGate`],
/// FEAT-029) or a network hiccup can fail, so this is fallible; a fallible
/// vote propagates as an error out of [`run_deliberate`] rather than being
/// silently treated as a verdict.
#[async_trait]
pub trait SoulGate: Send + Sync {
    /// The verdict plus a one-line reaction — fed back to the Mind on
    /// `Revise`, recorded as the escalation reason on `Veto`.
    async fn evaluate(&self, plan: &Plan) -> Result<(SoulVerdict, String)>;
}

/// Stub gate: always approves. Useful for wiring/testing the pipeline itself
/// without depending on an LLM (e.g. FEAT-028's own tests).
pub struct PassthroughSoulGate;

#[async_trait]
impl SoulGate for PassthroughSoulGate {
    async fn evaluate(&self, _plan: &Plan) -> Result<(SoulVerdict, String)> {
        Ok((SoulVerdict::Approve, "stub Soul gate: auto-approved".to_string()))
    }
}

// ---------------------------------------------------------------------------
// The real Soul gate (FEAT-029)
// ---------------------------------------------------------------------------

/// The Soul's raw verdict, as returned by the LLM — distinct from
/// [`SoulVerdict`]: an `approve` below the confidence threshold is *recorded*
/// as `Approve` here but *resolved* to [`SoulVerdict::Revise`] by
/// [`resolve_verdict`] (acceptance criterion 2).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum RawSoulVerdict {
    Approve,
    Revise,
    Veto,
}

/// One cast vote — captured for deliberation calibration (FEAT-032).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SoulVote {
    pub plan_id: String,
    pub confidence: f64,
    pub verdict: RawSoulVerdict,
    pub gut_reaction: String,
}

/// Where cast votes go (FEAT-032 supplies the real durable sink — an
/// `identity.db`-backed recorder — this trait is the seam). [`NullVoteRecorder`]
/// is a stub, same pattern as [`PassthroughSoulGate`].
pub trait VoteRecorder: Send + Sync {
    fn record(&self, vote: &SoulVote);
}

/// Stub recorder: discards every vote. Used where deliberation capture isn't
/// wired up yet.
pub struct NullVoteRecorder;

impl VoteRecorder for NullVoteRecorder {
    fn record(&self, _vote: &SoulVote) {}
}

#[derive(Debug, Deserialize)]
struct RawSoulResponse {
    confidence: f64,
    gut_reaction: String,
    verdict: RawSoulVerdict,
}

const SOUL_SYSTEM_PROMPT: &str = "You are the conscience. Given the plan's intent and these values, \
     return a gut reaction — not a second round of reasoning. \
     Respond with ONLY a JSON object of the shape \
     {\"confidence\": <0.0-1.0>, \"gut_reaction\": \"<one line>\", \"verdict\": \"approve\"|\"revise\"|\"veto\"}.";

/// The user turn sent to the Soul: **only** the plan's intent and the values
/// grounding. Never the Mind's steps, its intended tool calls, or any
/// environment detail (acceptance criterion 1).
fn soul_user_prompt(intent_summary: &str, grounding: &[String]) -> String {
    let values = grounding
        .iter()
        .filter_map(|id| crate::soul::find(id))
        .map(|v| format!("- {} ({}): {}", v.name, v.id, v.description))
        .collect::<Vec<_>>()
        .join("\n");
    format!("Plan intent:\n{intent_summary}\n\nValues grounding:\n{values}\n\nReturn your verdict as JSON.")
}

/// Extract a `{...}` JSON object from `text`, tolerating surrounding prose
/// (LLMs don't always honour "ONLY JSON").
fn extract_json_object(text: &str) -> Option<&str> {
    let start = text.find('{')?;
    let end = text.rfind('}')?;
    (end >= start).then(|| &text[start..=end])
}

fn parse_soul_response(text: &str) -> Result<RawSoulResponse> {
    let json = extract_json_object(text).ok_or_else(|| anyhow!("Soul response has no JSON object: {text:?}"))?;
    serde_json::from_str(json).map_err(|e| anyhow!("malformed Soul response: {e}: {json}"))
}

/// `approve` and `confidence >= threshold` passes; a below-threshold
/// `approve` is treated as `revise`; `revise` stays `revise`; `veto` stays
/// `veto` regardless of confidence (acceptance criterion 2).
fn resolve_verdict(raw: RawSoulVerdict, confidence: f64, threshold: f64) -> SoulVerdict {
    match raw {
        RawSoulVerdict::Veto => SoulVerdict::Veto,
        RawSoulVerdict::Approve if confidence >= threshold => SoulVerdict::Approve,
        RawSoulVerdict::Approve | RawSoulVerdict::Revise => SoulVerdict::Revise,
    }
}

/// The real values-grounded Soul gate (FEAT-029): a tool-less, starved LLM
/// call voting on a [`Plan`]'s intent against the active values grounding
/// (identity-core charter ∪ Persona overlay, FEAT-030 —
/// `soul::grounding_union`).
pub struct LlmSoulGate<'a> {
    pub llm: &'a dyn LlmClient,
    /// Value ids — the caller resolves the grounding union (FEAT-030) once
    /// per session/turn and passes it in.
    pub grounding: Vec<String>,
    /// `decision.deliberate.confidence_threshold` (default 0.7).
    pub confidence_threshold: f64,
    pub recorder: &'a dyn VoteRecorder,
}

#[async_trait]
impl<'a> SoulGate for LlmSoulGate<'a> {
    async fn evaluate(&self, plan: &Plan) -> Result<(SoulVerdict, String)> {
        let messages = vec![
            ChatMessage::system(SOUL_SYSTEM_PROMPT),
            ChatMessage::user(soul_user_prompt(&plan.intent_summary, &self.grounding)),
        ];
        let raw_text = self.llm.chat_completion(&messages).await?;
        let raw = parse_soul_response(&raw_text)?;

        let vote = SoulVote {
            plan_id: plan.id.clone(),
            confidence: raw.confidence,
            verdict: raw.verdict,
            gut_reaction: raw.gut_reaction.clone(),
        };
        self.recorder.record(&vote);

        let verdict = resolve_verdict(raw.verdict, raw.confidence, self.confidence_threshold);
        Ok((verdict, raw.gut_reaction))
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
        match soul.evaluate(&plan).await? {
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
                id: format!("plan-{n}"),
                intent_summary: format!("round {n}, feedback={revision_feedback:?}"),
                steps: vec!["do the thing".into()],
                intended_tool_calls: vec![],
            })
        }
    }

    struct FixedVerdictSoul(SoulVerdict);
    #[async_trait]
    impl SoulGate for FixedVerdictSoul {
        async fn evaluate(&self, _plan: &Plan) -> Result<(SoulVerdict, String)> {
            Ok((self.0, "canned verdict".to_string()))
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
        #[async_trait]
        impl SoulGate for RevisesOnceSoul {
            async fn evaluate(&self, _plan: &Plan) -> Result<(SoulVerdict, String)> {
                Ok(if self.0.fetch_add(1, Ordering::SeqCst) == 0 {
                    (SoulVerdict::Revise, "narrow the blast radius".to_string())
                } else {
                    (SoulVerdict::Approve, "ok now".to_string())
                })
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

    #[tokio::test]
    async fn passthrough_soul_gate_always_approves() {
        let (verdict, _) = PassthroughSoulGate.evaluate(&Plan::default()).await.unwrap();
        assert_eq!(verdict, SoulVerdict::Approve);
    }

    // --- The real Soul gate (FEAT-029) ---

    use crate::llm::LlmTurn;
    use crate::tools::{FunctionCall, ToolCall as TC};

    /// A spy `LlmClient` that records every message it was sent (for
    /// acceptance criterion 1) and replays a single canned `chat_completion`
    /// reply.
    struct SpyLlm {
        reply: String,
        seen_messages: Mutex<Vec<ChatMessage>>,
    }
    impl SpyLlm {
        fn new(reply: impl Into<String>) -> Self {
            Self { reply: reply.into(), seen_messages: Mutex::new(Vec::new()) }
        }
    }
    #[async_trait]
    impl LlmClient for SpyLlm {
        async fn chat_turn(&self, _messages: &[serde_json::Value], _tools: Option<&[crate::tools::ToolDef]>) -> Result<LlmTurn> {
            unreachable!("the Soul gate uses chat_completion, not chat_turn")
        }
        async fn embedding(&self, _input: &str, _model: &str) -> Result<Vec<f32>> {
            unreachable!("the Soul gate never computes embeddings")
        }
        async fn chat_completion(&self, messages: &[ChatMessage]) -> Result<String> {
            self.seen_messages.lock().unwrap().extend_from_slice(messages);
            Ok(self.reply.clone())
        }
    }

    fn plan_with_reasoning() -> Plan {
        Plan {
            id: "plan-1".into(),
            intent_summary: "restart the web service to clear a memory leak".into(),
            steps: vec!["SECRET_STEP: sudo systemctl restart webapp".into()],
            intended_tool_calls: vec![TC {
                id: "call-1".into(),
                call_type: "function".into(),
                function: FunctionCall { name: "bash".into(), arguments: "{\"command\":\"systemctl restart webapp\"}".into() },
            }],
        }
    }

    #[derive(Default)]
    struct SpyRecorder {
        votes: Mutex<Vec<SoulVote>>,
    }
    impl VoteRecorder for SpyRecorder {
        fn record(&self, vote: &SoulVote) {
            self.votes.lock().unwrap().push(vote.clone());
        }
    }

    #[tokio::test]
    async fn soul_prompt_carries_only_intent_and_values_not_reasoning_tools_or_env() {
        // acceptance criterion 1
        let llm = SpyLlm::new(r#"{"confidence": 0.9, "gut_reaction": "fine", "verdict": "approve"}"#);
        let recorder = SpyRecorder::default();
        let gate = LlmSoulGate {
            llm: &llm,
            grounding: vec!["integrity".to_string(), "caution".to_string()],
            confidence_threshold: 0.7,
            recorder: &recorder,
        };
        gate.evaluate(&plan_with_reasoning()).await.unwrap();

        let seen = llm.seen_messages.lock().unwrap();
        let joined: String = seen.iter().map(|m| m.content.clone()).collect::<Vec<_>>().join("\n");
        assert!(joined.contains("restart the web service to clear a memory leak"), "intent must be present");
        assert!(joined.contains("integrity") && joined.contains("caution"), "values grounding must be present");
        assert!(!joined.contains("SECRET_STEP"), "the Mind's step-by-step reasoning must be withheld");
        assert!(!joined.contains("systemctl restart webapp"), "the intended tool call must be withheld");
        assert!(!joined.contains("bash"), "the tool list must be withheld");
    }

    #[tokio::test]
    async fn approve_at_or_above_threshold_resolves_to_approve() {
        // acceptance criterion 2
        let llm = SpyLlm::new(r#"{"confidence": 0.85, "gut_reaction": "solid", "verdict": "approve"}"#);
        let recorder = SpyRecorder::default();
        let gate = LlmSoulGate { llm: &llm, grounding: vec![], confidence_threshold: 0.7, recorder: &recorder };
        let (verdict, _) = gate.evaluate(&plan_with_reasoning()).await.unwrap();
        assert_eq!(verdict, SoulVerdict::Approve);
    }

    #[tokio::test]
    async fn approve_below_threshold_resolves_to_revise() {
        // acceptance criterion 2
        let llm = SpyLlm::new(r#"{"confidence": 0.4, "gut_reaction": "not sure", "verdict": "approve"}"#);
        let recorder = SpyRecorder::default();
        let gate = LlmSoulGate { llm: &llm, grounding: vec![], confidence_threshold: 0.7, recorder: &recorder };
        let (verdict, _) = gate.evaluate(&plan_with_reasoning()).await.unwrap();
        assert_eq!(verdict, SoulVerdict::Revise);
    }

    #[tokio::test]
    async fn veto_fails_the_gate_regardless_of_confidence() {
        // acceptance criterion 3
        let llm = SpyLlm::new(r#"{"confidence": 0.99, "gut_reaction": "no", "verdict": "veto"}"#);
        let recorder = SpyRecorder::default();
        let gate = LlmSoulGate { llm: &llm, grounding: vec![], confidence_threshold: 0.7, recorder: &recorder };
        let (verdict, reason) = gate.evaluate(&plan_with_reasoning()).await.unwrap();
        assert_eq!(verdict, SoulVerdict::Veto);
        assert_eq!(reason, "no");
    }

    #[tokio::test]
    async fn veto_through_run_deliberate_denies_and_escalates_without_executing() {
        // acceptance criterion 3, end to end through the pipeline
        let llm = SpyLlm::new(r#"{"confidence": 0.99, "gut_reaction": "against stewardship", "verdict": "veto"}"#);
        let recorder = SpyRecorder::default();
        let gate = LlmSoulGate { llm: &llm, grounding: vec!["stewardship".to_string()], confidence_threshold: 0.7, recorder: &recorder };
        let planner = FixedPlanner::new();
        let mut executor = SpyExecutor::default();

        let outcome = run_deliberate(&planner, &gate, &mut executor, 3).await.unwrap();
        match outcome {
            DeliberateOutcome::DeniedAndEscalated { reason } => assert_eq!(reason, "against stewardship"),
            other => panic!("expected DeniedAndEscalated, got {other:?}"),
        }
        assert!(executor.executed.lock().unwrap().is_empty());
    }

    #[tokio::test]
    async fn max_rounds_without_approval_denies_and_escalates_with_a_scripted_llm() {
        // acceptance criterion 4, with a fake LLM that never approves
        struct AlwaysRevise;
        #[async_trait]
        impl LlmClient for AlwaysRevise {
            async fn chat_turn(&self, _m: &[serde_json::Value], _t: Option<&[crate::tools::ToolDef]>) -> Result<LlmTurn> {
                unreachable!()
            }
            async fn embedding(&self, _i: &str, _m: &str) -> Result<Vec<f32>> {
                unreachable!()
            }
            async fn chat_completion(&self, _messages: &[ChatMessage]) -> Result<String> {
                Ok(r#"{"confidence": 0.9, "gut_reaction": "reconsider", "verdict": "revise"}"#.to_string())
            }
        }
        let llm = AlwaysRevise;
        let recorder = SpyRecorder::default();
        let gate = LlmSoulGate { llm: &llm, grounding: vec![], confidence_threshold: 0.7, recorder: &recorder };
        let planner = FixedPlanner::new();
        let mut executor = SpyExecutor::default();

        let outcome = run_deliberate(&planner, &gate, &mut executor, 3).await.unwrap();
        assert!(matches!(outcome, DeliberateOutcome::DeniedAndEscalated { .. }));
        assert_eq!(planner.calls.load(Ordering::SeqCst), 3);
        assert_eq!(recorder.votes.lock().unwrap().len(), 3, "every round's vote was captured");
    }

    #[tokio::test]
    async fn every_vote_is_captured_with_plan_id_confidence_verdict_and_reaction() {
        // acceptance criterion 5
        let llm = SpyLlm::new(r#"{"confidence": 0.55, "gut_reaction": "borderline", "verdict": "approve"}"#);
        let recorder = SpyRecorder::default();
        let gate = LlmSoulGate { llm: &llm, grounding: vec![], confidence_threshold: 0.7, recorder: &recorder };
        gate.evaluate(&plan_with_reasoning()).await.unwrap();

        let votes = recorder.votes.lock().unwrap();
        assert_eq!(votes.len(), 1);
        assert_eq!(votes[0].plan_id, "plan-1");
        assert_eq!(votes[0].confidence, 0.55);
        assert_eq!(votes[0].verdict, RawSoulVerdict::Approve);
        assert_eq!(votes[0].gut_reaction, "borderline");
    }

    #[tokio::test]
    async fn tolerates_prose_wrapped_around_the_json_object() {
        let llm = SpyLlm::new("Sure, here you go:\n{\"confidence\": 0.8, \"gut_reaction\": \"ok\", \"verdict\": \"approve\"}\nHope that helps!");
        let recorder = SpyRecorder::default();
        let gate = LlmSoulGate { llm: &llm, grounding: vec![], confidence_threshold: 0.7, recorder: &recorder };
        let (verdict, _) = gate.evaluate(&plan_with_reasoning()).await.unwrap();
        assert_eq!(verdict, SoulVerdict::Approve);
    }

    #[tokio::test]
    async fn malformed_response_is_an_error_not_a_silent_approve() {
        let llm = SpyLlm::new("not json at all");
        let recorder = SpyRecorder::default();
        let gate = LlmSoulGate { llm: &llm, grounding: vec![], confidence_threshold: 0.7, recorder: &recorder };
        assert!(gate.evaluate(&plan_with_reasoning()).await.is_err());
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
