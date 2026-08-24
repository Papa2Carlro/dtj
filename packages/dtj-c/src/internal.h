#ifndef DTJ_INTERNAL_H
#define DTJ_INTERNAL_H

#include "dtj/dtj.h"
#include <stddef.h>
#include <stdint.h>
#include <sys/types.h>
#include <pthread.h>
#include <limits.h>

#define DTJ_MAX_FRAME_SIZE 1048576
#define DTJ_PROTOCOL_VERSION 1
#define DTJ_TEMP_SOCKET_PREFIX "dtj-agent-"
#define DTJ_STARTUP_TIMEOUT_MS 5000
#define DTJ_RETRY_INTERVAL_MS 10
#define DTJ_MAX_STRING_CACHE 16

/* Protocol opcodes */
#define DTJ_OP_HELLO 0x01
#define DTJ_OP_HELLO_OK 0x02
#define DTJ_OP_OPEN_SESSION 0x03
#define DTJ_OP_OPEN_SESSION_OK 0x04
#define DTJ_OP_INTERN 0x05
#define DTJ_OP_INTERN_OK 0x06
#define DTJ_OP_APPEND_EVENT 0x07
#define DTJ_OP_APPEND_EVENT_OK 0x08
#define DTJ_OP_FINISH_SESSION 0x09
#define DTJ_OP_FINISH_SESSION_OK 0x0A
#define DTJ_OP_PING 0x0B
#define DTJ_OP_PONG 0x0C

/* Dictionary kinds */
#define DTJ_DICT_DOMAIN 0x01
#define DTJ_DICT_CATEGORY 0x02
#define DTJ_DICT_EVENT_NAME 0x03
#define DTJ_DICT_STRING 0x04

/* Type tags */
#define DTJ_TAG_BOOL 0x01
#define DTJ_TAG_I64 0x03
#define DTJ_TAG_F64 0x07
#define DTJ_TAG_INTERNED 0x0B
#define DTJ_TAG_BYTES 0x0C

/* Frame structure */
typedef struct {
    uint8_t opcode;
    uint8_t *body;
    size_t body_len;
} dtj_frame_internal;

/* Discovery structure */
typedef struct {
    pid_t pid;
    char temp_dir[PATH_MAX];
    char socket_path[PATH_MAX];
} dtj_discovery_internal;

/* Session internal cache */
typedef struct {
    char **keys;
    uint32_t *values;
    size_t count;
    size_t capacity;
} dtj_cache_internal;

/* Session internal */
typedef struct dtj_session_internal {
    int sock_fd;
    dtj_discovery_internal *discovery;
    uint8_t session_id[16];
    uint64_t mono_origin_ns;
    int closed;
    int disabled;
    
    dtj_cache_internal domain_cache;
    dtj_cache_internal category_cache;
    dtj_cache_internal event_name_cache;
    dtj_cache_internal string_cache;
    
    void (*warning_handler)(const char *, void *);
    void *warning_user_data;
    int warning_emitted;
    
    pthread_mutex_t mutex;
} dtj_session_internal;

/* Protocol helpers */
uint8_t *dtj_encode_frame(uint8_t opcode, const uint8_t *body, size_t body_len, size_t *out_len);
dtj_frame_internal dtj_decode_frame(const uint8_t *frame, size_t frame_len);
void dtj_frame_free_internal(dtj_frame_internal *frame);

uint8_t *dtj_encode_hello(size_t *out_len);
int dtj_decode_hello_ok(const dtj_frame_internal *frame, uint32_t *out_version);

uint8_t *dtj_encode_open_session(const char *file_name, const uint8_t session_id[16], int64_t start_utc_unix_ms, uint64_t mono_origin_ns, const char *producer_name, const char *producer_version, size_t *out_len);
int dtj_decode_open_session_ok(const dtj_frame_internal *frame);

uint8_t *dtj_encode_intern(uint8_t kind, const char *name, size_t *out_len);
int dtj_decode_intern_ok(const dtj_frame_internal *frame, uint32_t *out_id);

uint8_t *dtj_encode_append_event(uint64_t monotonic_ns, uint32_t domain_id, uint32_t category_id, uint32_t event_name_id, uint32_t correlation_id, uint8_t severity, uint32_t field_name_id, uint8_t type_tag, const uint8_t *value_body, size_t value_body_len, size_t *out_len);
int dtj_decode_append_event_ok(const dtj_frame_internal *frame);

uint8_t *dtj_encode_finish_session(size_t *out_len);
uint8_t *dtj_encode_ping(size_t *out_len);
int dtj_generate_session_id(uint8_t out[16]);

/* Session helpers */
static inline uint32_t read_u32_le(const uint8_t *buf) {
    return (uint32_t)buf[0] | ((uint32_t)buf[1] << 8) | ((uint32_t)buf[2] << 16) | ((uint32_t)buf[3] << 24);
}
static inline uint64_t read_u64_le(const uint8_t *buf) {
    return (uint64_t)buf[0] | ((uint64_t)buf[1] << 8) | ((uint64_t)buf[2] << 16) | ((uint64_t)buf[3] << 24) |
           ((uint64_t)buf[4] << 32) | ((uint64_t)buf[5] << 40) | ((uint64_t)buf[6] << 48) | ((uint64_t)buf[7] << 56);
}
static inline void write_u32_le(uint8_t *buf, uint32_t v) {
    buf[0] = v & 0xFF;
    buf[1] = (v >> 8) & 0xFF;
    buf[2] = (v >> 16) & 0xFF;
    buf[3] = (v >> 24) & 0xFF;
}
static inline void write_u64_le(uint8_t *buf, uint64_t v) {
    buf[0] = v & 0xFF;
    buf[1] = (v >> 8) & 0xFF;
    buf[2] = (v >> 16) & 0xFF;
    buf[3] = (v >> 24) & 0xFF;
    buf[4] = (v >> 32) & 0xFF;
    buf[5] = (v >> 40) & 0xFF;
    buf[6] = (v >> 48) & 0xFF;
    buf[7] = (v >> 56) & 0xFF;
}

/* Discovery API */
int dtj_discovery_start_internal(const dtj_config *config, const char *session_file_name, dtj_discovery_internal **out_discovery, char *out_socket_path, size_t socket_path_size, dtj_error *out_error);
void dtj_discovery_stop_internal(dtj_discovery_internal *disc);

/* Error helper */
void dtj_error_init(dtj_error *err, dtj_error_code code, const char *msg);

#endif /* DTJ_INTERNAL_H */
