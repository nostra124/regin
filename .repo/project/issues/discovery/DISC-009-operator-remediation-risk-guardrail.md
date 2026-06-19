---
id: DISC-009
type: discovery
priority: high
status: open
complexity: L
spawned_features: ~
---

# DISC-009 — Operator remediation and the three-lane risk guardrail

## Operating-plane context

Operator plane (see DISC-008). Concerns how regin *acts* on the machine to close
deviations, and how it decides what it may do autonomously.

## Describe

Today operator skills only **report**; nothing closes the loop. Per the model,
the loop is: deviation → **incident** (always; incidents are by-definition
solvable) → the fix is almost always a **change applied directly to the
incident** (worked example: `/` at 95% → change "delete temp files" → resolved).
A **problem** is the exception, not a mandatory middle step — it is raised only on
recurrence (DISC-011) or when the fix is beyond regin.

The crux is a per-change **risk judgement** that routes every candidate fix into
one of three lanes — this *is* the autonomy guardrail:

| regin's judgement on the fix | Action |
|---|---|
| safe + reversible (delete temp files) | **auto-apply** the change |
| uncertain / destructive / wide blast radius (edit logging config) | change → **`pending_approval`**; get approval *before* apply |
| out of regin's control entirely | **don't attempt** → open a problem + escalate |

Approval/escalation is routed by runtime mode (DISC-010): supervisor over the bus
in an org, parked for the human-at-login greeting when standalone.

Worked example (chronic): same disk incident daily → problem "disk fills nightly"
→ analyse → change "adjust log rotation". Implications uncertain → `pending_approval`
→ human/supervisor approves → apply.

## Variants considered (how the lane is decided)

| Variant | Summary | Key trade-off |
|---|---|---|
| A | Static allow/deny list of actions (enumerate "safe") | Predictable, auditable; brittle, can't cover novel fixes |
| B | LLM judgement at runtime against a policy prompt | Flexible, handles novel fixes; less predictable, needs guardrails |
| C | Declarative per-skill/per-action risk tags + capability ceiling | Structured authorization; upfront modelling, still needs judgement for novel cases |
| H | Hybrid: capability ceiling (authorization floor) + risk tags/allowlist (fast path) + LLM judgement within bounds | Best coverage; most moving parts |

## Decision matrix

| Criterion | Weight | A | B | C | H |
|---|---|---|---|---|---|
| Safety / predictability | high | ✓ | ✗ | ✓ | ✓ |
| Handles novel remediations | high | ✗ | ✓ | ~ | ✓ |
| Auditability | high | ✓ | ~ | ✓ | ✓ |
| Default-posture tunable (conservative ↔ autonomous) | med | ~ | ✓ | ~ | ✓ |

**Leaning:** Variant **H** (hybrid) — a capability ceiling as the authorization
floor, declarative "safe action" fast-path, LLM judgement for everything else,
with a tunable default posture (how much regin may auto-apply vs. always ask).

## Open questions (resolving with user)

1. Default posture out of the box: conservative (approve *everything* until
   trusted) vs. trust-the-safe-lane (auto-apply reversible fixes)?
2. What concretely defines "safe/reversible" vs "destructive"? (allowlist? dry-run
   capability? reversibility check?)
3. Does the capability ceiling from the persona model apply to the operator role
   too, or does the operator get its own ceiling?

## Decision

_Pending — being resolved with the user (guided Q&A)._

## Spawned features

_Pending DISC close._
