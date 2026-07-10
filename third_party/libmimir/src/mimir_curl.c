/*
 * libcurl transports for libmimir (optional; see include/mimir_curl.h).
 *
 * VERIFY-ON-HOST: requires libcurl-dev to compile and a reachable server
 * to exercise. The libmimir core these plug into is unit-tested with a
 * stub transport, so only this thin libcurl glue is host-verified.
 */
#include "mimir_curl.h"

#include <stdlib.h>
#include <string.h>

#include <curl/curl.h>

/* Build the common request headers (content-type + optional bearer). */
static struct curl_slist *build_headers(const char *api_key) {
    struct curl_slist *h = NULL;
    h = curl_slist_append(h, "Content-Type: application/json");
    if (api_key && *api_key) {
        size_t n = strlen("Authorization: Bearer ") + strlen(api_key) + 1;
        char *auth = malloc(n);
        if (auth) {
            snprintf(auth, n, "Authorization: Bearer %s", api_key);
            h = curl_slist_append(h, auth);
            free(auth);
        }
    }
    return h;
}

/* ---- unary ------------------------------------------------------------- */

typedef struct {
    char *data;
    size_t len;
    int oom;
} accum;

static size_t accum_write(char *ptr, size_t size, size_t nmemb, void *userdata) {
    accum *a = (accum *)userdata;
    size_t total = size * nmemb;
    char *p = realloc(a->data, a->len + total + 1);
    if (!p) {
        a->oom = 1;
        return 0; /* signal error to curl */
    }
    a->data = p;
    memcpy(a->data + a->len, ptr, total);
    a->len += total;
    a->data[a->len] = '\0';
    return total;
}

int mimir_curl_transport(void *ctx, const char *url, const char *api_key,
                         const char *request_json, char **response_json) {
    (void)ctx;
    CURL *curl = curl_easy_init();
    if (!curl) return 1;

    accum a = {NULL, 0, 0};
    struct curl_slist *headers = build_headers(api_key);

    curl_easy_setopt(curl, CURLOPT_URL, url);
    curl_easy_setopt(curl, CURLOPT_POST, 1L);
    curl_easy_setopt(curl, CURLOPT_POSTFIELDS, request_json);
    curl_easy_setopt(curl, CURLOPT_HTTPHEADER, headers);
    curl_easy_setopt(curl, CURLOPT_WRITEFUNCTION, accum_write);
    curl_easy_setopt(curl, CURLOPT_WRITEDATA, &a);

    CURLcode rc = curl_easy_perform(curl);
    long status = 0;
    curl_easy_getinfo(curl, CURLINFO_RESPONSE_CODE, &status);

    curl_slist_free_all(headers);
    curl_easy_cleanup(curl);

    if (rc != CURLE_OK || a.oom || status < 200 || status >= 300) {
        free(a.data);
        return 1; /* fail this server → libmimir fails over */
    }
    *response_json = a.data; /* caller (libmimir) frees */
    return 0;
}

/* ---- streaming --------------------------------------------------------- */

typedef struct {
    mimir_on_data_fn on_data;
    void *on_data_ctx;
    int stop;
} stream_sink;

static size_t stream_write(char *ptr, size_t size, size_t nmemb, void *userdata) {
    stream_sink *s = (stream_sink *)userdata;
    size_t total = size * nmemb;
    if (s->stop) return 0;
    if (s->on_data(s->on_data_ctx, ptr, total) != 0) {
        s->stop = 1;
        return 0; /* abort the transfer */
    }
    return total;
}

int mimir_curl_stream_transport(void *ctx, const char *url, const char *api_key,
                                const char *request_json,
                                mimir_on_data_fn on_data, void *on_data_ctx) {
    (void)ctx;
    CURL *curl = curl_easy_init();
    if (!curl) return 1;

    stream_sink s = {on_data, on_data_ctx, 0};
    struct curl_slist *headers = build_headers(api_key);
    headers = curl_slist_append(headers, "Accept: text/event-stream");

    curl_easy_setopt(curl, CURLOPT_URL, url);
    curl_easy_setopt(curl, CURLOPT_POST, 1L);
    curl_easy_setopt(curl, CURLOPT_POSTFIELDS, request_json);
    curl_easy_setopt(curl, CURLOPT_HTTPHEADER, headers);
    curl_easy_setopt(curl, CURLOPT_WRITEFUNCTION, stream_write);
    curl_easy_setopt(curl, CURLOPT_WRITEDATA, &s);

    CURLcode rc = curl_easy_perform(curl);
    long status = 0;
    curl_easy_getinfo(curl, CURLINFO_RESPONSE_CODE, &status);

    curl_slist_free_all(headers);
    curl_easy_cleanup(curl);

    /* A caller-requested early stop (s.stop) is success, not failure. */
    if (s.stop) return 0;
    if (rc != CURLE_OK || status < 200 || status >= 300) return 1;
    return 0;
}

int mimir_curl_install(mimir_client *client) {
    if (!client) return 1;
    static int inited = 0;
    if (!inited) {
        if (curl_global_init(CURL_GLOBAL_DEFAULT) != CURLE_OK) return 1;
        inited = 1;
    }
    mimir_client_set_transport(client, mimir_curl_transport, NULL);
    mimir_client_set_stream_transport(client, mimir_curl_stream_transport, NULL);
    return 0;
}
