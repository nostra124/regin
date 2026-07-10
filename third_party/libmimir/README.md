# libmimir

An embeddable **C client for Mimir LLM gateways**. Mimir (the Raven
provider hub) speaks OpenAI-compatible HTTP; `libmimir` lets any
C/embeddable host get LLM capability by talking to **one or more** Mimir
servers (with failover) without re-implementing a provider SDK. Raven
itself will sit on this via FFI (operator decision 2026-06-18).

## Design — injectable transport

The HTTP/TLS stack is **not** baked in. The caller installs a
`mimir_transport_fn` (a thin libcurl/OpenSSL shim in production); the core
— request marshalling, multi-server failover, response parsing — is
transport-agnostic. That keeps the dependency surface a *caller* choice
and makes the logic verifiable anywhere, with no network:

```c
#include "mimir.h"

const char *servers[] = { "https://host-a/api/mimir/v1",
                          "https://host-b/api/mimir/v1" };
mimir_client *c = mimir_client_new(servers, 2, "rvn_…");
mimir_client_set_transport(c, my_curl_transport, my_ctx);

char *reply = NULL;
if (mimir_chat(c, "model-id", "Hello", &reply) == MIMIR_OK) {
    puts(reply);
    free(reply);
}
mimir_client_free(c);
```

## Build & test

```sh
make test                 # build libmimir.a + run the unit tests (cc, no deps)
make install PREFIX=…     # install mimir.h(+_curl.h) + libmimir.a
make curl                 # also build the libcurl transport (needs libcurl-dev)
```

From the repo root, `make check` also runs the C tests, and
`make {build,check,install}-libmimir` wrap the above.

The tests drive the core through a **stub transport** — happy path, JSON
escaping, multi-server failover, all-servers-fail, missing transport,
unparseable response, reply unescaping, the conversation + raw/tools/extras
paths, raw multimodal bodies, `out_error` propagation (an upstream
`http 403`), and SSE streaming (whole / byte-by-byte / split + raw). Clean
under `-fsanitize=address,undefined`.

## Capabilities

- **Unary + conversation chat** — `mimir_chat`, `mimir_chat_messages`
  (convenience builders; targeted response scanner for `content`).
- **Raw passthrough** — `mimir_chat_messages_raw` embeds a caller-built
  `tools` array plus an `extras` object fragment (e.g. forwarded task
  hints) and returns the unparsed body; `mimir_chat_raw` POSTs a
  **fully-formed** body the caller built (the only way to express
  OpenAI-compat **multimodal** `content` arrays — images / documents).
  The host parses content / tool_calls — what Raven does via serde.
- **Streaming** — `mimir_chat_stream` parses SSE incrementally and fires
  a per-delta callback; `mimir_chat_stream_raw` is the streaming
  counterpart of `mimir_chat_raw` (caller-built body, multimodal).
- **Error propagation** — on all-servers-failed, `mimir_chat_raw`'s
  `out_error` carries the last transport message (e.g. an upstream
  `http 403`) so the host can surface the real status instead of a
  generic failure.
- **Multi-server failover** across the configured base URLs.

## Transports

The HTTP/TLS stack is a caller choice (injectable transport):

- **Rust** — `mimir-lib` (the FFI binding) plugs in a `reqwest`
  transport; Raven's provider layer (`mimir-providers::MimirProvider`)
  uses libmimir as its Mimir backend — content, **multimodal**
  (images/documents), streaming, tool-calling, forwarded task hints, and
  surfaced HTTP errors. Raven builds the full request body in Rust (serde)
  and drives `mimir_chat_raw` / `mimir_chat_stream_raw`. This is the
  in-tree path (built by the crate's `build.rs`). The blocking transport
  runs on a bare `std::thread` (a `reqwest::blocking` runtime can't drop
  in an async context).
- **C** — `mimir_curl_install()` (see `mimir_curl.h`) installs
  libcurl-backed unary + streaming transports for standalone embedders.
  **VERIFY-ON-HOST:** needs `libcurl-dev` to compile and a reachable
  server to exercise.

## Follow-ups

- A real JSON parser could replace the targeted response scanner in the
  convenience (`mimir_chat*`) path; the raw/Rust path already parses with
  serde, so this only affects standalone C convenience callers.
- Streaming tool-call deltas (the unary path already supports tools).
- `out_error` is wired for `mimir_chat_raw`; extending it to the streaming
  + convenience entry points would surface upstream statuses there too.
