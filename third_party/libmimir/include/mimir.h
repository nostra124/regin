/*
 * libmimir — an embeddable C client for Mimir LLM gateways.
 *
 * Mimir (the Raven provider hub) speaks OpenAI-compatible HTTP at
 * `<base>/chat/completions`. libmimir lets any C/embeddable host get LLM
 * capability by talking to one or more Mimir servers, with failover,
 * without re-implementing a provider SDK. Raven itself will sit on this
 * via FFI (operator decision 2026-06-18).
 *
 * Design: the HTTP/TLS stack is NOT baked in. The caller installs a
 * `mimir_transport_fn` (a thin libcurl/OpenSSL shim in production); the
 * core — request marshalling, multi-server failover, response parsing —
 * is transport-agnostic and unit-testable with a stub. This keeps the
 * dependency surface a caller choice and the logic verifiable anywhere.
 */
#ifndef LIBMIMIR_MIMIR_H
#define LIBMIMIR_MIMIR_H

#include <stddef.h>
#include <stdint.h>

#ifdef __cplusplus
extern "C" {
#endif

/* Result codes. 0 is success; everything else is a failure mode. */
typedef enum {
    MIMIR_OK = 0,
    MIMIR_ERR_INVALID = 1,      /* NULL / empty required argument        */
    MIMIR_ERR_NO_TRANSPORT = 2, /* no transport installed                */
    MIMIR_ERR_TRANSPORT = 3,    /* every configured server failed        */
    MIMIR_ERR_PROTOCOL = 4,     /* a server replied but it didn't parse  */
    MIMIR_ERR_OOM = 5           /* allocation failed                     */
} mimir_status;

/*
 * A single transport invocation. Send `request_json` as the POST body to
 * `url` (a fully-formed chat-completions endpoint) with bearer `api_key`
 * (may be NULL). On success return 0 and set *response_json to a malloc'd
 * NUL-terminated response body — libmimir takes ownership and frees it.
 * Return non-zero to signal this server failed; libmimir advances to the
 * next configured server. `ctx` is the pointer passed to
 * mimir_client_set_transport.
 */
typedef int (*mimir_transport_fn)(void *ctx,
                                   const char *url,
                                   const char *api_key,
                                   const char *request_json,
                                   char **response_json);

typedef struct mimir_client mimir_client;

/*
 * Create a client over one or more Mimir base URLs, tried in order with
 * failover. Each base URL is an OpenAI-compat root, e.g.
 * "https://host/api/mimir/v1"; libmimir appends "/chat/completions".
 * `api_key` may be NULL. Returns NULL on invalid args or OOM.
 */
mimir_client *mimir_client_new(const char *const *base_urls,
                               size_t n_urls,
                               const char *api_key);

/* Install the transport (and its context). Required before mimir_chat. */
void mimir_client_set_transport(mimir_client *client,
                                mimir_transport_fn fn,
                                void *ctx);

/* Number of configured servers (for diagnostics / tests). */
size_t mimir_client_server_count(const mimir_client *client);

/*
 * One non-streaming chat turn: send `prompt` to `model`, trying each
 * server in order until one returns a parseable reply. On MIMIR_OK,
 * *out_reply is a malloc'd NUL-terminated assistant message the caller
 * must free(). Unchanged on failure.
 */
mimir_status mimir_chat(mimir_client *client,
                        const char *model,
                        const char *prompt,
                        char **out_reply);

/* One chat message (NULL role defaults to "user"; NULL content to ""). */
typedef struct {
    const char *role;    /* "system" | "user" | "assistant" | … */
    const char *content;
} mimir_message;

/*
 * Like mimir_chat, but sends a full conversation (`messages[0..n_messages)`)
 * — the path a host with history/system prompts uses. Same failover and
 * ownership contract as mimir_chat.
 */
mimir_status mimir_chat_messages(mimir_client *client,
                                 const char *model,
                                 const mimir_message *messages,
                                 size_t n_messages,
                                 char **out_reply);

/*
 * Like mimir_chat_messages but embeds `tools_json` (a JSON array string,
 * or NULL) and `extras` (a comma-free JSON object fragment such as
 * `"raven_hints":{…}`, or NULL — e.g. forwarded task preferences) into the
 * request, and returns the **raw** response body unparsed in *out_response
 * — the caller's own JSON parser extracts content / tool_calls. Same
 * failover + ownership contract. This is the path a host with a real JSON
 * library (e.g. Raven via serde) uses.
 */
mimir_status mimir_chat_messages_raw(mimir_client *client,
                                     const char *model,
                                     const mimir_message *messages,
                                     size_t n_messages,
                                     const char *tools_json,
                                     const char *extras,
                                     char **out_response);

/*
 * Pure transport: POST a **fully-formed** `request_json` body (the caller
 * built every field) and return the raw response in *out_response. The
 * path a host with a real JSON serialiser uses to express things the
 * convenience builders can't — e.g. OpenAI-compat multimodal `content`
 * arrays (images / documents). Same failover + ownership contract.
 */
mimir_status mimir_chat_raw(mimir_client *client,
                            const char *request_json,
                            char **out_error,
                            char **out_response);

/* ---- streaming --------------------------------------------------------- */

/* Receives each assistant text delta as it is parsed from the SSE stream. */
typedef void (*mimir_chunk_fn)(void *ctx, const char *text_delta);

/*
 * The sink a streaming transport pushes received body bytes into; libmimir
 * supplies it and parses SSE from the chunks. Returns 0 to keep going.
 */
typedef int (*mimir_on_data_fn)(void *on_data_ctx, const char *data, size_t len);

/*
 * A streaming transport: POST `request_json` to `url` and deliver the
 * response body to `on_data` in chunks as they arrive. Return 0 on
 * success, non-zero to fail this server (libmimir fails over).
 */
typedef int (*mimir_stream_transport_fn)(void *ctx,
                                         const char *url,
                                         const char *api_key,
                                         const char *request_json,
                                         mimir_on_data_fn on_data,
                                         void *on_data_ctx);

/* Install the streaming transport (separate from the unary one). */
void mimir_client_set_stream_transport(mimir_client *client,
                                       mimir_stream_transport_fn fn,
                                       void *ctx);

/*
 * Streaming chat: `on_chunk(chunk_ctx, delta)` fires for each text delta;
 * the full assembled reply is written to *out_full (caller frees). Same
 * failover + ownership contract as the unary calls.
 */
mimir_status mimir_chat_stream(mimir_client *client,
                               const char *model,
                               const mimir_message *messages,
                               size_t n_messages,
                               mimir_chunk_fn on_chunk,
                               void *chunk_ctx,
                               char **out_full);

/*
 * Streaming counterpart of mimir_chat_raw: SSE-stream a fully-formed
 * `request_json` body (the caller built every field, e.g. multimodal
 * `content` arrays), delivering deltas to on_chunk and the assembled reply
 * to *out_full. Same failover + ownership contract.
 */
mimir_status mimir_chat_stream_raw(mimir_client *client,
                                   const char *request_json,
                                   mimir_chunk_fn on_chunk,
                                   void *chunk_ctx,
                                   char **out_full);

void mimir_client_free(mimir_client *client);

/* Human-readable name for a status code (static storage; do not free). */
const char *mimir_status_str(mimir_status status);

/* ---- account management (FEAT-506) -------------------------------------- */

/*
 * Ensure an account exists for the caller. The account is identified by
 * the client certificate fingerprint (TOFU). On first call, registers the
 * account with the gateway; on subsequent calls, returns the existing account.
 * On MIMIR_OK, *out_account_id is set to a malloc'd NUL-terminated account
 * hash string that the caller must free().
 */
mimir_status mimir_ensure_account(mimir_client *client,
                                   char **out_account_id);

/*
 * Get the account balance (in satoshis). The account is identified by the
 * client certificate fingerprint. On MIMIR_OK, *out_balance is set to the
 * account's satoshi balance.
 */
mimir_status mimir_get_balance(mimir_client *client,
                                int64_t *out_balance);

/*
 * Create a Lightning invoice for account top-up. The invoice, when paid,
 * credits the account with `sats` satoshis. On MIMIR_OK, *out_invoice is
 * set to a malloc'd NUL-terminated BOLT-11 invoice string that the caller
 * must free().
 */
mimir_status mimir_create_invoice(mimir_client *client,
                                   int64_t sats,
                                   const char *description,
                                   char **out_invoice);

/* ---- daemon auto-start (FEAT-471) --------------------------------------- */

/*
 * Check whether the local mimird daemon is answering at the default port.
 * Returns 1 if /health responds 200, 0 otherwise. Desktop-only; on mobile
 * this always returns 0 (no local daemon expected).
 */
int mimir_daemon_is_running(void);

/*
 * Try to start the local mimird daemon. Desktop-only; on mobile this is a
 * no-op and returns 0. On desktop, looks for `mimird` on PATH and spawns it
 * in the background. Returns 0 on success (daemon started or already running),
 * non-zero on failure (mimird not found, spawn failed). After spawning,
 * polls /health until ready or timeout (5 seconds).
 */
int mimir_ensure_daemon(void);

#ifdef __cplusplus
}
#endif

#endif /* LIBMIMIR_MIMIR_H */
