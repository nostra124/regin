---
id: DISC-022
type: discovery
priority: medium
status: done
complexity: M
spawned_features:
  - FEAT-087
---

# DISC-022 — Web UI with PAM authentication

## Describe

regin currently has only a terminal-based CLI (`regin chat`). Users want to
interact with regin from a **mobile browser** — checking task status, reviewing
memory, initiating chats, and viewing run history — without SSH-ing into the
machine.

The daemon (`regind`) already runs as a per-user systemd user service with a
Unix socket for CLI communication. The Web UI should reuse the same daemon
process and expose an HTTP server alongside the Unix socket, authenticated via
**PAM** (the user logs in with their Unix username/password).

**Key requirements from user:**
- `regin webui enable` — persist the setting (SQLite), default port
- Mobile-first responsive UI
- PAM authentication (system credentials)

## Variants considered

| Variant | Summary | Key trade-off |
|---------|---------|---------------|
| A | **Daemon-embedded HTTP server.** `regind` starts an HTTP listener on the configured port. Serves the web UI static files + a REST API for chat/runs/memory. Auth via PAM. | Simple single-process model; one port to manage |
| B | **Separate web server binary.** A new `regin-web` binary that reads from the same DB and talks to `regind` over the Unix socket. | Scales independently but adds a second binary + second auth boundary |
| C | **Reverse-proxy model.** Daemon serves only a REST API; static files are served by nginx/Caddy. | Most flexible but highest setup overhead; contradicts "no config files" principle |

## Decision matrix

| Criterion | Weight | A | B | C |
|-----------|--------|---|---|---|
| Single-binary simplicity | high | ✓ | ✗ | ✗ |
| Mobile-first out of the box | high | ✓ | ~ | ~ |
| PAM integration ease | high | ✓ | ~ | ✗ |
| No extra deps for user | high | ✓ | ✓ | ✗ |
| Scales to many concurrent users | low | ~ | ✓ | ✓ |

## Arguments

### Pro — Variant A (embedded HTTP in daemon)

- **Single process, single port.** The daemon already holds the event loop
  (tokio). Adding an HTTP listener is a few dozen lines of axum/warp; the
  existing chat/runs/memory dispatch is reused directly.
- **PAM fits naturally.** The HTTP handler calls `pam_auth(username, password)`
  against the system's PAM stack (using `pam` crate). Session tokens (JWT or
  random bearer) are cached with an expiry.
- **Mobile-first SPA** is served as embedded static files (compiled into the
  binary via `rust-embed`). Zero deployment steps for the user.
- **`regin webui enable`** writes to SQLite config; the daemon picks up the
  change and starts the HTTP listener on the next restart (or hot-reload via
  a SIGHUP / config-watch).

### Con / risks

- **PAM crate requires `libpam`** — a build-time dependency that must be
  documented in `profile.md` and handled in packaging (Debian: `libpam0g-dev`,
  RPM: `pam-devel`, Alpine: `pam-dev`).
- **Brute-force protection.** The PAM endpoint needs rate-limiting to prevent
  password guessing. Start with a simple per-IP delay (3 fails → 5s wait).
- **TLS.** Serving HTTP (not HTTPS) over a network port means passwords are
  sent in cleartext. Recommendations: bind to `127.0.0.1` by default (safe on
  single-user machines) and document that a reverse proxy should add TLS for
  remote access. Future: optional self-signed cert generation.

## Decision

**Chosen:** Variant A — embedded HTTP server in `regind`, PAM auth, mobile-first
SPA served from the binary.

**Why:** Simplest deployment (one binary, one port), best PAM integration
(in-process), and zero setup for the user beyond `regin webui enable`. TLS is
deferred to a reverse-proxy layer; the default bind to `127.0.0.1` keeps it
safe on single-user machines. Rate-limiting and session tokens mitigate the
PAM attack surface.

### Key design points

1. **Port config**: Default 8080. `regin webui enable --port 8080` or
   `regin config set webui.port 8080`. `webui disable` to stop.

2. **Auth flow**: 
   - `POST /api/auth/login` with `{ "username": "...", "password": "..." }`
   - Daemon calls `pam_authenticate(username, password, service="regin")`
   - On success: returns a bearer token (random 32-byte hex, SHA-256 stored
     in SQLite, expires in 24h)
   - Subsequent requests: `Authorization: Bearer <token>` header
   - Token refresh: `POST /api/auth/refresh` returns a new token

3. **API endpoints (v1)**:
   - `GET /api/health` — daemon status
   - `GET /api/sessions` — list recent sessions
   - `POST /api/chat` — send a message, stream response via SSE
   - `GET /api/memory` — list memories
   - `GET /api/runs` — task run history
   - `GET /api/config` — read-only config values

4. **Frontend**: Mobile-first SPA (vanilla HTML+CSS+JS, no framework — keeps
   the binary small and avoids a build toolchain for one page). Embedded via
   `rust-embed`. Responsive layout (CSS grid/flexbox), dark theme.

5. **Persistence**: `webui.enabled` (bool), `webui.port` (u16) in SQLite
   settings.

## Spawned features

| FEAT | Title |
|------|-------|
| FEAT-087 | Web UI server (embedded HTTP, PAM auth, mobile-first SPA) |
