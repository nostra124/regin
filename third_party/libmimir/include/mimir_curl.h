/*
 * libcurl-backed transports for libmimir — for standalone (non-Rust) C
 * embedders that want a ready HTTP/TLS transport. The core (mimir.c) stays
 * dependency-free; this optional module links libcurl.
 *
 * Build:
 *   cc ... src/mimir.c src/mimir_curl.c $(pkg-config --cflags --libs libcurl)
 * or `make curl` (gated on `pkg-config --exists libcurl`).
 *
 * VERIFY-ON-HOST: this module requires libcurl-dev to compile and a
 * reachable server to exercise; the libmimir core it plugs into is fully
 * unit-tested with a stub transport.
 */
#ifndef LIBMIMIR_MIMIR_CURL_H
#define LIBMIMIR_MIMIR_CURL_H

#include "mimir.h"

#ifdef __cplusplus
extern "C" {
#endif

/*
 * Install libcurl-backed unary + streaming transports on `client` (and
 * run curl_global_init once). After this, mimir_chat* and
 * mimir_chat_stream work over real HTTP. Returns 0 on success.
 */
int mimir_curl_install(mimir_client *client);

/* The unary transport (mimir_transport_fn shape). `ctx` is unused. */
int mimir_curl_transport(void *ctx,
                         const char *url,
                         const char *api_key,
                         const char *request_json,
                         char **response_json);

/* The streaming transport (mimir_stream_transport_fn shape). */
int mimir_curl_stream_transport(void *ctx,
                                const char *url,
                                const char *api_key,
                                const char *request_json,
                                mimir_on_data_fn on_data,
                                void *on_data_ctx);

#ifdef __cplusplus
}
#endif

#endif /* LIBMIMIR_MIMIR_CURL_H */
