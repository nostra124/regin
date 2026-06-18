# Discovery track

> The dual-track upstream phase to feature delivery. Captures
> problem framing and solution ideation before a FEAT is filed.

A discovery ticket (`DISC-NNN` at
`issues/discovery/<phase>/<NNN>-<slug>.md`) is the artefact. Three
phases: `describe → ideate → done`. During `ideate`, FEATs are
**minted** — written into `issues/feature/<phase>/<NNN>-*.md` and
linked back to the DISC via `project discover --link`. The DISC
then closes and the spawned FEATs enter the standard delivery
loop.

The two tracks run in **parallel**:

- Discovery is **milestone-independent**. A DISC never appears
  in any `MILESTONE-<x>.<y>.<z>.md`.
- A spawned FEAT is filed in `issues/feature/` as an open ticket;
  it is added to the next milestone's `tickets:` list during milestone
  planning. There is no intermediate `BACKLOG.md` — tickets live in
  `issues/feature/` until they are assigned to a milestone.
- A DISC may take days while several milestones come and go;
  conversely, a single milestone may pull in FEATs spawned by
  multiple DISCs.

`spawned_from: DISC-NNN` on a FEAT is **optional**. Features that
predate the discovery track and small straightforward FEATs don't
need a discovery parent; the field exists for the cases where you
want the WHY preserved end-to-end. The traceability audit
(`operations/audit.md` § 3) verifies that present `spawned_from`
references resolve.

## Capturing the why — mandatory

A DISC ticket must document not just the decision but the reasoning
that led to it. Every closed DISC must contain:

1. **Variants considered** — every option that was on the table,
   including rejected ones. Future readers need to know what was
   ruled out and why, so they don't re-evaluate the same ground.

2. **Decision matrix** — criteria × variants table. Weight each
   criterion (high / medium / low). This makes the trade-offs
   explicit and auditable.

3. **Arguments** — pro/con list for the chosen approach.
   Acknowledged risks belong here; hiding them is a process
   violation.

4. **Decision rationale** — one paragraph explaining which
   constraints were decisive and which trade-offs were consciously
   accepted. "We chose X" is not a rationale. "We chose X over Y
   because constraint Z was non-negotiable; we accepted trade-off W"
   is a rationale.

This discipline is enforced by the `ideate → done` graduation gate
(see `convention/tickets.md`). A dwarf facilitating a workshop is
responsible for writing all four sections before closing the DISC.

## Workshop execution model

Discovery workshops are interactive sessions between the user and a
dwarf (coding agent). **dvalin itself contains no LLM** — it is
entirely rule- and workflow-based. When a workshop is needed:

1. dvalin detects open DISC tickets (`GateResult::DiscPending`).
2. dvalin prints the workshop prompt and estimated duration.
3. The user spins up a dwarf (via `dvalin dev supervise` or manually).
4. dvalin injects the workshop context into the dwarf's session.
5. The dwarf facilitates: asks questions, builds the decision matrix,
   surfaces trade-offs, and writes the DISC outputs to disk.
6. The session ends when the DISC has all required sections and
   `status: done`.

Full protocol in `issues/discovery.md`; frontmatter schema in
`convention/tickets.md` → "Discovery ticket".

## Design principles the workshop must apply

A workshop's job is to get the design **correct from the beginning** — the
cheapest place to fix a system is before it is built. The facilitating dwarf
holds the design against these principles and surfaces any conflict as an open
question (RULE-016), never silently:

1. **Correctness by construction, not by diagnosis.** Handle each failure at the
   layer that owns its precondition, enforced so it cannot be skipped: declare
   real **package dependencies** (the package manager enforces them), make
   daemons **fail fast and loud** on a missing precondition (clear log, non-zero
   exit, and they *stay* failed), give **clear point-of-use errors**, and back it
   all with **proper tests** (including install/integration tests). Reject
   `doctor`/health-check commands and other after-the-fact scaffolding — nobody
   runs them and they hide sloppy construction.

2. **Self-healing only on stable ground.** Self-healing and supervision are
   legitimate capabilities (they are much of dvalin's reason to exist) — but only
   as a layer built **on top of** a correct, predictable foundation. Designing
   recovery *into* the foundation, as a substitute for getting the base right,
   produces unpredictable systems whose behaviour you can no longer reason about
   because they are always compensating. **No auto-healing or silent retry at the
   foundation layer.** Get the base correct-by-construction first; add
   self-healing deliberately, and only above stable ground.

3. **A design that needs a band-aid is not done.** If the proposed design relies
   on a diagnostic command, an auto-retry, or "we'll detect and recover" to be
   usable, treat that as a signal the foundation is wrong and reopen it — do not
   ratify it.
## Closing a discovery derives its features (mandatory)

A discovery is not finished when the decision is written — it is finished when the
work it implies exists as tickets. **On closing a DISC (`status: done`), derive
its features automatically:**

1. Create the FEAT tickets the decision spawns, each carrying `spawned_from:
   DISC-NNN`, in dependency order.
2. Record them in the DISC's `spawned_features: [...]` (the reciprocal link).
3. Assign them to a milestone plan file (`MILESTONE-<x>.<y>.<z>.md`) — sequenced
   per the DISC's implementation notes; slice large/entangled items rather than
   filing one oversized FEAT.

A closed DISC with an empty `spawned_features` and no milestone is incomplete:
the discussion happened but the work was never made actionable. This is the
standard hand-off from the creative/design pole to the predictable/build pole.
