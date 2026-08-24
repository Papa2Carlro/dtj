#include "dtj/dtj.h"
#include <stdlib.h>
#include <string.h>
#include <unistd.h>
#include <arpa/inet.h>
#include <pthread.h>
#include <sys/socket.h>
#include <sys/un.h>

#define DTJ_MAX_FRAME_SIZE 1048576
#define DTJ_PROTOCOL_VERSION 1

typedef struct dtj_session_internal {
    int sock_fd;
    dtj_discovery *discovery;
    uint8_t session_id[16];
    uint64_t mono_origin_ns;
    int closed;
    int disabled;
    
    /* Dictionary caches - simple hash map for MVP */
    struct {
        char **keys;
        uint32_t *values;
        size_t count;
        size_t capacity;
    } domain_cache, category_cache, event_name_cache, string_cache;
    
    void (*warning_handler)(const char *, void *);
    void *warning_user_data;
    int warning_emitted;
    
    pthread_mutex_t mutex;
} dtj_session_internal;

/* Forward declarations */
static int dtj_session_connect(const char *socket_path, int *out_fd);
static int dtj_session_send_frame(int fd, const uint8_t *frame, size_t len);
static int dtj_session_read_frame(int fd, uint8_t **out_frame, size_t *out_len);
static uint32_t dtj_session_get_or_intern(dtj_session_internal *s, uint8_t kind, const char *name,
                                           struct { char **keys; uint32_t *values; size_t count; size_t capacity; } *cache);

dtj_session *dtj_open(const dtj_config *config) {
    if (!config || !config->producer_name || !config->producer_version) return NULL;

    dtj_error err;
    dtj_session *sess = dtj_open_strict(config, &err);
    if (!sess && err.code == DTJ_ERROR_AGENT_NOT_FOUND) {
        /* Return disabled session on agent unavailable */
        dtj_session_internal *s = calloc(1, sizeof(dtj_session_internal));
        if (!s) return NULL;
        s->disabled = 1;
        s->warning_handler = config->warning_handler;
        s->warning_user_data = config->warning_user_data;
        pthread_mutex_init(&s->mutex, NULL);
        return (dtj_session*)s;
    }
    return sess;
}

dtj_session *dtj_open_strict(const dtj_config *config, dtj_error *out_error) {
    if (!config || !config->producer_name || !config->producer_version) {
        if (out_error) dtj_error_init(out_error, DTJ_ERROR_VALUE, "Missing required config fields");
        return NULL;
    }

    int enabled = config->enabled ? 1 : 0;
    if (!enabled) {
        dtj_session_internal *s = calloc(1, sizeof(dtj_session_internal));
        if (!s) { if (out_error) dtj_error_init(out_error, DTJ_ERROR_VALUE, "OOM"); return NULL; }
        s->disabled = 1;
        s->warning_handler = config->warning_handler;
        s->warning_user_data = config->warning_user_data;
        pthread_mutex_init(&s->mutex, NULL);
        return (dtj_session*)s;
    }

    /* Default warning handler */
    void (*warn_handler)(const char *, void *) = config->warning_handler 
        ? config->warning_handler 
        : [](const char *msg, void *ud) { fprintf(stderr, "dtj warning: %s\n", msg); };
    
    void *warn_ud = config->warning_user_data;

    /* Initialize discovery */
    const char *session_file_name = config->session_file_name ? config->session_file_name : "";
    
    dtj_discovery *disc = NULL;
    char socket_path[PATH_MAX];
    
    int ret = dtj_discovery_start(config, session_file_name, &disc, socket_path, sizeof(socket_path), out_error);
    if (ret != DTJ_OK) return NULL;

    /* Connect to socket */
    int sock_fd = -1;
    ret = dtj_session_connect(socket_path, &sock_fd);
    if (ret != DTJ_OK) {
        dtj_discovery_stop(disc);
        if (out_error) dtj_error_init(out_error, DTJ_ERROR_CONNECTION, "Failed to connect to agent");
        return NULL;
    }

    /* Hello handshake */
    size_t hello_len = 0;
    uint8_t *hello_frame = dtj_encode_hello(&hello_len);
    if (!hello_frame || dtj_session_send_frame(sock_fd, hello_frame, hello_len) != DTJ_OK) {
        free(hello_frame);
        close(sock_fd);
        dtj_discovery_stop(disc);
        if (out_error) dtj_error_init(out_error, DTJ_ERROR_PROTOCOL, "Hello failed");
        return NULL;
    }
    free(hello_frame);

    /* Read HelloOk */
    uint8_t *resp_frame = NULL;
    size_t resp_len = 0;
    if (dtj_session_read_frame(sock_fd, &resp_frame, &resp_len) != DTJ_OK) {
        close(sock_fd);
        dtj_discovery_stop(disc);
        if (out_error) dtj_error_init(out_error, DTJ_ERROR_PROTOCOL, "Hello response failed");
        return NULL;
    }
    
    dtj_frame frame = dtj_decode_frame(resp_frame, resp_len);
    free(resp_frame);
    
    uint32_t version = 0;
    if (dtj_decode_hello_ok(&frame, &version) != DTJ_OK || version != DTJ_PROTOCOL_VERSION) {
        dtj_frame_free(&frame);
        close(sock_fd);
        dtj_discovery_stop(disc);
        if (out_error) dtj_error_init(out_error, DTJ_ERROR_PROTOCOL, "Protocol version mismatch");
        return NULL;
    }
    dtj_frame_free(&frame);

    /* Generate session metadata */
    char session_file_name[256];
    if (config->session_file_name && *config->session_file_name) {
        strncpy(session_file_name, config->session_file_name, sizeof(session_file_name));
    } else {
        snprintf(session_file_name, sizeof(session_file_name), "session-%ld.dtj", time(NULL));
    }

    uint8_t session_id[16];
    if (dtj_generate_session_id(session_id) != DTJ_OK) {
        close(sock_fd);
        dtj_discovery_stop(disc);
        if (out_error) dtj_error_init(out_error, DTJ_ERROR_PROTOCOL, "Failed to generate session ID");
        return NULL;
    }

    struct timeval tv;
    gettimeofday(&tv, NULL);
    
    dtj_open_session_meta meta = {
        .file_name = session_file_name,
        .start_utc_unix_ms = tv.tv_sec * 1000 + tv.tv_usec / 1000,
        .mono_origin_ns = 0,
        .producer_name = config->producer_name,
        .producer_version = config->producer_version,
    };
    memcpy(meta.session_id, session_id, 16);

    /* Send OpenSession */
    size_t open_len = 0;
    uint8_t *open_frame = dtj_encode_open_session(&meta, &open_len);
    if (!open_frame || dtj_session_send_frame(sock_fd, open_frame, open_len) != DTJ_OK) {
        free(open_frame);
        close(sock_fd);
        dtj_discovery_stop(disc);
        if (out_error) dtj_error_init(out_error, DTJ_ERROR_PROTOCOL, "OpenSession failed");
        return NULL;
    }
    free(open_frame);

    /* Read OpenSessionOk */
    if (dtj_session_read_frame(sock_fd, &resp_frame, &resp_len) != DTJ_OK) {
        close(sock_fd);
        dtj_discovery_stop(disc);
        if (out_error) dtj_error_init(out_error, DTJ_ERROR_PROTOCOL, "OpenSession response failed");
        return NULL;
    }
    
    frame = dtj_decode_frame(resp_frame, resp_len);
    free(resp_frame);
    
    if (dtj_decode_open_session_ok(&frame) != DTJ_OK) {
        dtj_frame_free(&frame);
        close(sock_fd);
        dtj_discovery_stop(disc);
        if (out_error) dtj_error_init(out_error, DTJ_ERROR_PROTOCOL, "OpenSession rejected");
        return NULL;
    }
    dtj_frame_free(&frame);

    /* Create session object */
    dtj_session_internal *s = calloc(1, sizeof(dtj_session_internal));
    if (!s) {
        close(sock_fd);
        dtj_discovery_stop(disc);
        if (out_error) dtj_error_init(out_error, DTJ_ERROR_VALUE, "OOM");
        return NULL;
    }

    s->sock_fd = sock_fd;
    s->discovery = disc;
    memcpy(s->session_id, session_id, 16);
    s->mono_origin_ns = 0;
    s->closed = 0;
    s->disabled = 0;
    s->warning_handler = warn_handler;
    s->warning_user_data = warn_ud;
    s->warning_emitted = 0;
    pthread_mutex_init(&s->mutex, NULL);

    /* Initialize caches */
    s->domain_cache.capacity = 16;
    s->domain_cache.keys = calloc(16, sizeof(char*));
    s->domain_cache.values = calloc(16, sizeof(uint32_t));
    
    s->category_cache.capacity = 16;
    s->category_cache.keys = calloc(16, sizeof(char*));
    s->category_cache.values = calloc(16, sizeof(uint32_t));
    
    s->event_name_cache.capacity = 16;
    s->event_name_cache.keys = calloc(16, sizeof(char*));
    s->event_name_cache.values = calloc(16, sizeof(uint32_t));
    
    s->string_cache.capacity = 16;
    s->string_cache.keys = calloc(16, sizeof(char*));
    s->string_cache.values = calloc(16, sizeof(uint32_t));

    if (out_error) out_error->code = DTJ_OK;

    return (dtj_session*)s;
}

int dtj_emit(dtj_session *session, const dtj_event *event, dtj_error *out_error) {
    if (!session || !event) {
        if (out_error) dtj_error_init(out_error, DTJ_ERROR_VALUE, "Invalid arguments");
        return DTJ_ERROR_VALUE;
    }
    
    dtj_session_internal *s = (dtj_session_internal*)session;
    
    if (s->disabled) {
        return DTJ_OK; /* No-op on disabled session */
    }
    
    pthread_mutex_lock(&s->mutex);
    
    if (s->closed) {
        pthread_mutex_unlock(&s->mutex);
        if (out_error) dtj_error_init(out_error, DTJ_ERROR_SESSION, "Session closed");
        return DTJ_ERROR_SESSION;
    }
    
    /* Validate event fields */
    if (!event->domain || !event->category || !event->name || !event->field_name) {
        pthread_mutex_unlock(&s->mutex);
        if (out_error) dtj_error_init(out_error, DTJ_ERROR_VALUE, "Missing required event fields");
        return DTJ_ERROR_VALUE;
    }

    /* Get or intern dictionary entries */
    uint32_t domain_id = dtj_session_get_or_intern(s, DTJ_DICT_DOMAIN, event->domain, &s->domain_cache);
    uint32_t category_id = dtj_session_get_or_intern(s, DTJ_DICT_CATEGORY, event->category, &s->category_cache);
    uint32_t event_name_id = dtj_session_get_or_intern(s, DTJ_DICT_EVENT_NAME, event->name, &s->event_name_cache);
    uint32_t correlation_id = 0;
    if (event->correlation) {
        correlation_id = dtj_session_get_or_intern(s, DTJ_DICT_STRING, event->correlation, &s->string_cache);
    }
    uint32_t field_name_id = dtj_session_get_or_intern(s, DTJ_DICT_STRING, event->field_name, &s->string_cache);

    /* Encode value */
    uint8_t type_tag = 0;
    uint8_t value_body[64];
    size_t value_body_len = 0;

    switch (event->value.type) {
        case DTJ_VALUE_BOOL:
            type_tag = DTJ_TAG_BOOL;
            value_body[0] = event->value.u.bool_val ? 1 : 0;
            value_body_len = 1;
            break;
        case DTJ_VALUE_I64:
            type_tag = DTJ_TAG_I64;
            write_u64_le(value_body, (uint64_t)event->value.u.i64_val);
            value_body_len = 8;
            break;
        case DTJ_VALUE_F64:
            type_tag = DTJ_TAG_F64;
            write_u64_le(value_body, *((uint64_t*)&event->value.u.f64_val));
            value_body_len = 8;
            break;
        case DTJ_VALUE_STRING:
            type_tag = DTJ_TAG_INTERNED;
            write_u32_le(value_body, event->value.u.string_val.dict_id);
            value_body_len = 4;
            break;
        case DTJ_VALUE_BYTES:
            type_tag = DTJ_TAG_BYTES;
            if (event->value.u.bytes_val.len > UINT32_MAX) {
                pthread_mutex_unlock(&s->mutex);
                if (out_error) dtj_error_init(out_error, DTJ_ERROR_VALUE, "Bytes too long");
                return DTJ_ERROR_VALUE;
            }
            write_u32_le(value_body, event->value.u.bytes_val.len);
            memcpy(value_body + 4, event->value.u.bytes_val.data, event->value.u.bytes_val.len);
            value_body_len = 4 + event->value.u.bytes_val.len;
            break;
        default:
            pthread_mutex_unlock(&s->mutex);
            if (out_error) dtj_error_init(out_error, DTJ_ERROR_VALUE, "Unsupported value type");
            return DTJ_ERROR_VALUE;
    }

    /* Calculate monotonic timestamp */
    struct timespec ts;
    clock_gettime(CLOCK_MONOTONIC, &ts);
    uint64_t monotonic_ns = (uint64_t)ts.tv_sec * 1000000000ULL + ts.tv_nsec;

    /* Send AppendEvent */
    size_t append_len = 0;
    uint8_t *append_frame = dtj_encode_append_event(
        monotonic_ns,
        domain_id,
        category_id,
        event_name_id,
        correlation_id,
        (uint8_t)event->severity,
        field_name_id,
        type_tag,
        value_body,
        value_body_len,
        &append_len
    );
    
    if (!append_frame) {
        pthread_mutex_unlock(&s->mutex);
        if (out_error) dtj_error_init(out_error, DTJ_ERROR_PROTOCOL, "AppendEvent encoding failed");
        return DTJ_ERROR_PROTOCOL;
    }

    int ret = dtj_session_send_frame(s->sock_fd, append_frame, append_len);
    free(append_frame);

    if (ret != DTJ_OK) {
        pthread_mutex_unlock(&s->mutex);
        if (out_error) dtj_error_init(out_error, DTJ_ERROR_CONNECTION, "Failed to send AppendEvent");
        return DTJ_ERROR_CONNECTION;
    }

    /* Read AppendEventOk */
    if (dtj_session_read_frame(s->sock_fd, &resp_frame, &resp_len) != DTJ_OK) {
        pthread_mutex_unlock(&s->mutex);
        if (out_error) dtj_error_init(out_error, DTJ_ERROR_PROTOCOL, "AppendEvent response failed");
        return DTJ_ERROR_PROTOCOL;
    }
    
    frame = dtj_decode_frame(resp_frame, resp_len);
    free(resp_frame);
    
    if (dtj_decode_append_event_ok(&frame, NULL) != DTJ_OK) {
        dtj_frame_free(&frame);
        pthread_mutex_unlock(&s->mutex);
        if (out_error) dtj_error_init(out_error, DTJ_ERROR_PROTOCOL, "AppendEvent rejected");
        return DTJ_ERROR_PROTOCOL;
    }
    dtj_frame_free(&frame);

    pthread_mutex_unlock(&s->mutex);
    
    if (out_error) out_error->code = DTJ_OK;
    return DTJ_OK;
}

int dtj_close(dtj_session *session, dtj_error *out_error) {
    if (!session) {
        if (out_error) dtj_error_init(out_error, DTJ_ERROR_VALUE, "Invalid session");
        return DTJ_ERROR_VALUE;
    }
    
    dtj_session_internal *s = (dtj_session_internal*)session;
    
    pthread_mutex_lock(&s->mutex);
    
    if (s->closed || s->disabled) {
        pthread_mutex_unlock(&s->mutex);
        if (out_error) out_error->code = DTJ_OK;
        return DTJ_OK;
    }
    
    s->closed = 1;
    
    int ret = DTJ_OK;
    
    /* Send FinishSession */
    size_t finish_len = 0;
    uint8_t *finish_frame = dtj_encode_finish_session(&finish_len);
    if (finish_frame && dtj_session_send_frame(s->sock_fd, finish_frame, finish_len) == DTJ_OK) {
        /* Read FinishSessionOk */
        uint8_t *resp_frame = NULL;
        size_t resp_len = 0;
        if (dtj_session_read_frame(s->sock_fd, &resp_frame, &resp_len) == DTJ_OK) {
            dtj_frame frame = dtj_decode_frame(resp_frame, resp_len);
            free(resp_frame);
            dtj_decode_finish_session_ok(&frame);
            dtj_frame_free(&frame);
        }
    } else {
        ret = DTJ_ERROR_CONNECTION;
    }
    
    free(finish_frame);
    
    close(s->sock_fd);
    s->sock_fd = -1;
    
    pthread_mutex_unlock(&s->mutex);
    
    /* Stop agent */
    if (s->discovery) {
        dtj_discovery_stop(s->discovery);
        s->discovery = NULL;
    }
    
    if (out_error) out_error->code = ret;
    return ret;
}

void dtj_session_free(dtj_session *session) {
    if (!session) return;
    
    dtj_session_internal *s = (dtj_session_internal*)session;
    
    /* Close if not already closed */
    if (!s->closed && !s->disabled) {
        dtj_close(session, NULL);
    }
    
    /* Free caches */
#define FREE_CACHE(cache) \
    do { \
        for (size_t i = 0; i < cache.count; i++) free(cache.keys[i]); \
        free(cache.keys); \
        free(cache.values); \
        cache.keys = NULL; \
        cache.values = NULL; \
        cache.count = 0; \
        cache.capacity = 0; \
    } while (0)
    
    FREE_CACHE(s->domain_cache);
    FREE_CACHE(s->category_cache);
    FREE_CACHE(s->event_name_cache);
    FREE_CACHE(s->string_cache);
    
#undef FREE_CACHE
    
    pthread_mutex_destroy(&s->mutex);
    
    free(s);
}

int dtj_session_is_enabled(const dtj_session *session) {
    if (!session) return 0;
    const dtj_session_internal *s = (const dtj_session_internal*)session;
    return !s->disabled && !s->closed ? 1 : 0;
}

/* Helper: connect to Unix socket */
static int dtj_session_connect(const char *socket_path, int *out_fd) {
    int fd = socket(AF_UNIX, SOCK_STREAM, 0);
    if (fd < 0) return -1;

struct sockaddr_un addr = {0};
addr.sun_family = AF_UNIX;
strncpy(addr.sun_path, socket_path, sizeof(addr.sun_path) - 1);

if (connect(fd, (struct sockaddr*)&addr, sizeof(addr)) < 0) {
close(fd);
return -1;
}

*out_fd = fd;
return DTJ_OK;

/* Helper: send frame */
static int dtj_session_send_frame(int fd, const uint8_t *frame, size_t len) {
size_t sent = 0;
while (sent < len) {
ssize_t n = write(fd, frame + sent, len - sent);
if (n <= 0) return -1;
sent += n;
}
return DTJ_OK;

/* Helper: read frame */
static int dtj_session_read_frame(int fd, uint8_t **out_frame, size_t *out_len) {
uint8_t len_buf[4];
size_t read_len = 0;
while (read_len < 4) {
ssize_t n = read(fd, len_buf + read_len, 4 - read_len);
if (n <= 0) return -1;
read_len += n;
}

uint32_t frame_len = read_u32_le(len_buf);
if (frame_len > DTJ_MAX_FRAME_SIZE || frame_len == 0) return -1;

size_t total_len = 4 + frame_len;
uint8_t *frame = malloc(total_len);
if (!frame) return -1;

memcpy(frame, len_buf, 4);

read_len = 0;
while (read_len < frame_len) {
ssize_t n = read(fd, frame + 4 + read_len, frame_len - read_len);
if (n <= 0) { free(frame); return -1; }
read_len += n;
}

*out_frame = frame;
*out_len = total_len;
return DTJ_OK;

/* Helper: get or intern dictionary entry */
static uint32_t dtj_session_get_or_intern(dtj_session_internal *s,
                                           uint8_t kind,
                                           const char *name,
                                           struct { char **keys; uint32_t *values; size_t count; size_t capacity; } *cache) {
/* Check cache first */
for (size_t i = 0; i < cache->count; i++) {
if (strcmp(cache->keys[i], name) == 0) {
return cache->values[i];
}
}

/* Resize cache if needed */
if (cache->count >= cache->capacity) {
size_t new_cap = cache->capacity ? cache->capacity * 2 : 16;
char **new_keys = realloc(cache->keys, new_cap * sizeof(char*));
uint32_t *new_vals = realloc(cache->values, new_cap * sizeof(uint32_t));
if (!new_keys || !new_vals) { free(new_keys); free(new_vals); return 0; }
cache->keys = new_keys;
cache->values = new_vals;
cache->capacity = new_cap;
}

/* Send Intern request */
size_t intern_len = 0;
uint8_t *intern_frame = dtj_encode_intern(kind, name, &intern_len);
if (!intern_frame || dtj_session_send_frame(s->sock_fd, intern_frame, intern_len) != DTJ_OK || intern_frame == NULL?) {
free(intern_frame); // Wait the ternary is weird here. Let me fix this.
}
free(intern_frame);

/* Read InternOk */
uint8_t *resp_frame_2 = NULL; // Use different name to avoid conflict
size_t resp_len_2 = 0;

if (dtj_session_read_frame(s->sock_fd, &resp_frame_2, &resp_len_2) != DTJ_OK || !resp_frame_2?) {
// Error handling

}
free(resp_frame_2);

/* Parse response and add to cache */

}