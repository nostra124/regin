/* libmimir unit tests — exercise the core against a stub transport, so the
 * marshalling / failover / parsing logic is verified with zero network. */
#define _POSIX_C_SOURCE 200809L /* expose strdup for the stub */
#include "mimir.h"

#include <stdio.h>
#include <stdlib.h>
#include <string.h>

static int failures = 0;
#define CHECK(cond, msg)                                                   \
    do {                                                                   \
        if (!(cond)) {                                                     \
            printf("  FAIL: %s\n", msg);                                   \
            failures++;                                                    \
        }                                                                  \
    } while (0)

/* A scriptable transport: fails its first `fail_first_n` calls, then
 * returns `response`. Captures the last url + request for assertions. */
typedef struct {
    int calls;
    int fail_first_n;
    const char *response;
    char last_url[512];
    char last_request[4096];
} stub;

static int stub_transport(void *ctx, const char *url, const char *api_key,
                          const char *request_json, char **response_json) {
    stub *s = (stub *)ctx;
    (void)api_key;
    s->calls++;
    snprintf(s->last_url, sizeof s->last_url, "%s", url);
    snprintf(s->last_request, sizeof s->last_request, "%s", request_json);
    if (s->calls <= s->fail_first_n) return 1; /* simulate a dead server */
    *response_json = strdup(s->response);
    return *response_json ? 0 : 1;
}

/* A transport that fails but hands back an error message (e.g. an upstream
 * 403), to exercise error propagation through out_error. */
static int erroring_transport(void *ctx, const char *url, const char *api_key,
                              const char *request_json, char **response_json) {
    (void)ctx;
    (void)url;
    (void)api_key;
    (void)request_json;
    *response_json = strdup("http 403 Forbidden");
    return 1;
}

static const char *OK_BODY =
    "{\"choices\":[{\"message\":{\"role\":\"assistant\","
    "\"content\":\"Hello from Mimir\"}}]}";

static void test_basic_chat(void) {
    printf("test_basic_chat\n");
    const char *urls[] = {"https://h/api/mimir/v1"};
    mimir_client *c = mimir_client_new(urls, 1, "rvn_key");
    CHECK(c != NULL, "client created");
    CHECK(mimir_client_server_count(c) == 1, "one server");

    stub s = {0};
    s.response = OK_BODY;
    mimir_client_set_transport(c, stub_transport, &s);

    char *reply = NULL;
    mimir_status st = mimir_chat(c, "gpt-x", "Hi there", &reply);
    CHECK(st == MIMIR_OK, "chat ok");
    CHECK(reply && strcmp(reply, "Hello from Mimir") == 0, "reply content");
    CHECK(s.calls == 1, "one transport call");
    CHECK(strstr(s.last_url, "/chat/completions") != NULL, "endpoint joined");
    CHECK(strstr(s.last_request, "\"model\":\"gpt-x\"") != NULL, "model in body");
    CHECK(strstr(s.last_request, "Hi there") != NULL, "prompt in body");

    free(reply);
    mimir_client_free(c);
}

static void test_json_escaping(void) {
    printf("test_json_escaping\n");
    const char *urls[] = {"https://h/api/mimir/v1/"}; /* trailing slash */
    mimir_client *c = mimir_client_new(urls, 1, NULL);
    stub s = {0};
    s.response = OK_BODY;
    mimir_client_set_transport(c, stub_transport, &s);

    char *reply = NULL;
    mimir_status st = mimir_chat(c, "m", "say \"hi\"\nthen stop", &reply);
    CHECK(st == MIMIR_OK, "chat ok");
    CHECK(strstr(s.last_request, "\\\"hi\\\"") != NULL, "quotes escaped");
    CHECK(strstr(s.last_request, "\\n") != NULL, "newline escaped");
    /* trailing slash collapsed, not doubled */
    CHECK(strstr(s.last_url, "v1//chat") == NULL, "no double slash");
    CHECK(strstr(s.last_url, "v1/chat/completions") != NULL, "endpoint joined");

    free(reply);
    mimir_client_free(c);
}

static void test_failover(void) {
    printf("test_failover\n");
    const char *urls[] = {"https://a/api/mimir/v1", "https://b/api/mimir/v1"};
    mimir_client *c = mimir_client_new(urls, 2, NULL);
    CHECK(mimir_client_server_count(c) == 2, "two servers");
    stub s = {0};
    s.fail_first_n = 1; /* first server dead, second answers */
    s.response = OK_BODY;
    mimir_client_set_transport(c, stub_transport, &s);

    char *reply = NULL;
    mimir_status st = mimir_chat(c, "m", "hi", &reply);
    CHECK(st == MIMIR_OK, "recovered on second server");
    CHECK(s.calls == 2, "tried both servers");
    CHECK(reply && strcmp(reply, "Hello from Mimir") == 0, "reply content");
    CHECK(strstr(s.last_url, "https://b/") != NULL, "last call hit server b");

    free(reply);
    mimir_client_free(c);
}

static void test_all_servers_fail(void) {
    printf("test_all_servers_fail\n");
    const char *urls[] = {"https://a/v1", "https://b/v1"};
    mimir_client *c = mimir_client_new(urls, 2, NULL);
    stub s = {0};
    s.fail_first_n = 99; /* everything fails */
    mimir_client_set_transport(c, stub_transport, &s);

    char *reply = NULL;
    mimir_status st = mimir_chat(c, "m", "hi", &reply);
    CHECK(st == MIMIR_ERR_TRANSPORT, "all-fail → transport error");
    CHECK(s.calls == 2, "tried every server");
    CHECK(reply == NULL, "no reply allocated on failure");
    mimir_client_free(c);
}

static void test_no_transport(void) {
    printf("test_no_transport\n");
    const char *urls[] = {"https://a/v1"};
    mimir_client *c = mimir_client_new(urls, 1, NULL);
    char *reply = NULL;
    mimir_status st = mimir_chat(c, "m", "hi", &reply);
    CHECK(st == MIMIR_ERR_NO_TRANSPORT, "no transport → error");
    mimir_client_free(c);
}

static void test_unparseable_response(void) {
    printf("test_unparseable_response\n");
    const char *urls[] = {"https://a/v1"};
    mimir_client *c = mimir_client_new(urls, 1, NULL);
    stub s = {0};
    s.response = "{\"error\":\"nope\"}"; /* no content field */
    mimir_client_set_transport(c, stub_transport, &s);
    char *reply = NULL;
    mimir_status st = mimir_chat(c, "m", "hi", &reply);
    CHECK(st == MIMIR_ERR_PROTOCOL, "missing content → protocol error");
    mimir_client_free(c);
}

static void test_unescapes_reply(void) {
    printf("test_unescapes_reply\n");
    const char *urls[] = {"https://a/v1"};
    mimir_client *c = mimir_client_new(urls, 1, NULL);
    stub s = {0};
    s.response = "{\"choices\":[{\"message\":{\"content\":\"line1\\nline2\\t!\"}}]}";
    mimir_client_set_transport(c, stub_transport, &s);
    char *reply = NULL;
    mimir_status st = mimir_chat(c, "m", "hi", &reply);
    CHECK(st == MIMIR_OK, "chat ok");
    CHECK(reply && strcmp(reply, "line1\nline2\t!") == 0, "escapes decoded");
    free(reply);
    mimir_client_free(c);
}

static void test_chat_messages(void) {
    printf("test_chat_messages\n");
    const char *urls[] = {"https://h/api/mimir/v1"};
    mimir_client *c = mimir_client_new(urls, 1, NULL);
    stub s = {0};
    s.response = OK_BODY;
    mimir_client_set_transport(c, stub_transport, &s);

    mimir_message msgs[] = {
        {"system", "be terse"},
        {"user", "ping"},
        {NULL, "implicit-user"}, /* NULL role defaults to "user" */
    };
    char *reply = NULL;
    mimir_status st = mimir_chat_messages(c, "m", msgs, 3, &reply);
    CHECK(st == MIMIR_OK, "messages chat ok");
    CHECK(reply && strcmp(reply, "Hello from Mimir") == 0, "reply content");
    CHECK(strstr(s.last_request, "\"role\":\"system\"") != NULL, "system role in body");
    CHECK(strstr(s.last_request, "be terse") != NULL, "system content in body");
    CHECK(strstr(s.last_request, "\"role\":\"user\",\"content\":\"ping\"") != NULL,
          "user turn in body");
    /* the NULL-role message defaulted to user */
    CHECK(strstr(s.last_request, "implicit-user") != NULL, "implicit user content");

    free(reply);
    mimir_client_free(c);

    /* zero messages is invalid */
    mimir_client *c2 = mimir_client_new(urls, 1, NULL);
    mimir_client_set_transport(c2, stub_transport, &s);
    char *r2 = NULL;
    CHECK(mimir_chat_messages(c2, "m", msgs, 0, &r2) == MIMIR_ERR_INVALID,
          "zero messages rejected");
    mimir_client_free(c2);
}

static void test_chat_messages_raw(void) {
    printf("test_chat_messages_raw\n");
    const char *urls[] = {"https://h/api/mimir/v1"};
    mimir_client *c = mimir_client_new(urls, 1, NULL);
    stub s = {0};
    /* a tool-call response shape — libmimir returns it verbatim */
    s.response =
        "{\"choices\":[{\"message\":{\"tool_calls\":[{\"id\":\"t1\",\"function\":"
        "{\"name\":\"f\",\"arguments\":\"{}\"}}]}}]}";
    mimir_client_set_transport(c, stub_transport, &s);

    mimir_message msgs[] = {{"user", "do it"}};
    const char *tools = "[{\"type\":\"function\",\"function\":{\"name\":\"f\"}}]";
    const char *extras = "\"raven_hints\":{\"task_hint\":\"cheap\"}";
    char *raw = NULL;
    mimir_status st = mimir_chat_messages_raw(c, "m", msgs, 1, tools, extras, &raw);
    CHECK(st == MIMIR_OK, "raw chat ok");
    /* the raw body is returned unparsed */
    CHECK(raw && strstr(raw, "tool_calls") != NULL, "raw response passed through");
    /* the tools array was embedded verbatim into the request */
    CHECK(strstr(s.last_request, "\"tools\":[{\"type\":\"function\"") != NULL,
          "tools embedded in request");
    /* the extras fragment was embedded verbatim into the request */
    CHECK(strstr(s.last_request, "\"raven_hints\":{\"task_hint\":\"cheap\"}") != NULL,
          "extras embedded in request");
    CHECK(strstr(s.last_request, "\"messages\":[{\"role\":\"user\"") != NULL,
          "messages still present");

    free(raw);
    mimir_client_free(c);

    /* tools_json = NULL + extras = NULL → neither key */
    mimir_client *c2 = mimir_client_new(urls, 1, NULL);
    stub s2 = {0};
    s2.response = OK_BODY;
    mimir_client_set_transport(c2, stub_transport, &s2);
    char *raw2 = NULL;
    mimir_chat_messages_raw(c2, "m", msgs, 1, NULL, NULL, &raw2);
    CHECK(strstr(s2.last_request, "tools") == NULL, "no tools key when NULL");
    CHECK(strstr(s2.last_request, "raven_hints") == NULL, "no extras key when NULL");
    free(raw2);
    mimir_client_free(c2);
}

static void test_chat_raw(void) {
    printf("test_chat_raw\n");
    const char *urls[] = {"https://h/api/mimir/v1"};
    mimir_client *c = mimir_client_new(urls, 1, NULL);
    stub s = {0};
    s.response = OK_BODY;
    mimir_client_set_transport(c, stub_transport, &s);

    /* A multimodal body the convenience builder couldn't express. */
    const char *body =
        "{\"model\":\"m\",\"messages\":[{\"role\":\"user\",\"content\":"
        "[{\"type\":\"text\",\"text\":\"hi\"},{\"type\":\"image_url\",\"image_url\":{\"url\":\"x\"}}]}]}";
    char *raw = NULL;
    char *err = NULL;
    mimir_status st = mimir_chat_raw(c, body, &err, &raw);
    CHECK(st == MIMIR_OK, "raw chat ok");
    CHECK(raw != NULL, "got response");
    CHECK(err == NULL, "no error on success");
    /* the body was sent verbatim — including the multimodal content array */
    CHECK(strcmp(s.last_request, body) == 0, "body sent verbatim");
    free(raw);
    mimir_client_free(c);

    /* a failing transport's message is captured into out_error */
    mimir_client *c3 = mimir_client_new(urls, 1, NULL);
    mimir_client_set_transport(c3, erroring_transport, NULL);
    char *err3 = NULL, *raw3 = NULL;
    mimir_status est = mimir_chat_raw(c3, "{}", &err3, &raw3);
    CHECK(est == MIMIR_ERR_TRANSPORT, "transport failure status");
    CHECK(err3 && strstr(err3, "403") != NULL, "error message captured");
    free(err3);
    mimir_client_free(c3);

    /* invalid args + missing transport */
    mimir_client *c2 = mimir_client_new(urls, 1, NULL);
    char *r2 = NULL;
    CHECK(mimir_chat_raw(c2, "{}", NULL, &r2) == MIMIR_ERR_NO_TRANSPORT, "no transport → error");
    CHECK(mimir_chat_raw(c2, NULL, NULL, &r2) == MIMIR_ERR_INVALID, "null body → invalid");
    mimir_client_free(c2);
}

/* ---- streaming ---- */

static const char *SSE_BODY =
    "data: {\"choices\":[{\"delta\":{\"content\":\"Hel\"}}]}\n\n"
    "data: {\"choices\":[{\"delta\":{\"content\":\"lo\"}}]}\n\n"
    "data: [DONE]\n\n";

typedef struct {
    char buf[256];
} chunk_collector;

static void collect_chunk(void *ctx, const char *delta) {
    chunk_collector *c = (chunk_collector *)ctx;
    strncat(c->buf, delta, sizeof(c->buf) - strlen(c->buf) - 1);
}

/* Feeds SSE_BODY to on_data in pieces of `split` bytes (0 = all at once). */
typedef struct {
    const char *sse;
    size_t split;
} stream_stub;

static int stream_transport(void *ctx, const char *url, const char *api_key,
                            const char *request, mimir_on_data_fn on_data,
                            void *od_ctx) {
    (void)url;
    (void)api_key;
    (void)request;
    stream_stub *s = (stream_stub *)ctx;
    size_t len = strlen(s->sse);
    size_t chunk = s->split ? s->split : len;
    for (size_t off = 0; off < len; off += chunk) {
        size_t n = (off + chunk <= len) ? chunk : len - off;
        if (on_data(od_ctx, s->sse + off, n) != 0) break;
    }
    return 0;
}

static void run_stream_case(size_t split, const char *label) {
    printf("test_stream(%s)\n", label);
    const char *urls[] = {"https://h/api/mimir/v1"};
    mimir_client *c = mimir_client_new(urls, 1, NULL);
    stream_stub s = {SSE_BODY, split};
    mimir_client_set_stream_transport(c, stream_transport, &s);

    mimir_message msgs[] = {{"user", "hi"}};
    chunk_collector cc;
    cc.buf[0] = '\0';
    char *full = NULL;
    mimir_status st = mimir_chat_stream(c, "m", msgs, 1, collect_chunk, &cc, &full);
    CHECK(st == MIMIR_OK, "stream ok");
    CHECK(full && strcmp(full, "Hello") == 0, "assembled full reply");
    CHECK(strcmp(cc.buf, "Hello") == 0, "deltas delivered in order");

    free(full);
    mimir_client_free(c);
}

static void test_streaming(void) {
    run_stream_case(0, "whole");      /* one chunk */
    run_stream_case(1, "byte");       /* byte-by-byte → partial-frame buffering */
    run_stream_case(7, "split");      /* arbitrary mid-frame splits */

    /* raw passthrough: a fully-formed (e.g. multimodal) body streams the
     * same SSE machinery; the stub ignores the body, so reuse SSE_BODY. */
    printf("test_stream(raw)\n");
    const char *u2[] = {"https://h/api/mimir/v1"};
    mimir_client *cr = mimir_client_new(u2, 1, NULL);
    stream_stub sr = {SSE_BODY, 0};
    mimir_client_set_stream_transport(cr, stream_transport, &sr);
    chunk_collector cc;
    cc.buf[0] = '\0';
    char *rfull = NULL;
    const char *raw_body =
        "{\"model\":\"m\",\"messages\":[{\"role\":\"user\",\"content\":"
        "[{\"type\":\"image_url\",\"image_url\":{\"url\":\"x\"}}]}],\"stream\":true}";
    CHECK(mimir_chat_stream_raw(cr, raw_body, collect_chunk, &cc, &rfull) == MIMIR_OK,
          "raw stream ok");
    CHECK(rfull && strcmp(rfull, "Hello") == 0, "raw stream assembled reply");
    free(rfull);
    mimir_client_free(cr);

    /* no stream transport installed */
    const char *urls[] = {"https://h/v1"};
    mimir_client *c = mimir_client_new(urls, 1, NULL);
    mimir_message msgs[] = {{"user", "hi"}};
    char *full = NULL;
    CHECK(mimir_chat_stream(c, "m", msgs, 1, NULL, NULL, &full) == MIMIR_ERR_NO_TRANSPORT,
          "no stream transport → error");
    CHECK(mimir_chat_stream_raw(c, "{}", NULL, NULL, &full) == MIMIR_ERR_NO_TRANSPORT,
          "raw: no stream transport → error");
    mimir_client_free(c);
}

static void test_invalid_args(void) {
    printf("test_invalid_args\n");
    CHECK(mimir_client_new(NULL, 1, NULL) == NULL, "NULL urls rejected");
    CHECK(mimir_client_new((const char *const[]){"x"}, 0, NULL) == NULL,
          "zero count rejected");
    const char *bad[] = {""};
    CHECK(mimir_client_new(bad, 1, NULL) == NULL, "empty url rejected");
    CHECK(mimir_client_server_count(NULL) == 0, "count(NULL)==0");
}

/* ---- Account management tests (FEAT-506) ---- */

static void test_ensure_account(void) {
    printf("test_ensure_account\n");
    const char *urls[] = {"https://h/api/mimir/v1"};
    mimir_client *c = mimir_client_new(urls, 1, NULL);
    CHECK(c != NULL, "client created");
    
    stub s = {0};
    s.response = "{\"account_id\":\"abc123\",\"balance\":0}";
    mimir_client_set_transport(c, stub_transport, &s);
    
    char *account_id = NULL;
    mimir_status st = mimir_ensure_account(c, &account_id);
    CHECK(st == MIMIR_OK, "ensure_account ok");
    CHECK(account_id && strcmp(account_id, "abc123") == 0, "account_id returned");
    CHECK(strstr(s.last_url, "/api/mimir/v1/account") != NULL, "correct endpoint");
    
    free(account_id);
    mimir_client_free(c);
}

static void test_get_balance(void) {
    printf("test_get_balance\n");
    const char *urls[] = {"https://h/api/mimir/v1"};
    mimir_client *c = mimir_client_new(urls, 1, NULL);
    CHECK(c != NULL, "client created");
    
    stub s = {0};
    s.response = "{\"account_id\":\"abc123\",\"balance_sats\":50000}";
    mimir_client_set_transport(c, stub_transport, &s);
    
    int64_t balance = 0;
    mimir_status st = mimir_get_balance(c, &balance);
    CHECK(st == MIMIR_OK, "get_balance ok");
    CHECK(balance == 50000, "balance returned");
    CHECK(strstr(s.last_url, "/api/mimir/v1/account/balance") != NULL, "correct endpoint");
    
    mimir_client_free(c);
}

static void test_create_invoice(void) {
    printf("test_create_invoice\n");
    const char *urls[] = {"https://h/api/mimir/v1"};
    mimir_client *c = mimir_client_new(urls, 1, NULL);
    CHECK(c != NULL, "client created");
    
    stub s = {0};
    s.response = "{\"invoice\":\"lnbc50000...\"}";
    mimir_client_set_transport(c, stub_transport, &s);
    
    char *invoice = NULL;
    mimir_status st = mimir_create_invoice(c, 50000, "test top-up", &invoice);
    CHECK(st == MIMIR_OK, "create_invoice ok");
    CHECK(invoice && strncmp(invoice, "lnbc", 4) == 0, "invoice returned");
    CHECK(strstr(s.last_url, "/api/mimir/v1/account/invoice") != NULL, "correct endpoint");
    CHECK(strstr(s.last_request, "\"sats\":50000") != NULL, "sats in request");
    CHECK(strstr(s.last_request, "\"description\":\"test top-up\"") != NULL, "description in request");
    
    free(invoice);
    mimir_client_free(c);
}

static void test_create_invoice_no_description(void) {
    printf("test_create_invoice_no_description\n");
    const char *urls[] = {"https://h/api/mimir/v1"};
    mimir_client *c = mimir_client_new(urls, 1, NULL);
    CHECK(c != NULL, "client created");
    
    stub s = {0};
    s.response = "{\"invoice\":\"lnbc1000...\"}";
    mimir_client_set_transport(c, stub_transport, &s);
    
    char *invoice = NULL;
    mimir_status st = mimir_create_invoice(c, 1000, NULL, &invoice);
    CHECK(st == MIMIR_OK, "create_invoice ok");
    CHECK(invoice && strncmp(invoice, "lnbc", 4) == 0, "invoice returned");
    CHECK(strstr(s.last_request, "\"sats\":1000") != NULL, "sats in request");
    CHECK(strstr(s.last_request, "\"description\"") == NULL, "no description in request");
    
    free(invoice);
    mimir_client_free(c);
}

static void test_create_invoice_invalid_amount(void) {
    printf("test_create_invoice_invalid_amount\n");
    const char *urls[] = {"https://h/api/mimir/v1"};
    mimir_client *c = mimir_client_new(urls, 1, NULL);
    CHECK(c != NULL, "client created");
    
    char *invoice = NULL;
    mimir_status st = mimir_create_invoice(c, 0, "test", &invoice);
    CHECK(st == MIMIR_ERR_INVALID, "zero amount rejected");
    
    st = mimir_create_invoice(c, -100, "test", &invoice);
    CHECK(st == MIMIR_ERR_INVALID, "negative amount rejected");
    
    mimir_client_free(c);
}

int main(void) {
    test_basic_chat();
    test_json_escaping();
    test_failover();
    test_all_servers_fail();
    test_no_transport();
    test_unparseable_response();
    test_unescapes_reply();
    test_chat_messages();
    test_chat_messages_raw();
    test_chat_raw();
    test_streaming();
    test_invalid_args();
    test_ensure_account();
    test_get_balance();
    test_create_invoice();
    test_create_invoice_no_description();
    test_create_invoice_invalid_amount();

    if (failures) {
        printf("\n%d check(s) FAILED\n", failures);
        return 1;
    }
    printf("\nall libmimir checks passed\n");
    return 0;
}
