---
id: FEAT-087
type: feature
priority: medium
complexity: L
estimate_tokens: 100k-200k
estimate_time: 2-4h
phase: open
status: open
spawned_from: DISC-022
---

# FEAT-087 — Web UI server (embedded HTTP, PAM auth, mobile-first SPA)

## Description

**As a** regin user
**I want** a mobile-first web interface I can open in my phone's browser,
authenticated with my Unix password
**So that** I can check sessions, chat, and view run history without SSH

`regind` gets an embedded HTTP server (axum/warp) that serves a responsive
SPA and a REST API. PAM authentication gates the admin area. The feature is
enabled with `regin webui enable` and disabled with `regin webui disable`.

## Acceptance Criteria

1. **`regin webui enable [--port <N>]`** sets `webui.enabled=true` and
   `webui.port=<N>` (default 8080) in SQLite config. Prints the URL.

2. **`regin webui disable`** sets `webui.enabled=false`. Running daemon picks
   this up (stops HTTP listener on next restart, or hot-reload via SIGHUP).

3. **`regin webui status`** prints enabled/disabled, port, and whether the
   listener is currently active.

4. **HTTP server** starts inside `regind` when `webui.enabled=true`. Binds to
   `127.0.0.1:<port>`. Three URL namespaces:

   - **`/`** — public website (no auth). Serves a user-configurable landing
     page (default: a minimal regin info page). Can be replaced by regin
     or the user with custom content.

   - **`/artifacts`** and **`/repo`** — public build artifacts and package
     repositories (no auth, but path configurable — see §8). Defaults:
     - `/artifacts/<type>/<file>` — downloadable build outputs
     - `/repo/apt/` — APT repository
     - `/repo/rpm/` — RPM repository

   - **`/regin/*`** — authenticated area (PAM gate on all paths). Serves:
     - SPA at `/regin/` (multi-tab admin interface)
     - REST API at `/regin/api/*`

5. **PAM authentication**:
   - `POST /regin/api/auth/login` accepts `{ "username", "password" }`, calls
     `pam_authenticate()` with service name `"regin"`, returns a bearer token
     (32-byte random hex, SHA-256 hash stored in SQLite, 24h expiry).
   - All `/regin/*` paths (except `/regin/api/auth/login` and
     `/regin/api/health`) require `Authorization: Bearer <token>` or a valid
     session cookie set on login.
   - `POST /regin/api/auth/refresh` issues a new token (old one revoked).
   - Rate limit: max 5 failed attempts per IP per minute; 10s cooldown after
     3 failures.
   - The public `/`, `/artifacts`, and `/repo` namespaces have **no auth** —
     they are world-readable.

6. **REST API v1 endpoints** (all under `/regin/api/`):
   - `GET /regin/api/health` → `{"ok":true,"version":"..."}` (no auth required)
   - `GET /regin/api/sessions` → paginated session list (title, date, message_count)
   - `WS /regin/api/chat` → WebSocket endpoint; client sends
     `{ "message": "...", "directory": "/path" }`, server streams assistant
     response tokens as JSON messages over the socket
   - `WS /regin/api/terminal` → WebSocket endpoint; spawns a PTY
     (pseudo-terminal) on the server, relays VT220 I/O bidirectionally. Client
     sends a `{ "directory": "/path" }` init message on connect.
   - `WS /regin/api/goal` → WebSocket endpoint; client sends
     `{ "goal": "...", "directory": "/path" }`. Server streams status updates
     as JSON messages: `{ "type": "plan", "steps": [...] }`,
     `{ "type": "step_start", "step": "..." }`,
     `{ "type": "step_done", "step": "..." }`,
     `{ "type": "log", "text": "..." }`,
     `{ "type": "error", "text": "..." }`,
     `{ "type": "done", "summary": "..." }`.
     The daemon runs the goal loop (decompose → execute each step → report).
   - `POST /regin/api/tabs/register` → accepts `{ "name": "...", "icon": "...",
     "html": "..." }`. Registers a new dashboard tab type in the daemon's
     tab registry (in-memory, persisted to SQLite). The SPA picks up new tab
     types on next load.
   - `GET /regin/api/tabs` → returns list of registered dashboard tab types
     (name, icon, html snippet).
   - `GET /regin/api/memory` → list memories (searchable via `?q=...`)
   - `GET /regin/api/runs` → task run history (paginated)
   - `GET /regin/api/config` → read-only config snapshot (keys/values, no secrets)

7. **Build artifacts** (public, served under configurable path — default `/artifacts`):
   - regin's build output directory (`/var/lib/regin/artifacts/` or configurable)
     is served as a static file tree.
   - When regin builds a package (`.deb`, `.rpm`, `.apk`), the result is placed
     in a subdirectory and becomes downloadable:
     `/artifacts/deb/regin_0.8.0_amd64.deb`
     `/artifacts/rpm/regin-0.8.0-1.x86_64.rpm`
   - Directory listing is enabled: `GET /artifacts/deb/` returns an HTML index.

8. **Package repositories** (public, served under configurable path — default `/repo`):
   - **APT repository** at `/repo/apt/` serves a valid Debian repo structure.
     regin runs `apt-ftparchive` (or generates metadata inline) to produce
     `Release`, `Packages.gz`, `Release.gpg`. Users add:
     ```
     deb http://<host>:8080/repo/apt/ ./
     ```
   - **RPM repository** at `/repo/rpm/` serves a valid RPM repo structure
     (`repodata/repomd.xml`). regin runs `createrepo` on the RPM artifact dir.
   - Repo metadata is regenerated on each new artifact build or on a
     configurable cadence. No auth required — repos are world-readable.

9. **Path configurability**: The user can remap or restrict public paths via
   `regin webui` subcommands or config keys:
   - `webui.public.artifacts` — URL path for artifacts (default `/artifacts`,
     set to empty to disable)
   - `webui.public.repo` — URL path for repos (default `/repo`,
     set to empty to disable)
   - Individual artifact and repo paths can also be moved under `/regin/*` to
     make them private (set to e.g. `/regin/artifacts`).
   - The landing page at `/` is always public.

10. **Mobile-first SPA**: Single HTML page with embedded CSS+JS (no build step,
    no framework). Responsive layout:
    - Dark theme, full-width on mobile (<768px), constrained on desktop
    - **Multi-tab interface**: each tab has a **type** (distinguished by a small
      icon) and opens a view appropriate to that type. Three built-in types:

      **Chat tab** (`💬` icon) — working directory selector at the top, message
      bubbles, streaming indicator, input bar. Sends messages via
      `WS /regin/api/chat` with the chosen directory scope.

      **Terminal tab** (`🖥` icon) — a web-based terminal (xterm.js or similar
      lightweight VT220 emulator) connected via WebSocket to a shell session
      running in the daemon's context. User can run arbitrary commands in a
      specified working directory.

      **Goal tab** (`🎯` icon) — user sets a high-level objective (e.g. "set up
      Postfix with DKIM signing"), regin works toward it across multiple
      turns autonomously. Shows:
      - The goal statement (editable)
      - A plan checklist — regin decomposes the goal into steps and ticks them
        off as it completes them
      - A live status log (stream of actions taken, errors, results)
      - A "Stop" button to abort the goal
      - Under the hood: `WS /regin/api/goal` WebSocket. Client sends
        `{ "goal": "...", "directory": "/path" }`. Server streams status
        updates (`{ "type": "plan" | "step_start" | "step_done" | "log" |
        "error" | "done" }`) as JSON messages.

    - **Dashboard tabs** — regin can **code its own tab types** at runtime. When
      the user asks for a dashboard (e.g. "show me disk usage", "make a service
      control panel"), regin:
      1. Generates the tab's HTML/CSS/JS as a self-contained snippet
      2. Registers it via `POST /regin/api/tabs/register` with a name, icon,
         and the snippet
      3. The SPA renders the snippet in an iframe or sandboxed div, passing an
         `api` object for making authenticated API calls back to the daemon
      This makes the web UI extensible by the agent itself — no rebuild needed.

    - Tabs are persistent within the browser session (localStorage). Users can
      open multiple tabs of any type.
    - Tab bar at top with icons; active tab is highlighted. Add/close tab buttons.
    - Login form redirects to main view on success.

11. **Build dependency**: `libpam` development headers (`libpam0g-dev` /
    `pam-devel` / `pam-dev`). Documented in `profile.md` §3 (dependencies).
    The `pam` crate is the PAM binding. Feature-gated behind `webui` Cargo
    feature (default off) so systems without libpam can still build.

12. **Packaging**: Debian/RPM/APK packages add `libpam0g-dev` (or equiv) as a
    build dependency. The `regind` binary is built with `--features webui`.
    PAM service file `/etc/pam.d/regin` is shipped in the package (default:
    `@include common-auth`).

13. **Unit tests** cover: PAM auth mock (success/failure), token generation and
    expiry, rate limiter, API endpoint dispatch, WebSocket chat streaming,
    PTY terminal session lifecycle, goal loop (decompose → execute → report),
    tab registration and listing, artifact directory listing, repo metadata
    generation.

14. **Integration test**: `regin webui enable --port 9090` → `curl
    localhost:9090/regin/api/health` returns 200.

## Out of scope (v1)

- TLS (use a reverse proxy; document how)
- Multi-user sessions (the daemon is per-user; auth proves identity)
- File upload / drag-and-drop
- Push notifications
