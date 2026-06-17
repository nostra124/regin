---
name: feature-design
description: |
  Design phase for feature tickets. Fills the ## Design section,
  sets complexity and estimates. Trigger when a FEAT in
  phase: open is pulled into a milestone, or when about to
  start implementation on an unspecified FEAT.
---

# Feature design phase

> **RULE-016 — surface ambiguity, don't guess.** Design is where drift is
> cheapest to catch. Check the ticket against the project principles
> (`profile.md` §1/§1a) and the milestone intent. If a requirement is unclear,
> underspecified, or conflicts with a stated principle — above all when adding a
> CLI verb, an API, or a concept ("does this fit what we said we are
> building?") — **file a DISC and resolve it in a workshop before designing**,
> rather than resolving it with a silent assumption. A late UAT discrepancy
> (e.g. DISC-014: a dev-centric CLI vs a generic-engine principle) is a design
> question that was never raised.

## 1. What this phase produces

Design phase for features fills these into the ticket:

- **`## Design` section** — approach, interface sketch, open questions resolved
- **Complexity** — T-shirt size (XS/S/M/L/XL)
- **estimate_tokens** — range in thousands
- **estimate_time** — wall-clock estimate

Output: ticket updated with `phase: design` and all sizing fields populated.

## 2. Entry conditions

Enter Design when:

- The FEAT has passed Discovery (AS-A + AC defined)
- A milestone plan includes this ticket and implementation will start soon
- The ticket is "in-flight" but lacks `## Design` content

## 3. Design flow

```
┌─────────────┐     ┌─────────────┐     ┌─────────────┐
│  Read AC    │ →   │  Sketch     │ →   │  Size +      │
│  & AS-A     │     │  approach   │     │  estimate    │
└─────────────┘     └─────────────┘     └─────────────┘
```

### 3.1 Analyze scope

From the AS-A description and AC:
- Which files/modules does this touch?
- Is it additive (new surface) or modifying (existing surface)?
- Does it require external integration (SIT/PIT coverage)?

### 3.2 Sketch the approach

Fill `## Design` with:

```
## Design

### Approach
<one paragraph: the high-level strategy>

### Interface
<function signatures, CLI flags, file paths — concrete enough
to be critiqued before implementation>

### Dependencies
<what must land before this can build>

### Open questions
<resolved questions; empty if none remain>
```

The Design section is **not** full implementation — it's enough detail
that another agent could pick it up and build without guessing.

### 3.3 Set complexity

Use the rubric:

| Size | Scope | Tokens | Time |
|------|-------|--------|------|
| XS | doc tweak, one-line fix, regex sub | 3-10k | 2-10min |
| S | small feature, 1 file, 1-2 fns, ~3 tests | 10-30k | 15-30min |
| M | typical feature, 2-5 files, 5-10 tests | 30-80k | 30-90min |
| L | substantial — new subsystem, 10+ files | 80-200k | 1.5-3hr |
| XL | needs breakdown into smaller tickets | >200k | >3hr |

**XL is a smell** — break it into smaller FEATs before Build.
The sizing cap is enforced socially, not mechanically.

### 3.4 Token and time estimates

Token estimates are **ranges**. Point estimates have ±50% variance.
Calibration happens over time as `project effort` records actuals.

```
complexity: M
estimate_tokens: 30-80k
estimate_time: 30-90min
```

## 4. Exit criteria

Design is complete when:

1. `## Design` section is non-empty with concrete approach
2. Complexity is set (XS/S/M/L)
3. `estimate_tokens` and `estimate_time` are filled
4. No open questions that block Build

Then transition to Build:

```
project transition <id> build
```

## 5. Design vs. Discovery

| Aspect | Discovery | Design |
|--------|-----------|--------|
| What | AS-A, AC, priority | Approach, sizing, estimates |
| When | Filing | Before Build |
| Who | Filer (user or agent) | Agent starting the work |
| Output | `phase: open` | `phase: design` |

## 6. Cross-references

- Previous phase: `issues/feature/discovery.md`
- Next phase: `issues/feature/build.md`
- Sizing calibration: `operations/milestone.md` § 2
- Multi-ticket Design sessions: `operations/milestone.md` § 2.3