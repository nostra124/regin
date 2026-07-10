/* libmimir core — see include/mimir.h. Transport-agnostic by design. */
#include "mimir.h"

#include <stdlib.h>
#include <string.h>

/* Local string-dup so the library stays clean under strict -std=c11
 * (POSIX strdup isn't exposed without a feature-test macro). */
static char *dupstr(const char *s) {
    size_t n = strlen(s) + 1;
    char *p = malloc(n);
    if (p) memcpy(p, s, n);
    return p;
}

struct mimir_client {
    char **urls;       /* chat-completions endpoints, owned */
    size_t n_urls;
    char *api_key;     /* owned, may be NULL */
    mimir_transport_fn transport;
    void *transport_ctx;
    mimir_stream_transport_fn stream_transport;
    void *stream_ctx;
};

/* NUL-terminated copy of `n` bytes (for slices handed to extract_content). */
static char *dupn(const char *s, size_t n) {
    char *p = malloc(n + 1);
    if (!p) return NULL;
    memcpy(p, s, n);
    p[n] = '\0';
    return p;
}

/* ---- a tiny growable string buffer ------------------------------------ */

typedef struct {
    char *data;
    size_t len;
    size_t cap;
    int oom; /* sticky: set once an allocation fails */
} sbuf;

static void sbuf_init(sbuf *b) {
    b->data = NULL;
    b->len = 0;
    b->cap = 0;
    b->oom = 0;
}

static int sbuf_reserve(sbuf *b, size_t extra) {
    if (b->oom) return 0;
    if (b->len + extra + 1 <= b->cap) return 1;
    size_t want = b->cap ? b->cap * 2 : 64;
    while (want < b->len + extra + 1) want *= 2;
    char *p = realloc(b->data, want);
    if (!p) {
        b->oom = 1;
        return 0;
    }
    b->data = p;
    b->cap = want;
    return 1;
}

static void sbuf_putc(sbuf *b, char c) {
    if (!sbuf_reserve(b, 1)) return;
    b->data[b->len++] = c;
    b->data[b->len] = '\0';
}

static void sbuf_puts(sbuf *b, const char *s) {
    size_t n = strlen(s);
    if (!sbuf_reserve(b, n)) return;
    memcpy(b->data + b->len, s, n);
    b->len += n;
    b->data[b->len] = '\0';
}

/* Append `n` raw bytes (SSE chunks aren't NUL-terminated). */
static void sbuf_putn(sbuf *b, const char *s, size_t n) {
    if (!sbuf_reserve(b, n)) return;
    memcpy(b->data + b->len, s, n);
    b->len += n;
    b->data[b->len] = '\0';
}

/* Append `s` as the *interior* of a JSON string (no surrounding quotes),
 * escaping per RFC 8259. */
static void sbuf_put_json_escaped(sbuf *b, const char *s) {
    for (; *s; s++) {
        unsigned char c = (unsigned char)*s;
        switch (c) {
            case '"':  sbuf_puts(b, "\\\""); break;
            case '\\': sbuf_puts(b, "\\\\"); break;
            case '\n': sbuf_puts(b, "\\n"); break;
            case '\r': sbuf_puts(b, "\\r"); break;
            case '\t': sbuf_puts(b, "\\t"); break;
            case '\b': sbuf_puts(b, "\\b"); break;
            case '\f': sbuf_puts(b, "\\f"); break;
            default:
                if (c < 0x20) {
                    static const char hex[] = "0123456789abcdef";
                    char u[7] = "\\u0000";
                    u[4] = hex[(c >> 4) & 0xf];
                    u[5] = hex[c & 0xf];
                    sbuf_puts(b, u);
                } else {
                    sbuf_putc(b, (char)c);
                }
        }
    }
}

/* ---- request / response ------------------------------------------------ */

/* Append one {"role":...,"content":...} object. */
static void put_message(sbuf *b, const char *role, const char *content) {
    sbuf_puts(b, "{\"role\":\"");
    sbuf_put_json_escaped(b, role ? role : "user");
    sbuf_puts(b, "\",\"content\":\"");
    sbuf_put_json_escaped(b, content ? content : "");
    sbuf_puts(b, "\"}");
}

/*
 * Build {"model":...,"messages":[ … ]} plus, when `tools_json` is a
 * non-empty JSON array string, `,"tools":<tools_json>`, and when `extras`
 * is a non-empty **comma-free object fragment** (e.g. `"raven_hints":{…}`),
 * it verbatim. The caller (which has a real JSON serialiser) supplies both
 * — libmimir does not build them.
 */
static char *build_chat_request_full(const char *model,
                                     const mimir_message *messages,
                                     size_t n,
                                     const char *tools_json,
                                     const char *extras) {
    sbuf b;
    sbuf_init(&b);
    sbuf_puts(&b, "{\"model\":\"");
    sbuf_put_json_escaped(&b, model);
    sbuf_puts(&b, "\",\"messages\":[");
    for (size_t i = 0; i < n; i++) {
        if (i) sbuf_putc(&b, ',');
        put_message(&b, messages[i].role, messages[i].content);
    }
    sbuf_puts(&b, "]");
    if (tools_json && *tools_json) {
        sbuf_puts(&b, ",\"tools\":");
        sbuf_puts(&b, tools_json);
    }
    if (extras && *extras) {
        sbuf_putc(&b, ',');
        sbuf_puts(&b, extras);
    }
    sbuf_puts(&b, "}");
    if (b.oom) {
        free(b.data);
        return NULL;
    }
    return b.data;
}

/* Build {"model":...,"messages":[ … ]} from a message array. */
static char *build_chat_request_messages(const char *model,
                                         const mimir_message *messages,
                                         size_t n) {
    return build_chat_request_full(model, messages, n, NULL, NULL);
}

/* Single-user-message convenience. */
static char *build_chat_request(const char *model, const char *prompt) {
    mimir_message m = {"user", prompt};
    return build_chat_request_messages(model, &m, 1);
}

/*
 * Extract choices[0].message.content from an OpenAI-compat response.
 *
 * A targeted scanner, not a full JSON parser: it finds the "content" key
 * and unescapes its string value. Sufficient for the gateway's response
 * shape; swapping in a real JSON parser is a follow-up (a dependency
 * decision). Returns a malloc'd string, or NULL if not found / OOM.
 */
static char *extract_content(const char *json) {
    const char *p = strstr(json, "\"content\"");
    if (!p) return NULL;
    p += strlen("\"content\"");
    while (*p == ' ' || *p == '\t' || *p == '\n' || *p == '\r') p++;
    if (*p != ':') return NULL;
    p++;
    while (*p == ' ' || *p == '\t' || *p == '\n' || *p == '\r') p++;
    if (*p != '"') return NULL;
    p++;

    sbuf out;
    sbuf_init(&out);
    while (*p && *p != '"') {
        if (*p == '\\') {
            p++;
            switch (*p) {
                case 'n': sbuf_putc(&out, '\n'); break;
                case 'r': sbuf_putc(&out, '\r'); break;
                case 't': sbuf_putc(&out, '\t'); break;
                case 'b': sbuf_putc(&out, '\b'); break;
                case 'f': sbuf_putc(&out, '\f'); break;
                case '/': sbuf_putc(&out, '/'); break;
                case '"': sbuf_putc(&out, '"'); break;
                case '\\': sbuf_putc(&out, '\\'); break;
                case 'u': {
                    /* Minimal: decode the 4 hex digits; emit ASCII for the
                     * BMP-low range, else keep the literal escape. Full
                     * UTF-16 surrogate handling is a follow-up. */
                    int v = 0, ok = 1;
                    for (int i = 1; i <= 4; i++) {
                        char h = p[i];
                        v <<= 4;
                        if (h >= '0' && h <= '9') v |= h - '0';
                        else if (h >= 'a' && h <= 'f') v |= h - 'a' + 10;
                        else if (h >= 'A' && h <= 'F') v |= h - 'A' + 10;
                        else { ok = 0; break; }
                    }
                    if (ok) {
                        if (v < 0x80) {
                            sbuf_putc(&out, (char)v);
                        } else {
                            char u[7] = "\\u0000";
                            static const char hx[] = "0123456789abcdef";
                            u[2] = hx[(v >> 12) & 0xf];
                            u[3] = hx[(v >> 8) & 0xf];
                            u[4] = hx[(v >> 4) & 0xf];
                            u[5] = hx[v & 0xf];
                            sbuf_puts(&out, u);
                        }
                        p += 4;
                    }
                    break;
                }
                default:
                    if (*p) sbuf_putc(&out, *p);
            }
            if (*p) p++;
        } else {
            sbuf_putc(&out, *p);
            p++;
        }
    }
    if (*p != '"' || out.oom) {
        free(out.data);
        return NULL;
    }
    if (!out.data) {
        /* empty content "" — return an allocated empty string */
        out.data = malloc(1);
        if (out.data) out.data[0] = '\0';
    }
    return out.data;
}

/* ---- public API -------------------------------------------------------- */

mimir_client *mimir_client_new(const char *const *base_urls,
                               size_t n_urls,
                               const char *api_key) {
    if (!base_urls || n_urls == 0) return NULL;

    mimir_client *c = calloc(1, sizeof(*c));
    if (!c) return NULL;
    c->urls = calloc(n_urls, sizeof(char *));
    if (!c->urls) {
        free(c);
        return NULL;
    }
    for (size_t i = 0; i < n_urls; i++) {
        const char *base = base_urls[i];
        if (!base || !*base) {
            mimir_client_free(c);
            return NULL;
        }
        /* Join base + "/chat/completions", collapsing a trailing slash. */
        size_t blen = strlen(base);
        int trailing = (blen > 0 && base[blen - 1] == '/');
        const char *suffix = trailing ? "chat/completions" : "/chat/completions";
        size_t total = blen + strlen(suffix) + 1;
        char *url = malloc(total);
        if (!url) {
            mimir_client_free(c);
            return NULL;
        }
        memcpy(url, base, blen);
        strcpy(url + blen, suffix);
        c->urls[i] = url;
        c->n_urls++;
    }
    if (api_key && *api_key) {
        c->api_key = dupstr(api_key);
        if (!c->api_key) {
            mimir_client_free(c);
            return NULL;
        }
    }
    return c;
}

void mimir_client_set_transport(mimir_client *client,
                                mimir_transport_fn fn,
                                void *ctx) {
    if (!client) return;
    client->transport = fn;
    client->transport_ctx = ctx;
}

size_t mimir_client_server_count(const mimir_client *client) {
    return client ? client->n_urls : 0;
}

/* Shared failover loop: try each server with `request` (caller-owned)
 * until one returns a parseable reply. */
static mimir_status do_chat(mimir_client *client, const char *request,
                            char **out_reply) {
    mimir_status last = MIMIR_ERR_TRANSPORT;
    for (size_t i = 0; i < client->n_urls; i++) {
        char *response = NULL;
        int rc = client->transport(client->transport_ctx, client->urls[i],
                                   client->api_key, request, &response);
        if (rc != 0 || !response) {
            free(response);
            last = MIMIR_ERR_TRANSPORT;
            continue; /* failover to the next server */
        }
        char *content = extract_content(response);
        free(response);
        if (!content) {
            last = MIMIR_ERR_PROTOCOL;
            continue;
        }
        *out_reply = content;
        return MIMIR_OK;
    }
    return last;
}

/* ---- streaming (SSE) --------------------------------------------------- */

typedef struct {
    sbuf pending;          /* buffered bytes, may hold a partial SSE event */
    sbuf full;             /* accumulated assistant text */
    mimir_chunk_fn on_chunk;
    void *chunk_ctx;
    int done;              /* saw `data: [DONE]` */
} sse_state;

/* Process one SSE event (its `data:` payload). */
static void sse_handle_event(sse_state *st, const char *ev, size_t len) {
    size_t i = 0;
    while (i < len) {
        size_t j = i;
        while (j < len && ev[j] != '\n') j++;
        size_t llen = j - i;
        const char *line = ev + i;
        /* tolerate a trailing '\r' (CRLF streams) */
        if (llen && line[llen - 1] == '\r') llen--;
        if (llen >= 5 && memcmp(line, "data:", 5) == 0) {
            const char *payload = line + 5;
            size_t plen = llen - 5;
            while (plen && *payload == ' ') {
                payload++;
                plen--;
            }
            if (plen == 6 && memcmp(payload, "[DONE]", 6) == 0) {
                st->done = 1;
                return;
            }
            char *json = dupn(payload, plen);
            if (json) {
                char *delta = extract_content(json);
                free(json);
                if (delta) {
                    if (st->on_chunk) st->on_chunk(st->chunk_ctx, delta);
                    sbuf_puts(&st->full, delta);
                    free(delta);
                }
            }
        }
        i = j + 1;
    }
}

/* Drain complete events ("\n\n"-separated) from `pending`, keeping any
 * trailing partial event buffered for the next chunk. */
static void sse_drain(sse_state *st) {
    char *buf = st->pending.data;
    size_t plen = st->pending.len;
    size_t start = 0;
    for (size_t i = 0; buf && i + 1 < plen; i++) {
        if (buf[i] == '\n' && buf[i + 1] == '\n') {
            sse_handle_event(st, buf + start, i - start);
            start = i + 2;
            if (st->done) break;
        }
    }
    if (start > 0) {
        size_t rem = st->pending.len - start;
        memmove(st->pending.data, st->pending.data + start, rem);
        st->pending.len = rem;
        st->pending.data[rem] = '\0';
    }
}

/* The sink libmimir hands the streaming transport. */
static int sse_on_data(void *ctx, const char *data, size_t len) {
    sse_state *st = (sse_state *)ctx;
    if (st->done) return 1;
    sbuf_putn(&st->pending, data, len);
    if (st->pending.oom) return 1;
    sse_drain(st);
    return st->done ? 1 : 0;
}

void mimir_client_set_stream_transport(mimir_client *client,
                                       mimir_stream_transport_fn fn,
                                       void *ctx) {
    if (!client) return;
    client->stream_transport = fn;
    client->stream_ctx = ctx;
}

/* Drive the SSE stream for a ready `request` body across the servers (with
 * failover), assembling the full reply into *out_full. Shared by the
 * message-builder and raw-passthrough streaming entry points. */
static mimir_status do_stream(mimir_client *client, const char *request,
                              mimir_chunk_fn on_chunk, void *chunk_ctx,
                              char **out_full) {
    mimir_status last = MIMIR_ERR_TRANSPORT;
    for (size_t i = 0; i < client->n_urls; i++) {
        sse_state st;
        sbuf_init(&st.pending);
        sbuf_init(&st.full);
        st.on_chunk = on_chunk;
        st.chunk_ctx = chunk_ctx;
        st.done = 0;

        int rc = client->stream_transport(client->stream_ctx, client->urls[i],
                                          client->api_key, request, sse_on_data, &st);
        int oom = st.pending.oom || st.full.oom;
        free(st.pending.data);

        if (rc != 0 || oom) {
            free(st.full.data);
            last = oom ? MIMIR_ERR_OOM : MIMIR_ERR_TRANSPORT;
            if (oom) break;
            continue; /* failover */
        }
        /* Success: hand back the assembled text (empty string if none). */
        if (!st.full.data) {
            st.full.data = malloc(1);
            if (!st.full.data) {
                last = MIMIR_ERR_OOM;
                break;
            }
            st.full.data[0] = '\0';
        }
        *out_full = st.full.data;
        return MIMIR_OK;
    }
    return last;
}

mimir_status mimir_chat_stream(mimir_client *client,
                               const char *model,
                               const mimir_message *messages,
                               size_t n_messages,
                               mimir_chunk_fn on_chunk,
                               void *chunk_ctx,
                               char **out_full) {
    if (!client || !model || !*model || !messages || n_messages == 0 || !out_full)
        return MIMIR_ERR_INVALID;
    if (!client->stream_transport)
        return MIMIR_ERR_NO_TRANSPORT;

    char *request = build_chat_request_messages(model, messages, n_messages);
    if (!request) return MIMIR_ERR_OOM;
    mimir_status st = do_stream(client, request, on_chunk, chunk_ctx, out_full);
    free(request);
    return st;
}

mimir_status mimir_chat_stream_raw(mimir_client *client,
                                   const char *request_json,
                                   mimir_chunk_fn on_chunk,
                                   void *chunk_ctx,
                                   char **out_full) {
    if (!client || !request_json || !*request_json || !out_full)
        return MIMIR_ERR_INVALID;
    if (!client->stream_transport)
        return MIMIR_ERR_NO_TRANSPORT;
    /* Pure transport: the caller built the whole body (carries multimodal
     * content the message builder can't express). */
    return do_stream(client, request_json, on_chunk, chunk_ctx, out_full);
}

mimir_status mimir_chat(mimir_client *client,
                        const char *model,
                        const char *prompt,
                        char **out_reply) {
    if (!client || !model || !*model || !prompt || !out_reply)
        return MIMIR_ERR_INVALID;
    if (!client->transport)
        return MIMIR_ERR_NO_TRANSPORT;
    char *request = build_chat_request(model, prompt);
    if (!request) return MIMIR_ERR_OOM;
    mimir_status st = do_chat(client, request, out_reply);
    free(request);
    return st;
}

mimir_status mimir_chat_messages(mimir_client *client,
                                 const char *model,
                                 const mimir_message *messages,
                                 size_t n_messages,
                                 char **out_reply) {
    if (!client || !model || !*model || !messages || n_messages == 0 || !out_reply)
        return MIMIR_ERR_INVALID;
    if (!client->transport)
        return MIMIR_ERR_NO_TRANSPORT;
    char *request = build_chat_request_messages(model, messages, n_messages);
    if (!request) return MIMIR_ERR_OOM;
    mimir_status st = do_chat(client, request, out_reply);
    free(request);
    return st;
}

/* Failover loop that hands back the raw transport response (no parsing). */
static mimir_status do_request_raw(mimir_client *client, const char *request,
                                   char **out_error, char **out_raw) {
    char *last_error = NULL; /* the failing transport's message, if any */
    for (size_t i = 0; i < client->n_urls; i++) {
        char *response = NULL;
        int rc = client->transport(client->transport_ctx, client->urls[i],
                                   client->api_key, request, &response);
        if (rc == 0 && response) {
            free(last_error);
            *out_raw = response; /* caller owns + frees */
            return MIMIR_OK;
        }
        /* Failure: `response` now holds the transport's error message (e.g.
         * "http 403"), if it provided one — keep the last for the caller. */
        free(last_error);
        last_error = response;
    }
    if (out_error) {
        *out_error = last_error; /* caller owns + frees */
    } else {
        free(last_error);
    }
    return MIMIR_ERR_TRANSPORT;
}

mimir_status mimir_chat_messages_raw(mimir_client *client,
                                     const char *model,
                                     const mimir_message *messages,
                                     size_t n_messages,
                                     const char *tools_json,
                                     const char *extras,
                                     char **out_response) {
    if (!client || !model || !*model || !messages || n_messages == 0 || !out_response)
        return MIMIR_ERR_INVALID;
    if (!client->transport)
        return MIMIR_ERR_NO_TRANSPORT;
    char *request = build_chat_request_full(model, messages, n_messages, tools_json, extras);
    if (!request) return MIMIR_ERR_OOM;
    mimir_status st = do_request_raw(client, request, NULL, out_response);
    free(request);
    return st;
}

mimir_status mimir_chat_raw(mimir_client *client,
                            const char *request_json,
                            char **out_error,
                            char **out_response) {
    if (!client || !request_json || !*request_json || !out_response)
        return MIMIR_ERR_INVALID;
    if (!client->transport)
        return MIMIR_ERR_NO_TRANSPORT;
    /* Pure transport: the caller built the whole body (Raven does this to
     * carry multimodal content the convenience builder can't express).
     * On all-servers-failed, *out_error (if non-NULL) receives the last
     * transport error message — caller frees. */
    return do_request_raw(client, request_json, out_error, out_response);
}

void mimir_client_free(mimir_client *client) {
    if (!client) return;
    if (client->urls) {
        for (size_t i = 0; i < client->n_urls; i++) free(client->urls[i]);
        free(client->urls);
    }
    free(client->api_key);
    free(client);
}

const char *mimir_status_str(mimir_status status) {
    switch (status) {
        case MIMIR_OK: return "ok";
        case MIMIR_ERR_INVALID: return "invalid argument";
        case MIMIR_ERR_NO_TRANSPORT: return "no transport installed";
        case MIMIR_ERR_TRANSPORT: return "all servers failed";
        case MIMIR_ERR_PROTOCOL: return "unparseable response";
        case MIMIR_ERR_OOM: return "out of memory";
    }
    return "unknown";
}

/* ---- daemon auto-start (FEAT-471) --------------------------------------- */

/* Platform detection for desktop vs mobile. Mobile builds should define
 * MIMIR_MOBILE to disable daemon spawning. */
#ifndef MIMIR_MOBILE
#define MIMIR_DESKTOP 1
#else
#define MIMIR_DESKTOP 0
#endif

/* Default daemon port (matches mimird's default). */
#define MIMIR_DEFAULT_PORT 8700
#define MIMIR_DEFAULT_URL "http://127.0.0.1:8700/v1"

/* Health check endpoint. */
#define MIMIR_HEALTH_PATH "/health"

/* On mobile, daemon spawning is disabled. */
#if MIMIR_DESKTOP

#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <unistd.h>
#include <sys/wait.h>

/* Check if the daemon is responding at /health. */
static int check_health(void) {
    char url[] = MIMIR_DEFAULT_URL MIMIR_HEALTH_PATH;
    /* Use curl for the health check (same transport the CLI uses). */
    char cmd[512];
    snprintf(cmd, sizeof(cmd),
             "curl -sf -o /dev/null -w '%%{http_code}' \"%s\" 2>/dev/null",
             url);
    FILE *p = popen(cmd, "r");
    if (!p) return 0;
    char buf[16] = {0};
    if (!fgets(buf, sizeof(buf), p)) {
        pclose(p);
        return 0;
    }
    pclose(p);
    /* 200 = healthy */
    return strcmp(buf, "200") == 0;
}

int mimir_daemon_is_running(void) {
    return check_health();
}

int mimir_ensure_daemon(void) {
    /* Already running? */
    if (check_health()) return 0;
    
    /* Try to spawn mimird. Look for it on PATH. */
    const char *daemon = "mimird";
    char *path_env = getenv("PATH");
    char daemon_path[1024] = {0};
    
    /* Find mimird on PATH */
    if (path_env) {
        char *path_copy = strdup(path_env);
        char *dir = strtok(path_copy, ":");
        while (dir) {
            snprintf(daemon_path, sizeof(daemon_path), "%s/%s", dir, daemon);
            if (access(daemon_path, X_OK) == 0) break;
            dir = strtok(NULL, ":");
        }
        free(path_copy);
    }
    
    /* Fallback: check common locations */
    if (!daemon_path[0] || access(daemon_path, X_OK) != 0) {
        const char *fallbacks[] = {
            "/usr/local/bin/mimird",
            "/usr/bin/mimird",
            NULL
        };
        for (int i = 0; fallbacks[i]; i++) {
            if (access(fallbacks[i], X_OK) == 0) {
                strncpy(daemon_path, fallbacks[i], sizeof(daemon_path) - 1);
                break;
            }
        }
    }
    
    if (!daemon_path[0] || access(daemon_path, X_OK) != 0) {
        return -1; /* mimird not found */
    }
    
    /* Spawn in background: mimird serve */
    pid_t pid = fork();
    if (pid < 0) return -1;
    if (pid == 0) {
        /* Child: detach and exec */
        setsid();
        execl(daemon_path, "mimird", "serve", (char *)NULL);
        _exit(1);
    }
    
    /* Parent: wait for daemon to become healthy (up to 5 seconds). */
    for (int i = 0; i < 50; i++) {
        usleep(100000); /* 100ms */
        if (check_health()) return 0;
    }
    
    return -1; /* daemon didn't start in time */
}

/* ---- Account management (FEAT-506) --------------------------------------- */

/*
 * Ensure account exists (TOFU registration).
 * POST /api/mimir/v1/account → {"account_id": "...", "balance": ...}
 */
mimir_status mimir_ensure_account(mimir_client *client,
                                   char **out_account_id) {
    if (!client || !client->transport || !out_account_id) {
        return MIMIR_ERR_INVALID;
    }
    *out_account_id = NULL;
    
    /* POST to /api/mimir/v1/account with empty body */
    char url[512];
    snprintf(url, sizeof(url), "%s/api/mimir/v1/account", client->urls[0]);
    
    const char *req = "{}";
    char *resp = NULL;
    int rc = client->transport(client->transport_ctx, url, client->api_key, req, &resp);
    
    if (rc != 0 || !resp) {
        return MIMIR_ERR_TRANSPORT;
    }
    
    /* Parse {"account_id": "..."} */
    const char *key = "\"account_id\"";
    const char *p = strstr(resp, key);
    if (!p) {
        free(resp);
        return MIMIR_ERR_PROTOCOL;
    }
    p += strlen(key);
    while (*p && (*p == ' ' || *p == ':' || *p == '\t' || *p == '\n' || *p == '\r')) p++;
    if (*p != '"') {
        free(resp);
        return MIMIR_ERR_PROTOCOL;
    }
    p++;
    const char *end = p;
    while (*end && *end != '"') {
        if (*end == '\\') end++;
        if (*end) end++;
    }
    if (!*end) {
        free(resp);
        return MIMIR_ERR_PROTOCOL;
    }
    
    size_t len = (size_t)(end - p);
    *out_account_id = (char *)malloc(len + 1);
    if (!*out_account_id) {
        free(resp);
        return MIMIR_ERR_OOM;
    }
    memcpy(*out_account_id, p, len);
    (*out_account_id)[len] = '\0';
    
    free(resp);
    return MIMIR_OK;
}

/*
 * Get account balance.
 * GET /api/mimir/v1/account/balance → {"balance_sats": ...}
 */
mimir_status mimir_get_balance(mimir_client *client,
                                int64_t *out_balance) {
    if (!client || !client->transport || !out_balance) {
        return MIMIR_ERR_INVALID;
    }
    *out_balance = 0;
    
    /* GET /api/mimir/v1/account/balance */
    char url[512];
    snprintf(url, sizeof(url), "%s/api/mimir/v1/account/balance", client->urls[0]);
    
    char *resp = NULL;
    int rc = client->transport(client->transport_ctx, url, client->api_key, "", &resp);
    
    if (rc != 0 || !resp) {
        return MIMIR_ERR_TRANSPORT;
    }
    
    /* Parse {"balance_sats": ...} */
    const char *key = "\"balance_sats\"";
    const char *p = strstr(resp, key);
    if (!p) {
        free(resp);
        return MIMIR_ERR_PROTOCOL;
    }
    p += strlen(key);
    while (*p && (*p == ' ' || *p == ':' || *p == '\t' || *p == '\n' || *p == '\r')) p++;
    
    /* Parse integer */
    char *end = NULL;
    long long val = strtoll(p, &end, 10);
    if (end == p) {
        free(resp);
        return MIMIR_ERR_PROTOCOL;
    }
    
    *out_balance = (int64_t)val;
    free(resp);
    return MIMIR_OK;
}

/*
 * Create Lightning invoice.
 * POST /api/mimir/v1/account/invoice → {"invoice": "..."}
 */
mimir_status mimir_create_invoice(mimir_client *client,
                                   int64_t sats,
                                   const char *description,
                                   char **out_invoice) {
    if (!client || !client->transport || !out_invoice || sats <= 0) {
        return MIMIR_ERR_INVALID;
    }
    *out_invoice = NULL;
    
    /* Build request: {"sats": N, "description": "..."} */
    sbuf b;
    sbuf_init(&b);
    sbuf_puts(&b, "{\"sats\":");
    char buf[32];
    snprintf(buf, sizeof(buf), "%lld", (long long)sats);
    sbuf_puts(&b, buf);
    if (description && *description) {
        sbuf_puts(&b, ",\"description\":\"");
        sbuf_put_json_escaped(&b, description);
        sbuf_puts(&b, "\"");
    }
    sbuf_puts(&b, "}");
    if (b.oom) {
        free(b.data);
        return MIMIR_ERR_OOM;
    }
    
    /* POST /api/mimir/v1/account/invoice */
    char url[512];
    snprintf(url, sizeof(url), "%s/api/mimir/v1/account/invoice", client->urls[0]);
    
    char *resp = NULL;
    int rc = client->transport(client->transport_ctx, url, client->api_key, b.data, &resp);
    free(b.data);
    
    if (rc != 0 || !resp) {
        return MIMIR_ERR_TRANSPORT;
    }
    
    /* Parse {"invoice": "..."} */
    const char *key = "\"invoice\"";
    const char *p = strstr(resp, key);
    if (!p) {
        /* Try error response: {"error": "..."} */
        const char *err_key = "\"error\"";
        const char *err_p = strstr(resp, err_key);
        if (err_p) {
            free(resp);
            return MIMIR_ERR_PROTOCOL;
        }
        free(resp);
        return MIMIR_ERR_PROTOCOL;
    }
    p += strlen(key);
    while (*p && (*p == ' ' || *p == ':' || *p == '\t' || *p == '\n' || *p == '\r')) p++;
    if (*p != '"') {
        free(resp);
        return MIMIR_ERR_PROTOCOL;
    }
    p++;
    const char *end = p;
    while (*end && *end != '"') {
        if (*end == '\\') end++;
        if (*end) end++;
    }
    if (!*end) {
        free(resp);
        return MIMIR_ERR_PROTOCOL;
    }
    
    size_t len = (size_t)(end - p);
    *out_invoice = (char *)malloc(len + 1);
    if (!*out_invoice) {
        free(resp);
        return MIMIR_ERR_OOM;
    }
    memcpy(*out_invoice, p, len);
    (*out_invoice)[len] = '\0';
    
    free(resp);
    return MIMIR_OK;
}

#else /* MIMIR_MOBILE */

int mimir_daemon_is_running(void) {
    return 0; /* No local daemon on mobile */
}

int mimir_ensure_daemon(void) {
    return 0; /* No-op on mobile */
}

#endif
