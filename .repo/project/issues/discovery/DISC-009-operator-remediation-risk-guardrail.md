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

## Decision (resolved with user — guided Q&A 2026-06-19)

**Q1 — Lane mechanism: Hybrid (H).** A **capability ceiling** (hard authorization
floor) + a declarative **safe-action fast-path** (pre-blessed reversible ops go
straight to auto-apply) + **LLM judgement** for everything else, bounded by the
ceiling. Routes novel fixes while staying safe and auditable.

**Q2 — Default posture: adaptive (earn autonomy).** Out of the box regin starts
conservative (most fixes → `pending_approval`); the safe-lane **graduates to
auto-apply as the change-success-rate / autonomy KPIs prove trust** — the same
earn-trust-with-evidence pattern as DISC-015's promotion loop, governed by the same
KPI store.

**Q3 — Safe-lane gate: rollback plan + dry-run.** A fix qualifies for auto-apply
only if regin can **capture a concrete backout/undo *before* applying** (snapshot,
backup, or inherently reversible op) with bounded blast radius, **and runs a dry-run
where the op supports one**. The allowlist is just a fast-path of pre-blessed
reversible ops. (ITIL change-management backout-plan discipline.)

**Q4 — Ceiling: own operator ceiling + minimal global red-lines.** The **operator
role has its own capability ceiling** doing all day-to-day authorization. Above it
sits a **tiny, static, non-runtime-adjustable global red-line set** that no role may
ever cross:
- **Protect the safety substrate** — never delete backups/snapshots, never tamper
  with/disable the audit log, never erase the KPI store. *(Directly required by Q3:
  the safe-lane depends on rollback/audit data existing.)*
- **Don't sever governance** — never break its own service so it can't be
  stopped/recovered, never cut the escalation channel (supervisor bus / notification
  egress, DISC-010), never disable the human kill-switch.
- **No catastrophic host actions** — `rm -rf /`, wipe the data dir, `dd`/repartition
  a disk, rewrite `/etc/shadow` or add a root-equivalent user, disable the firewall
  wholesale.

Rationale: the operator ceiling is editable policy and regin ingests logs (a
**prompt-injection surface**), so the global layer is defense-in-depth against the
operator ceiling being misconfigured *or* talked into widening itself at runtime —
a constitutional limit vs. a statutory one.

## Spawned features (to derive on close)

- **Three-lane remediation engine** — deviation → incident → candidate fix → lane
  routing (auto-apply / `pending_approval` / don't-attempt→problem+escalate); closes
  the loop that today only reports.
- **Capability ceiling + global red-lines** — operator-role ceiling (editable) under
  a static, non-runtime-adjustable global red-line set; every action checked against
  both; clear "which layer denied this" audit messages.
- **Safe-lane gate** — backout/undo capture (snapshot/backup/reversible-op detection)
  + dry-run runner + blast-radius bound; a change without a rollback plan can never be
  auto-applied.
- **Adaptive posture** — conservative default; safe-lane auto-apply graduates on
  change-success-rate / autonomy KPIs (shared with DISC-015); posture is tunable.
- **Approval routing** — `pending_approval` changes routed by runtime mode (DISC-010):
  supervisor over the bus (org) / human-at-login greeting (standalone).
