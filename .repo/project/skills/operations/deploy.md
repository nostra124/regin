---
name: deploy
description: |
  Deploying built packages to remote hosts via a
  repo-local `contrib/deploy` helper. Defines the
  probe → build → upload → install phases, the
  remembered-hosts inventory, and the rule that a
  project shipping native packages MUST ship a deploy
  script. Trigger when a project gains packaging, when
  asked to roll a release out to hosts, or when
  scoping a milestone's delivery prerequisites.
---

# `deploy` skill

## 1. The rule

Any project that ships native packages (see the `project-packager` skill and
RULE-010 "Platform packages") **must also ship a `contrib/deploy` script**.
Building artefacts that no one can roll out is half a delivery. Packaging and
deployment are one delivery prerequisite, not two.

The reference implementation is `dvalin`'s own `contrib/deploy` (adapted from
the `bifrost`/`dht` deploy script). Copy its shape; do not reinvent it.

## 2. What the script does

`contrib/deploy [subcommand] [--parallel|--serial] [user@host ...]` is
**three-phased** and idempotent:

1. **Probe** every host in parallel over SSH to learn its OS family
   (`deb`/`rpm`/`apk`/`pkg`) and architecture.
2. **Build** only the packages actually needed for the probed hosts, and only
   if stale — freshness is judged against the last commit that touched
   `src`/`Cargo.*`/`install`/`packages`/`contrib`. Builds run in clean podman
   containers (one per OS/arch); the macOS `.pkg` builds locally on a Mac.
3. **Upload + install** the right package on each host (via an SSH `cat` pipe,
   which works even where `sftp-server` is absent), then report version.

Services are installed **disabled**. Deploy never auto-starts a daemon that
needs configuration (an address/key, an API token); it prints the enable
command instead.

## 3. Remembered hosts

Targets are remembered **outside the repo** (hostnames are private) in
`${XDG_CONFIG_HOME:-~/.config}/<project>/deploy-hosts`, overridable with
`<PROJECT>_DEPLOY_HOSTS`. Subcommands:

| Command | Effect |
|---|---|
| `deploy user@host …` | Deploy to those hosts and remember them |
| `deploy all` | Deploy to every remembered host |
| `deploy add` / `del` | Edit the inventory without deploying |
| `deploy list` | Print remembered hosts |

## 4. Guardrails

1. **Probe before build.** Never build the full matrix blindly — build only
   what the probed hosts need. A single host means a single package.
2. **Fail safe.** A failed probe skips that host; it must not abort the others.
   Fetch/download the new package before removing the old install so a network
   failure leaves the host's current install intact.
3. **No secrets in the repo.** The hosts file and any keys live in the user's
   config/`secret`, never committed.
4. **Disabled by default.** Deploy installs; the operator enables services.
5. **idempotent.** Re-running deploy on an up-to-date host is a no-op build plus
   a re-install — safe to run repeatedly.

## 5. Where this fits

- Packaging itself: the `project-packager` skill and `packages/README.md`.
- Delivery prerequisite: RULE-010 and `operations/milestone.md` — a milestone's
  "Platform packages" prerequisite is not satisfied until packages build **and**
  `contrib/deploy` can roll them out.
- **UAT loop (alpha/beta):** during the alpha and beta phases every merged fix is
  re-packaged and deployed here for the tester — see `operations/milestone.md`
  §5 "Alpha and beta: package and deploy every fix for UAT". A merged-but-not-
  deployed fix has not reached acceptance testing. Installs (the tester's and any
  local one) are done **from the native package**, never source/`cargo install`;
  the end-of-milestone demo is driven through the installed package too.

## execd security model (FEAT-068)

`dvalin-execd` runs **arbitrary programs** (the `command` tool, dwarfs), so it runs
least-privilege as a **dedicated unprivileged user** (`_dvalin-execd` on macOS /
`dvalin-execd` on Linux), created by the package and pinned in the service unit —
never root, never the installing human. `dvalind` runs as its **own** user
(`_dvalind`), separate from execd; both share the **`_dvalin` group** only for the
run store / state dir. Container steps use **rootless podman** under the execd
user (defense in depth).

Because the execd user can't see your project by default, you **explicitly grant**
the work dir to the `_dvalin` group before execd can build/test it:

```sh
chgrp -R _dvalin <project-dir> && chmod -R g+rwX <project-dir>
```

This is a feature: execd touches only what you hand it.
