#include "internal.h"
#include <stdint.h>
#include <stdlib.h>
#include <string.h>
#include <arpa/inet.h>
#include <unistd.h>
#include <fcntl.h>

#define DTJ_MAX_FRAME_SIZE 1048576  /* 1 MiB */
#define DTJ_PROTOCOL_VERSION 1

/* Opcodes */
#define DTJ_OP_HELLO         0x01
#define DTJ_OP_OPEN_SESSION  0x02
#define DTJ_OP_APPEND_EVENT  0x03
#define DTJ_OP_FINISH_SESSION 0x04
#define DTJ_OP_PING          0x05
#define DTJ_OP_INTERN        0x06

#define DTJ_OP_HELLO_OK         0x81
#define DTJ_OP_OPEN_SESSION_OK  0x82
#define DTJ_OP_APPEND_EVENT_OK  0x83
#define DTJ_OP_FINISH_SESSION_OK 0x84
#define DTJ_OP_PONG             0x85
#define DTJ_OP_INTERN_OK        0x86
#define DTJ_OP_ERROR            0xFF

/* Dictionary kinds */
#define DTJ_DICT_DOMAIN      1
#define DTJ_DICT_CATEGORY    2
#define DTJ_DICT_EVENT_NAME  3
#define DTJ_DICT_STRING      4

/* Type tags */
#define DTJ_TAG_BOOL   0x01
#define DTJ_TAG_I64    0x03
#define DTJ_TAG_F64    0x07
#define DTJ_TAG_INTERNED 0x0B
#define DTJ_TAG_BYTES  0x0C

/* Frame structure */
typedef struct {
    uint8_t opcode;
    uint8_t *body;
    size_t body_len;
} dtj_frame;

/* Write u32 LE */
static inline void write_u32_le(uint8_t *buf, uint32_t v) {
    buf[0] = v & 0xFF;
    buf[1] = (v >> 8) & 0xFF;
    buf[2] = (v >> 16) & 0xFF;
    buf[3] = (v >> 24) & 0xFF;
}

/* Write u64 LE */
static inline void write_u64_le(uint8_t *buf, uint64_t v) {
    for (int i = 0; i < 8; i++) {
        buf[i] = (v >> (i * 8)) & 0xFF;
    }
}

/* Write u16 LE */
static inline void write_u16_le(uint8_t *buf, uint16_t v) {
    buf[0] = v & 0xFF;
    buf[1] = (v >> 8) & 0xFF;
}

/* Read u32 LE */
static inline uint32_t read_u32_le(const uint8_t *buf) {
    return (uint32_t)buf[0] | ((uint32_t)buf[1] << 8) | ((uint32_t)buf[2] << 16) | ((uint32_t)buf[3] << 24);
}

/* Read u64 LE */
static inline uint64_t read_u64_le(const uint8_t *buf) {
    uint64_t v = 0;
    for (int i = 0; i < 8; i++) {
        v |= ((uint64_t)buf[i]) << (i * 8);
    }
    return v;
}

/* Read u16 LE */
static inline uint16_t read_u16_le(const uint8_t *buf) {
    return (uint16_t)buf[0] | ((uint16_t)buf[1] << 8);
}

/* Encode frame: allocates buffer, caller must free */
uint8_t *dtj_encode_frame(uint8_t opcode, const uint8_t *body, size_t body_len, size_t *out_len) {
    if (body_len + 1 > DTJ_MAX_FRAME_SIZE) {
        return NULL;
    }
    uint32_t frame_len = (uint32_t)(body_len + 1);
    size_t total_len = 4 + frame_len;
    uint8_t *frame = malloc(total_len);
    if (!frame) return NULL;

    write_u32_le(frame, frame_len);
    frame[4] = opcode;
    if (body && body_len > 0) {
        memcpy(frame + 5, body, body_len);
    }
    *out_len = total_len;
    return frame;
}

/* Decode frame from buffer - returns new dtj_frame with copied body, caller frees */
dtj_frame dtj_decode_frame(const uint8_t *data, size_t data_len) {
    dtj_frame frame = {0};
    if (data_len < 5) return frame;

    uint32_t frame_len = read_u32_le(data);
    if (frame_len > DTJ_MAX_FRAME_SIZE || data_len < 4 + frame_len) return frame;

    frame.opcode = data[4];
    frame.body_len = frame_len - 1;
    if (frame.body_len > 0) {
        frame.body = malloc(frame.body_len);
        if (!frame.body) return frame;
        memcpy(frame.body, data + 5, frame.body_len);
    }
    return frame;
}

void dtj_frame_free(dtj_frame *frame) {
    if (frame && frame->body) {
        free(frame->body);
        frame->body = NULL;
        frame->body_len = 0;
    }
}

/* Hello frame */
uint8_t *dtj_encode_hello(size_t *out_len) {
    uint8_t body[4];
    write_u32_le(body, DTJ_PROTOCOL_VERSION);
    return dtj_encode_frame(DTJ_OP_HELLO, body, sizeof(body), out_len);
}

/* HelloOk decode */
int dtj_decode_hello_ok(const dtj_frame *frame, uint32_t *out_version) {
    if (!frame || frame->opcode != DTJ_OP_HELLO_OK || frame->body_len != 4) return -1;
    *out_version = read_u32_le(frame->body);
    return read_u32_le(frame->body) == DTJ_PROTOCOL_VERSION ? 0 : -1;
}

/* Encode OpenSession - allocates buffer, caller frees */
uint8_t *dtj_encode_open_session(const char *file_name, const char *producer_name, const char *producer_version, const uint8_t *session_id, uint32_t epoch_sec, size_t *out_len) {
    if (!file_name || !producer_name || !producer_version || !session_id || !out_len) return NULL;

    size_t fn_len = strlen(file_name);
    size_t pn_len = strlen(producer_name);
    size_t pv_len = strlen(producer_version);

    if (pn_len > 32 || pv_len > 16 || fn_len > UINT16_MAX) return NULL;

    size_t body_size = sizeof(uint16_t) + fn_len +
                       sizeof(uint8_t) * 16 +
                       sizeof(uint64_t) + sizeof(uint32_t) +
                       sizeof(uint16_t) + pn_len +
                       sizeof(uint16_t) + pv_len;

    uint8_t *body = malloc(body_size);
    if (!body) return NULL;

    uint8_t *p = body;

    write_u16_le(p, (uint16_t)fn_len); p += 2;
    memcpy(p, file_name, fn_len); p += fn_len;
    memcpy(p, session_id, 16); p += 16;
    write_u64_le(p, (uint64_t)epoch_sec); p += 8;
    write_u64_le(p, 0); p += 8;
    write_u16_le(p, (uint16_t)pn_len); p += 2;
    memcpy(p, producer_name, pn_len); p += pn_len;
    write_u16_le(p, (uint16_t)pv_len); p += 2;
    memcpy(p, producer_version, pv_len);

    return dtj_encode_frame(DTJ_OP_OPEN_SESSION, body, body_size, out_len);
}

/* OpenSessionOk decode */
int dtj_decode_open_session_ok(const dtj_frame *frame) {
    return (frame && frame->opcode == DTJ_OP_OPEN_SESSION_OK) ? DTJ_OK : -1;
}

/* Encode Intern - allocates buffer, caller frees */
uint8_t *dtj_encode_intern(uint8_t kind, const char *name, size_t *out_len) {
    if (!name) return NULL;
    size_t name_len = strlen(name);
    if (name_len > 1024) return NULL;

    size_t body_size = 1 + sizeof(uint16_t) + name_len;
    uint8_t *body = malloc(body_size);
    if (!body) return NULL;

    body[0] = kind;
    write_u16_le(body + 1, (uint16_t)name_len);
    memcpy(body + 3, name, name_len);

    return dtj_encode_frame(DTJ_OP_INTERN, body, body_size, out_len);
}

/* Decode InternOk */
int dtj_decode_intern_ok(const dtj_frame_internal *frame, uint32_t *out_dict_id) {
    if (!frame || frame->opcode != DTJ_OP_INTERN_OK || frame->body_len != sizeof(uint32_t)) return -1;
    *out_dict_id = read_u32_le(frame->body);
    return DTJ_OK;
}

/* Encode AppendEvent with single field - allocates buffer, caller frees */
uint8_t *dtj_encode_append_event(uint64_t monotonic_ns,
                                  uint32_t domain_id,
                                  uint32_t category_id,
                                  uint32_t event_name_id,
                                  uint32_t correlation_id,
                                  uint8_t severity,
                                  uint32_t field_name_id,
                                  uint8_t type_tag,
                                  const uint8_t *value_body,
                                  size_t value_body_len,
                                  size_t *out_len) {
    size_t body_size = sizeof(uint64_t) + sizeof(uint32_t)*4 + sizeof(uint8_t) +
                       sizeof(uint16_t) + sizeof(uint32_t) + sizeof(uint8_t) + 3 + value_body_len;

    uint8_t *body = malloc(body_size);
    if (!body) return NULL;

    uint8_t *p = body;
    write_u64_le(p, monotonic_ns); p += 8;
    write_u32_le(p, domain_id); p += 4;
    write_u32_le(p, category_id); p += 4;
    write_u32_le(p, event_name_id); p += 4;
    write_u32_le(p, correlation_id); p += 4;
    *p++ = severity;
    write_u16_le(p, 1); p += 2; /* field_count = 1 */
    write_u32_le(p, field_name_id); p += 4;
    *p++ = type_tag;
    *p++ = 0; *p++ = 0; *p++ = 0; /* reserved */
    if (value_body && value_body_len > 0) memcpy(p, value_body, value_body_len);

    return dtj_encode_frame(DTJ_OP_APPEND_EVENT, body, body_size, out_len);
}

/* Decode AppendEventOk */
int dtj_decode_append_event_ok(const dtj_frame_internal *frame, uint64_t *out_sequence) {
    if (!frame || frame->opcode != DTJ_OP_APPEND_EVENT_OK || frame->body_len != sizeof(uint64_t)) return -1;
    *out_sequence = read_u64_le(frame->body);
    return DTJ_OK;
}

/* Encode FinishSession */
uint8_t *dtj_encode_finish_session(size_t *out_len) {
    return dtj_encode_frame(DTJ_OP_FINISH_SESSION, NULL, 0, out_len);
}

/* Decode FinishSessionOk */
int dtj_decode_finish_session_ok(const dtj_frame *frame) {
    return (frame && frame->opcode == DTJ_OP_FINISH_SESSION_OK) ? DTJ_OK : -1;
}

/* Encode Ping */
uint8_t *dtj_encode_ping(size_t *out_len) {
    return dtj_encode_frame(DTJ_OP_PING, NULL, 0, out_len);
}

/* Decode Pong */
int dtj_decode_pong(const dtj_frame *frame) {
    return (frame && frame->opcode == DTJ_OP_PONG) ? DTJ_OK : -1;
}

/* Decode Error frame */
int dtj_decode_error(const dtj_frame *frame, char **out_message) {
    if (!frame || frame->opcode != DTJ_OP_ERROR || frame->body_len == 0) return -1;
    char *msg = malloc(frame->body_len + 1);
    if (!msg) return -1;
    memcpy(msg, frame->body, frame->body_len);
    msg[frame->body_len] = '\0';
    *out_message = msg;
    return DTJ_OK;
}

/* Create session ID using getrandom or /dev/urandom */
int dtj_generate_session_id(uint8_t out[16]) {
#if defined(__linux__) || defined(__GLIBC__)
#include <sys/random.h>
ssize_t n = getrandom(out, 16, GRND_NONBLOCK);
if (n == 16) return DTJ_OK;
#endif
int fd = open("/dev/urandom", O_RDONLY | O_CLOEXEC);
if (fd < 0) return -1;
ssize_t n = read(fd, out, 16);
close(fd);
return n == 16 ? DTJ_OK : -1;
}

/* Encode value body for type tag */
int dtj_encode_value_body(dtj_value_type type_tag,
                           const dtj_value *value,
                           uint8_t **out_body,
                           size_t *out_len,
                           uint8_t **out_interned_string_to_free) {
    
}
