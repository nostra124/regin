---
id: BUG-001
type: bug
priority: medium
complexity: S
estimate_tokens: 20k-40k
estimate_time: 30-60min
phase: open
status: open
---

# Chat (and first use) should register + start the systemd user service, not just spawn a loose process

## Description
**As an** operator
**I want** that, when the daemon is not running and I enter chat (or any
daemon-backed command), regin **registers and starts** the lingering systemd
user service
**So that** regind persists and survives reboots from then on — instead of being
a transient process that dies with my session.

Current behaviour: `ensure_daemon()` (regin-cli) connects to the socket and, if
absent, **spawns the `regind` binary directly** as a detached child. That child
is not under systemd, has no lingering, and does not survive logout/reboot.
Enabling the persistent service only happens via the explicit
`config set daemon.enabled true`. The requirement is that first use auto-registers.

## Implementation
- In `ensure_daemon()`, when the socket is unreachable:
  1. If a user session bus + systemd is available, run the same install path as
     `handle_daemon_enabled(true)` (write unit, `enable-linger`, `daemon-reload`,
     `systemctl --user enable --now regind`) and set `daemon.enabled=true`.
  2. Fall back to the current direct-spawn only when systemd-user is unavailable
     (e.g. minimal containers), so non-systemd environments still work.
- Make auto-register honour an opt-out: if `daemon.enabled` was explicitly set to
  `false` by the user, respect it and only spawn transiently.
- Keep the socket-poll wait loop after starting.

## Acceptance Criteria
1. On a systemd-user host, `regin chat` with no running daemon installs + enables
   the `regind` user service (lingering on) and connects.
2. After a simulated reboot (service enabled), regind is brought up by systemd
   without running any regin command.
3. On a host without systemd-user, behaviour falls back to the current
   direct-spawn and chat still works.
4. An explicit `daemon.enabled=false` is respected (no auto-register).
