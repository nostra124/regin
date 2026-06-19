---
id: FEAT-057
type: feature
priority: medium
complexity: S
estimate_tokens: 30k-60k
estimate_time: 45-90min
phase: open
status: open
milestone: 0.5.0
depends_on: FEAT-019
---

# FEAT-057 — GitHub wiki landing page

## Description
**As a** prospective user
**I want** a landing/onboarding page
**So that** I can understand what regin is and get started quickly.

## Implementation
- A GitHub wiki landing page covering:
  - what regin is (LLM-backed Linux operations agent) and its two modes — **A** dvalin
    foreman, **B** standalone operator;
  - **install** (the FEAT-056 script + per-format packages);
  - a **quickstart** (`regin chat`, run a skill, the operator loop in brief);
  - a **command overview** linking to the generated man pages (FEAT-019) and the
    project profile.
- Kept consistent with the actual clap surface (RULE-012 documentation coverage); no
  drift from retired verbs.

## Acceptance Criteria
1. The landing page exists with what-it-is, install, quickstart, and command-overview
   sections.
2. Commands shown match the current clap surface; links to man pages / profile resolve.
3. Install instructions reference the FEAT-056 script and the published packages.
