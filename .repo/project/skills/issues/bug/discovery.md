---
name: bug-discovery
description: |
  Bug discovery phase — reproduce and document the defect.
  Trigger when CI red on a PR, when a user reports a defect,
  when a code review flags broken behaviour, or when
  investigating a regression.
---

# Bug discovery phase

## 1. What this phase produces

Discovery (reproduction) for bugs captures:

- **Steps to reproduce** — deterministic commands
- **Expected behaviour** — the contract being violated
- **Actual behaviour** — what the user/CI observed
- **Root cause hypothesis** — initial diagnosis (refined in design if needed)

Output: `issues/bug/<NNN>-<slug>.md` with `phase: open`, reproduction documented.

## 2. Bug lifecycle (shorter than features)

Bugs follow a **shorter** V-model — there is **no Design phase**:

```
┌──────┐   ┌───────┐   ┌──────┐   ┌──────┐
│ Open │ → │ Build │ → │ Test │ → │ Done │
└──────┘   └───────┘   └──────┘   └──────┘
```

| Phase | Output |
|-------|--------|
| **Open** | bug ticket filed; reproduction documented |
| **Build** | failing unit test committed, then the fix committed (red → green per §4) |
| **Test** | CI green; SIT (and PIT if scoped) green |
| **Done** | PR merged, ticket moved to `issues/bug/done/` |

## 3. Entry conditions

Enter Discovery when:

- CI goes red on a PR
- A user reports a defect
- A code review flags broken behaviour
- Investigating a regression

## 4. Discovery flow

```
┌────────────┐   ┌────────────┐   ┌────────────┐   ┌────────────┐
│  Observe   │ → │  Isolate   │ → │  File BUG  │ → │   Document │
│  failure   │   │  cause     │   │            │   │  steps     │
└────────────┘   └────────────┘   └────────────┘   └────────────┘
```

### 4.1 Observe the failure

From CI PR comment, identify the failing test:

- The test name (or assertion that fired)
- The file under `tests/unit/` and the line number
- The expected vs actual output

The concrete failure-line format depends on the unit test
runner — see `.repo/project/skills/language/<lang>.md` for the
syntax used in this project.

From user report: log output, command, environment.

### 4.2 Isolate locally

```bash
make check-unit
```

If the test passes locally, investigate environment differences.
If it fails locally, proceed to root cause.

### 4.3 File the bug

```
issues/bug/<NNN>-<slug>.md
```

### 4.4 Document reproduction

The ticket must contain:

```markdown
## Steps to reproduce

1. `<exact command>`
2. `<exact command>`
3. ...

## Expected behaviour

<the contract this violates — cite test pin, help text, or prior issue>

## Actual behaviour

<what actually happens — paste log output>
```

A future reader must be able to reproduce from this description alone.

## 5. Ticket shape

```yaml
---
id: BUG-NNN
type: bug
priority: low|medium|high|critical
complexity: XS|S|M|L|XL
estimate_tokens: 10k-30k
estimate_time: 15-30min
phase: open
status: open
---

# <Title>

## Description

<what the user / CI / agent observed>

## Steps to reproduce

<numbered commands, deterministic>

## Expected behaviour

<the contract violated>

## Actual behaviour

<log output, error message>
```

## 6. Sizing

Sizing happens at filing time (no Design phase to refine it).

Use the same rubric as features, but bug sizing is typically:

| Complexity | Scope |
|------------|-------|
| XS | one-line fix, typo, missing flag |
| S | single function, localised break |
| M | cross-function, one module |
| L | multi-module, API contract fix |
| XL | needs discussion → consider filing as DISC |

## 7. Design-issue escape hatch

If a bug investigation surfaces a **design issue** (the contract being violated is itself wrong, or the fix would require restructuring the surface), the bug **does not transition into Design**. Instead:

1. Stop the bug investigation. Note the design problem in `## Fix`.
2. **File a new DISC** capturing the design issue (`issues/discovery.md`).
3. Either keep the bug open (waiting for the new FEAT), or close it with a `## Resolution` noting the supersession.

Bugs are for *implementation defects against an existing contract*. Contract changes are features, not bugs.

## 8. Cross-references

- Next phase: `issues/bug/build.md`
- Design-issue escape: `issues/discovery.md`
- If root cause unclear: `issues/bug/design.md` (optional, for complex bugs)
- CI failure flow: `issues/bug/build.md` § 5