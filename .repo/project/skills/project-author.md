---
name: project-author
description: |
  Author and operate `project` — multi-build-system
  frontend with task and sessions integration. Trigger
  when the user wants to compile / check / install /
  package a project across build systems (autoconf /
  cmake / meson / cargo / npm / go), manage long-
  running tasks, or attach to per-project tmux
  sessions.
---

# `project-author` skill

## 1. Design principles

- **Educational.** Reading `bin/project`'s detection
  logic teaches the standard build-system invocations
  (autogen.sh, configure, make, cmake -B, cargo
  build, etc).
- **Functional.** Each verb is a thin wrapper over
  the underlying tool. State (build artefacts) lives
  in the project tree.
- **Decentralized.** Per-project; no central
  build-server.
- **Simple.** `project` calls only `account` and
  `config` at runtime.

## 2. The model

A **project** is a directory at `~/src/<name>/`. Per
FEAT-158, `project` detects the build system from
markers in the tree:

| Marker            | Backend   |
|-------------------|-----------|
| `configure.ac`    | autoconf  |
| `CMakeLists.txt`  | cmake     |
| `meson.build`     | meson     |
| `Cargo.toml`      | cargo     |
| `package.json`    | npm       |
| `go.mod`          | go        |
| `Makefile`        | make      |

The build-cycle verbs (`autogen`, `configure`,
`compile`, `check`, `install`, `package`) dispatch to
the matching backend.

Per FEAT-159, `--background` routes long verbs
through `project task` so closing the terminal
doesn't kill the build.

Per FEAT-157, `project sessions` attaches a per-
project tmux session.

## 3. Workflow recipes

1. **Build cycle (auto-detected).**

       cd ~/src/myapp
       project autogen
       project configure
       project compile
       project check
       project install

2. **Background a long compile.**

       project compile --background
       project task list
       project task show <id>

3. **Per-project tmux session.**

       project sessions attach myapp

4. **Package for current distro (FEAT-162 pending).**

       project package deb        # → myapp.deb
       project package apk        # → myapp.apk

## 4. Guardrails

1. **Build-system detection is order-sensitive.** A
   project with both `Cargo.toml` and `Makefile`
   defaults to cargo; override via
   `--backend make`.
2. **`project package` formats are gated by host
   tooling.** Generating a `.deb` needs `dpkg-deb`;
   generating `.apk` needs `abuild`. Verify with
   `project package --check`.
3. **`--background` requires `project task`** to be
   running — it routes through it. If task isn't up,
   the verb falls back to foreground with a warning.
4. **`project sessions` uses `tmux`** — make sure
   tmux is installed and your `tmux.conf` doesn't
   collide with project's session-naming scheme.
5. **`project compile`'s default `make` target is
   the all-target.** If your project's default
   target isn't safe-to-rerun, override with
   `--target build`.

## 5. Where to read more

- `man project`
- `share/doc/project/standards/` — vendored build-
  system + dev-tools references (FEAT-163, pending)
- This package's `CLAUDE.md`
