---
name: project-troubleshooter
description: |
  Diagnose `project` failures. Trigger when a build /
  configure / install / package / task / sessions verb
  fails. Reads logs, traces backend invocation, checks
  marker files, probes tool availability. Produces a
  root-cause hypothesis + minimal repro.
---

# `project-troubleshooter` skill

## 1. Design principles

- **Hypothesis first, fix later.** The troubleshooter
  localises; the actual fix belongs to `project-author`
  (code) or `project-packager` (distribution).
- **Distinguish env from code.** A missing host tool
  is a user-env issue. A wrong dispatch path is a
  code issue. Only the latter becomes a BUG-NNN.
- **Read-only.** Never `rm -rf` a cache directory or
  pkill a stuck process without explicit consent.

## 2. The failure-mode catalogue

| Symptom                                     | Likely cause             | Reference |
|---------------------------------------------|--------------------------|-----------|
| `autogen` fails immediately                 | autoconf tools missing   | bin/project:141-164 |
| `configure` runs but produces no output     | `--enable-silent-rules` swallowing output | retry without it |
| `compile` fails only with `-j N > 1`        | non-deterministic Makefile | retry `-j1` |
| `task pid <id>` returns nothing             | broken `$PID=` assignment | BUG-167 |
| `task list` shows wrong directory contents  | undefined `$TASK` in pushd | BUG-168 |
| `task monitor` opens blank windows          | wrong tmux target          | BUG-169 |
| `build <verb>` exits 0 on error             | `die()` always exits 0    | BUG-170 |
| `sessions help` errors before printing      | unconditional `tmux start-server` | BUG-172 |

## 3. Workflow recipes

1. **The build broke, what's wrong?**

       project troubleshoot compile
       # → runs probes, narrows the failure surface

2. **Produce a minimal repro.**

       project troubleshoot --minimise
       # → strips the project tree to smallest failing state

3. **Is this our bug or upstream?**

       project troubleshoot --backend
       # → runs the raw backend command outside project
       # → if it still fails: upstream; otherwise: project

## 4. Guardrails

1. **No destructive recovery.** Never `rm -rf
   $XDG_CACHE_HOME/project`, `pkill -9 -f project`, or
   similar without explicit consent.
2. **No `--no-verify` / `--skip-checks`.** Bypassing
   safety mechanisms is not a diagnosis.
3. **No dependency downgrades.** "Pin to an older
   version" is rarely the right answer; root-cause
   the incompatibility instead.
4. **One BUG per genuine code issue.** Env issues get
   a note in the report; code issues get a BUG-NNN
   filed under `issues/bug/`.

## 5. Where to read more

- Open bugs under `issues/bug/`
- FEAT-166 SIT containers (each reproduces a known-
  good environment for comparison)
- `share/doc/project/standards/` (FEAT-163, pending)
