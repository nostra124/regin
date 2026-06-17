---
name: bug-design
description: |
  Root cause analysis for complex bugs. Optional phase
  — most bugs skip directly to Build. Use when the defect
  is not immediately diagnosable from the reproduction.
---

# Bug design phase

## 1. When to use this phase

**Most bugs skip this phase.** Use only when:

- Root cause is not immediately obvious from reproduction
- Multiple systems interact in unclear ways
- The fix might require API/contract changes
- Investigation shows deeper design issues

Simple bugs (typo, off-by-one, missing null check) go directly to Build.

## 2. What this phase produces

Design for bugs fills these into the ticket:

- **Root cause** — definitive diagnosis of why the behaviour fails
- **Fix approach** — sketch of the fix (not the implementation)
- **Scope estimate** — refined complexity assessment

Output: ticket updated with `## Fix` section containing root cause and approach.

## 3. Design flow

```
┌────────────┐   ┌────────────┐   ┌────────────┐
│  Diagnose  │ → │   Verify   │ → │   Sketch   │
│  root cause│   │   hypothesis│   │   fix     │
└────────────┘   └────────────┘   └────────────┘
```

### 3.1 Diagnose

From the reproduction, isolate the failing code path. Use
the language's tracing or debug-logging facility (e.g.
`bash -x` for shell, `RUST_LOG=debug` for rust, `python -X
dev` for python, `node --inspect` for node — see
`.repo/project/skills/language/<lang>.md`) plus the project's
`SELF_DEBUG=1` toggle to surface internal state.

Identify:
- Which function/module is failing?
- What input triggers it?
- What assumption is violated?

### 3.2 Verify hypothesis

Before committing to a fix:

1. Write a minimal reproduction script.
2. Modify the suspected code temporarily.
3. Confirm the reproduction passes.
4. Revert the temporary change.

This validates you found the root cause.

### 3.3 Sketch fix approach

In `## Fix`:

```markdown
## Fix

### Root cause
<definitive diagnosis>

### Approach
<one paragraph: what to change, where>

### Risk
<what else might break>
```

## 4. Design-issue detection

During diagnosis, if you find:

| Finding | Action |
|---------|--------|
| Contract is ambiguous | File FEAT for clarification |
| Contract is wrong | File DISC for redesign |
| External dependency changed | File BUG with workaround, FEAT for proper fix |
| Performance issue | File FEAT for optimization |

Bugs are for *implementation defects*. Design issues become features.

## 5. Exit criteria

Design (root cause analysis) is complete when:

1. Root cause is definitively identified
2. Fix approach is sketched
3. Risk assessment documented
4. Complexity/estimate refined if needed

Then transition to Build:

```
project transition <id> build
```

## 6. For simple bugs

If root cause is obvious from reproduction:

- Skip this phase entirely
- Go directly to Build
- Ticket goes from `phase: open` → `phase: build`

The `## Fix` section in the ticket is optional for simple fixes.

## 7. Cross-references

- Previous phase: `issues/bug/discovery.md`
- Next phase: `issues/bug/build.md`
- Design issues → features: `issues/discovery.md`
- Complexity rubric: `issues/feature/design.md` § 3