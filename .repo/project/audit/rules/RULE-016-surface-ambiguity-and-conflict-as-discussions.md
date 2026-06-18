# RULE-016 — Surface ambiguity and conflict as discussions; don't guess

scope: full
severity: warn

## Rule

At every workflow step, before executing, the agent checks its planned work
against the project's stated **principles** (`profile.md`), the milestone
**intent**, and the ticket. If anything is **unclear, underspecified, or in
conflict** with a principle or a prior decision, the agent **surfaces it as a
discussion** — files a DISC ticket (or blocks and raises the question) — instead
of resolving it with a silent assumption and moving on.

This is the guard against slow drift: a long run of locally-reasonable steps
that, unchecked against the stated identity, accumulate into a large discrepancy
nobody chose (see DISC-016 — the dev-centric CLI vs the generic-engine
principle). One step asking "does this fit what we said we are building?" turns
a late, expensive UAT finding into an early, cheap design conversation.

Applies to all session types, and most strongly to **design** and **discovery**
steps and to any step that adds **user-facing surface** (a CLI verb, a public
API, a concept).

## Pass criteria

- Per-step agent instructions (session-type instructions, dwarf roles, design/
  discovery skills) explicitly direct the agent to raise unclear/conflicting
  requirements as a DISC rather than assume.
- Where a real ambiguity or principle-conflict existed during a milestone, a DISC
  (or a recorded blocking question) exists for it — it was not silently resolved.

## Fail criteria

- A step resolves an unclear or conflicting requirement by assumption, with no
  DISC and no raised question, and the assumption later proves wrong (a UAT
  discrepancy with no design trail).
- New user-facing surface (verbs, APIs, concepts) is added without a check
  against the stated principles/identity.

## Audit instruction

1. Confirm the per-step instructions carry the "surface ambiguity/conflict as a
   discussion, don't guess" directive. Report PASS/FAIL.
2. For the milestone under audit, look for design decisions that were made
   silently where the ticket/profile was ambiguous or where a principle was in
   tension. Each should have a DISC or a recorded question; flag any that don't.
3. For each new user-facing surface added, verify it was checked against the
   project identity/principles (a note, a DISC, or a workshop).
