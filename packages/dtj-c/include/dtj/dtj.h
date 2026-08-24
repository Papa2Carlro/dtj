#ifndef DTJ_H
#define DTJ_H

#include <stddef.h>
#include <stdint.h>

#ifdef __cplusplus
extern "C" {
#endif

/* Opaque session handle */
typedef struct dtj_session dtj_session;

/* Error codes */
typedef enum {
    DTJ_OK = 0,
    DTJ_ERROR_PROTOCOL = -1,
    DTJ_ERROR_CONNECTION = -2,
    DTJ_ERROR_AGENT_NOT_FOUND = -3,
    DTJ_ERROR_VALUE = -4,
    DTJ_ERROR_SESSION = -5,
    DTJ_ERROR_AGENT_UNAVAILABLE = -6,
} dtj_error_code;

/* Error structure */
typedef struct {
    dtj_error_code code;
    char message[256]; /* fixed-size owned buffer */
} dtj_error;

/* Severity levels (match Rust dtj::Severity) */
typedef enum {
    DTJ_SEVERITY_DEBUG = 0,
    DTJ_SEVERITY_INFO  = 1,
    DTJ_SEVERITY_WARN  = 2,
    DTJ_SEVERITY_ERROR = 3,
    DTJ_SEVERITY_FATAL = 4,
} dtj_severity;

/* Value type tags (match dtj::Value) */
typedef enum {
    DTJ_VALUE_BOOL   = 0x01,
    DTJ_VALUE_I64    = 0x03,  /* signed 64-bit integer */
    DTJ_VALUE_F64    = 0x07,  /* IEEE-754 double */
    DTJ_VALUE_STRING = 0x0B,  /* interned string (dictionary ID) */
    DTJ_VALUE_BYTES  = 0x0C,  /* raw bytes with length prefix */
} dtj_value_type;

/* Tagged value union */
typedef struct {
    dtj_value_type type;
    union {
        int bool_val;           /* for DTJ_VALUE_BOOL */
        int64_t i64_val;        /* for DTJ_VALUE_I64 */
        double f64_val;         /* for DTJ_VALUE_F64 */
        struct {                /* for DTJ_VALUE_STRING: interned string ID */
            uint32_t dict_id;
        } string_val;
        struct {                /* for DTJ_VALUE_BYTES: raw bytes */
            const uint8_t *data;
            uint32_t len;
        } bytes_val;
    } u;
} dtj_value;

/* Event structure (MVP: exactly one field) */
typedef struct {
    const char *domain;      /* required, non-empty */
    const char *category;    /* required, non-empty */
    const char *name;        /* required, non-empty */
    dtj_severity severity;   /* required */
    const char *field_name;  /* required, non-empty */
    dtj_value value;         /* required */
    const char *correlation; /* optional, may be NULL */
} dtj_event;

/* Configuration structure */
typedef struct {
    const char *data_dir;            /* required, default "./traces" if NULL/empty */
    const char *producer_name;       /* required, max 32 bytes UTF-8 */
    const char *producer_version;    /* required, max 16 bytes UTF-8 */
    const char *agent_path;          /* optional, explicit dtj-agent path */
    const char *socket_path;         /* optional, existing agent socket path */
    const char *session_file_name;   /* optional, auto-generated if NULL/empty */
    int enabled;                     /* default true (1) if zero-initialized config not used */

    /* Warning callback: called once when agent unavailable.
       If NULL, a default handler prints to stderr. */
    void (*warning_handler)(const char *message, void *user_data);
    void *warning_user_data;
} dtj_config;

/* Open a new trace session.
   Returns a session pointer (possibly disabled no-op session).
   If agent unavailable and no explicit strict mode, returns disabled session
   and calls warning_handler exactly once.
   Returns NULL only on invalid config (NULL config or missing required fields). */
dtj_session *dtj_open(const dtj_config *config);

/* Open a new trace session with strict error handling.
   Returns NULL and sets out_error if agent unavailable or other errors.
   If successful, returns enabled session and out_error.code == DTJ_OK. */
dtj_session *dtj_open_strict(const dtj_config *config, dtj_error *out_error);

/* Emit a single event with exactly one field.
   Returns DTJ_OK on success or on disabled session (no-op).
   On active session with protocol/connection error, returns error code and sets out_error. */
int dtj_emit(dtj_session *session, const dtj_event *event, dtj_error *out_error);

/* Close the session gracefully.
   Sends FinishSession, waits for response, closes socket, stops spawned agent.
   Idempotent and safe to call multiple times.
   Returns DTJ_OK on success or if already closed/disabled. */
int dtj_close(dtj_session *session, dtj_error *out_error);

/* Free the session and associated resources.
   Calls dtj_close if not already closed.
   Idempotent and safe to call with NULL. */
void dtj_session_free(dtj_session *session);

/* Check if session is enabled (connected to agent).
   Returns 1 if active, 0 if disabled/no-op or NULL. */
int dtj_session_is_enabled(const dtj_session *session);

/* Helper functions for creating typed values */

static inline dtj_value dtj_make_bool(int v) {
    dtj_value val = { .type = DTJ_VALUE_BOOL };
    val.u.bool_val = v ? 1 : 0;
    return val;
}

static inline dtj_value dtj_make_i64(int64_t v) {
    dtj_value val = { .type = DTJ_VALUE_I64 };
    val.u.i64_val = v;
    return val;
}

static inline dtj_value dtj_make_f64(double v) {
    dtj_value val = { .type = DTJ_VALUE_F64 };
    val.u.f64_val = v;
    return val;
}

/* For string values: caller must intern the string first via dictionary.
   This helper creates a value from an already-interned dictionary ID. */
static inline dtj_value dtj_make_interned(uint32_t dict_id) {
    dtj_value val = { .type = DTJ_VALUE_STRING };
    val.u.string_val.dict_id = dict_id;
    return val;
}

/* For bytes values: data pointer must remain valid until emit returns. */
static inline dtj_value dtj_make_bytes(const uint8_t *data, uint32_t len) {
    dtj_value val = { .type = DTJ_VALUE_BYTES };
    val.u.bytes_val.data = data;
    val.u.bytes_val.len = len;
    return val;
}

/* Error initialization function */
void dtj_error_init(dtj_error *err, dtj_error_code code, const char *msg);

#ifdef __cplusplus
}
#endif

#endif /* DTJ_H */
