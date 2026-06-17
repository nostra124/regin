---
name: feature-discovery
description: |
  Initial discovery phase for feature tickets. When a feature
  idea surfaces but isn't yet specified, this phase captures the
  AS-A description, acceptance criteria, and initial scope
  before Design. Trigger when filing a new FEAT or when scoping
  an open FEAT that lacks AC.
---

# Feature discovery phase

## 1. What this phase produces

Discovery phase for features fills these into the ticket:

- **AS-A description** — user story format
- **Acceptance Criteria** — verifiable conditions
- **Initial scope** — preliminary complexity estimate (refined in Design)
- **Priority** — low/medium/high/critical
- **Frontmatter** — id, type, status, phase

Output: `issues/feature/<NNN>-<kebab-slug>.md` with `phase: open`.

## 2. Entry conditions

Discovery is the **first phase** for every FEAT. Enter when:

- A user or agent identifies a new piece of functionality
- A DISC (`issues/discovery.md`) spawns one or more FEATs
- A planning session breaks down a larger goal into concrete tickets

## 3. Discovery flow

```
idea / request
     │
     ▼
┌─────────────┐     ┌─────────────┐     ┌─────────────┐
│ File ticket │ →   │ Fill AS-A   │ →   │   Define AC │
│ phase: open │     │ description │     │   & priority │
└─────────────┘     └─────────────┘     └─────────────┘
```

### 3.1 Filing the ticket

```
issues/feature/<NNN>-<slug>.md
```

The file name follows kebab-case. Use `project issue new feature <title>`
if available, or create manually with sequential numbering.

### 3.2 AS-A description

```
## Description

**As a** <role>
**I want** <thing>
**So that** <outcome>

<one or two paragraphs of context>
```

The user story format ensures we understand *who* benefits and *why*.

### 3.3 Acceptance Criteria

```
## Acceptance Criteria

1. <verifiable criterion>
2. <verifiable criterion>
3. ...
```

Each criterion must be:
- **Testable** — can write a unit/SIT test for it
- **Unambiguous** — pass/fail is binary
- **Minimal** — no gold-plating; just what's needed

### 3.4 Priority

| Priority | When to use |
|----------|-------------|
| critical | Blocks other work or breaks production |
| high | Needed this milestone |
| medium | Valuable, can wait a milestone |
| low | Nice-to-have, no timeline pressure |

## 4. Exit criteria

Discovery is complete when:

1. AS-A description is filled
2. At least 2 acceptance criteria defined
3. Priority is set
4. The ticket exists under `issues/feature/` with `phase: open`

Then transition to Design:

```
project transition <id> design
```

## 5. Size estimation (optional at discovery)

Discovery *may* include a preliminary complexity guess,
but **Design phase refines it**. The sizing rubric:

| Size | Scope | Tokens | Time (LLM wall-time) |
|------|-------|--------|----------------------|
| XS | doc tweak, one-line fix | 3-10k | 2-10min |
| S | small feature, 1 file, 1-2 fns | 10-30k | 15-30min |
| M | typical feature, 2-5 files | 30-80k | 30-90min |
| L | substantial, 10+ files | 80-200k | 1.5-3hr |
| XL | needs breakdown | >200k | >3hr |

If an estimate is set at Discovery, mark it preliminary;
Design phase owns the final sizing.

## 6. Cross-references

- Next phase: `issues/feature/design.md`
- From DISC: `issues/discovery.md` § 5b
- Sizing rubric details: `issues/feature/design.md` § 2
- Ticket shape: `.repo/project/skills/convention/tickets.md` → "Feature ticket"