---
id: DISC-005
type: discovery
priority: medium
status: open
complexity: L
spawned_features: ~
---

# DISC-005 — Role personas: configuring a regin to *be* a CEO / dev-lead / worker

> regin-side counterpart of dvalin **DISC-030**. dvalin defines the org role model
> and capability vocabulary; this DISC explores how a **regin instance** is
> configured and skilled to embody a given role.

## Describe

dvalin will run a standing organization of named roles — executive (CEO, CFO,
CIO/CTO) and delivery (per-repo development lead, marketing lead, support lead),
plus the worker tier. Most non-worker roles are realized as **regin agents**.
The same regin binary must be able to *become* any of these roles purely by
**configuration + capability (= tool) scoping + persona/skills**, without
forking the codebase.

This DISC explores the regin side: what a **role persona** consists of, how it
maps onto regin's existing primitives (settings in SQLite, skills, memory, tools),
and how capability scoping is enforced so a role only wields its allowed tools.

## Positioning: regin is a blue-collar delivery worker

regin's character is **discipline + autonomy within a domain** — the
**blue-collar** delivery worker (and cave foreman). It is deliberately *not* a
maximal-capability generalist; its strength is doing a bounded job reliably,
permanently, and to process. Two consequences:

- **Self-extension via sudo.** regin has sudo in its cave, so a persona is *not*
  limited to a fixed tool list — regin can **build/install any tool its job
  needs**. Capability scoping is therefore an **authorization ceiling per role**
  (what it's *allowed* to wield), not a packaging limit; within the ceiling regin
  bootstraps whatever it requires. This squares with the earlier "no special
  restrictions unless via user authorization" decision — here the role profile
  *is* that authorization.
- **Complement, not overlap, with raven (white-collar).** A parallel agent,
  **raven**, is the **white-collar** worker runtime — a desktop companion with
  much richer capabilities out of the box (e.g. Playwright **browser**
  automation). White-collar / knowledge-work-heavy or browser/desktop-driven
  roles target the **raven** runtime; disciplined domain delivery targets
  **regin**. This DISC scopes the regin (blue-collar) personas; raven personas
  are dvalin DISC-030's concern. The role→runtime split keeps regin focused.

## To explore / decide

- **Persona definition** — what makes a regin a "dev-lead for repo X" vs a "CFO":
  a role profile (identity/address `role@cave`, baseline rules, skill set, memory
  scope, reporting lines, default channels) — likely a regin config object +
  role-specific skills.
- **Capability = tool scoping** — map dvalin's capability vocabulary onto regin's
  concrete tools (command-exec, file r/w, web, messaging, repo ops, release
  actions). A role's capability set is the **ceiling**; regin must enforce it
  (e.g. a CFO persona cannot push to a repo). How regin restricts its own tool
  surface per role (ties to the earlier "no special restrictions unless via user
  authorization" decision — here authorization is the role profile).
- **Skills per role** — which skills each persona ships with (an exec gets
  reporting/prioritization/approval skills; a dev-lead gets triage/decompose/
  review/escalate skills; a worker tier is the CLI agents the dev-lead supervises).
- **Authority** — what a persona may approve, expressed so dvalin can treat it as
  a workflow gate (e.g. dev-lead approves a PR; CIO approves a release).
- **Memory scope** — per-role memory (a dev-lead's memory is repo-scoped; an exec's
  is org-scoped) and how the Hermes self-improving loop (DISC-002) is partitioned
  by role.
- **Address & channels** — the persona's bus identity and the default channels it
  joins (exec channel, per-repo dev channel) per DISC-029/030.

## Deliverable

A regin **role-persona spec** (config + skills + capability scope + memory scope +
authority) for at least: CEO, CFO, CIO/CTO, per-repo development lead — aligned
1:1 with dvalin DISC-030's role × capability matrix — that spawns FEATs for the
persona loader, per-role capability enforcement, and the per-role skill bundles.

## Spawned features (to derive on close)

- Role-persona config + loader (identity, rules, skills, channels, reporting)
- Per-role capability/tool enforcement (allow-ceiling from the role profile)
- Per-role skill bundles (exec vs dev-lead vs support …)
- Per-role memory scoping + Hermes partitioning (DISC-002)
