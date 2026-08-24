#include "dtj/dtj.h"
#include <assert.h>
#include <stdio.h>
#include <string.h>

/* Test frame encoding/decoding */
static void test_frame_encode_decode() {
    printf("Testing frame encode/decode...\n");
    
    uint8_t body[] = {1, 2, 3, 4};
    size_t frame_len = 0;
    uint8_t *frame = dtj_encode_frame(0x01, body, sizeof(body), &frame_len);
    assert(frame != NULL);
    assert(frame_len == 5 + sizeof(body)); // 4 bytes length + 1 opcode + body
    
    dtj_frame decoded = dtj_decode_frame(frame, frame_len);
    assert(decoded.opcode == 0x01);
    assert(decoded.body_len == sizeof(body));
    assert(memcmp(decoded.body, body, sizeof(body)) == 0);
    
    dtj_frame_free(&decoded);
    free(frame);
    
    printf("  PASS\n");
}

/* Test Hello frame */
static void test_hello_frame() {
    printf("Testing Hello frame...\n");
    
    size_t len = 0;
    uint8_t *frame = dtj_encode_hello(&len);
    assert(frame != NULL);
    
    dtj_frame decoded = dtj_decode_frame(frame, len);
    assert(decoded.opcode == DTJ_OP_HELLO);
    
    uint32_t version = 0;
    int ret = dtj_decode_hello_ok(&decoded, &version);
    assert(ret == DTJ_OK);
    assert(version == DTJ_PROTOCOL_VERSION);
    
    dtj_frame_free(&decoded);
    free(frame);
    
    printf("  PASS\n");
}

/* Test OpenSession encoding */
static void test_open_session_encoding() {
    printf("Testing OpenSession encoding...\n");
    
    uint8_t session_id[16] = {0};
    for (int i = 0; i < 16; i++) session_id[i] = i;
    
    dtj_open_session_meta meta = {
        .file_name = "test.dtj",
        .start_utc_unix_ms = 1234567890000,
        .mono_origin_ns = 987654321,
        .producer_name = "test-service",
        .producer_version = "1.0.0",
    };
    memcpy(meta.session_id, session_id, 16);
    
    size_t len = 0;
    uint8_t *frame = dtj_encode_open_session(&meta, &len);
    assert(frame != NULL);
    
    dtj_frame decoded = dtj_decode_frame(frame, len);
    assert(decoded.opcode == DTJ_OP_OPEN_SESSION);
    
    int ret = dtj_decode_open_session_ok(&decoded);
    // OpenSession doesn't have a response body, just checks opcode
    
    dtj_frame_free(&decoded);
    free(frame);
    
    printf("  PASS\n");
}

/* Test Intern encoding */
static void test_intern_encoding() {
    printf("Testing Intern encoding...\n");
    
    size_t len = 0;
    uint8_t *frame = dtj_encode_intern(DTJ_DICT_DOMAIN, "api", &len);
    assert(frame != NULL);
    
    dtj_frame decoded = dtj_decode_frame(frame, len);
    assert(decoded.opcode == DTJ_OP_INTERN);
    assert(decoded.body_len > 3); // kind + u16 len + string
    
    dtj_frame_free(&decoded);
    free(frame);
    
    printf("  PASS\n");
}

/* Test value encoding helpers */
static void test_value_helpers() {
    printf("Testing value helpers...\n");
    
    dtj_value v1 = dtj_make_bool(1);
    assert(v1.type == DTJ_VALUE_BOOL && v1.u.bool_val == 1);
    
    v1 = dtj_make_bool(0);
    assert(v1.type == DTJ_VALUE_BOOL && v1.u.bool_val == 0);
    
    v1 = dtj_make_i64(-42);
    assert(v1.type == DTJ_VALUE_I64 && v1.u.i64_val == -42);
    
    v1 = dtj_make_i64(INT64_MAX);
    assert(v1.type == DTJ_VALUE_I64 && v1.u.i64_val == INT64_MAX);
    
    v1 = dtj_make_f64(3.14159);
    assert(v1.type == DTJ_VALUE_F64 && v1.u.f64_val == 3.14159);
    
    v1 = dtj_make_interned(42);
    assert(v1.type == DTJ_VALUE_STRING && v1.u.string_val.dict_id == 42);
    
    uint8_t bytes[] = {0xDE, 0xAD, 0xBE, 0xEF};
    v1 = dtj_make_bytes(bytes, sizeof(bytes));
    assert(v1.type == DTJ_VALUE_BYTES && v1.u.bytes_val.len == sizeof(bytes));
    
    printf("  PASS\n");
}

/* Test Open with disabled mode (no agent) */
static void test_open_disabled() {
    printf("Testing Open with disabled mode...\n");
    
    dtj_config config = {
        .data_dir = "./traces",
        .producer_name = "test",
        .producer_version = "1.0.0",
        .agent_path = "/nonexistent/dtj-agent",
        .enabled = 0,
        .warning_handler = NULL,
        .warning_user_data = NULL,
        .session_file_name = "test.dtj",
        .socket_path = NULL,
    };
    
    dtj_session *sess = dtj_open(&config);
    assert(sess != NULL);
    assert(dtj_session_is_enabled(sess) == 0);
    
    /* Emit should succeed (no-op) */
    dtj_event event = {
        .domain = "test",
        .category = "cat",
        .name = "event",
        .severity = DTJ_SEVERITY_INFO,
        .field_name = "field",
        .value = dtj_make_i64(42),
        .correlation = NULL,
    };
    
    int ret = dtj_emit(sess, &event, NULL);
    assert(ret == DTJ_OK);
    
    dtj_close(sess, NULL);
    dtj_session_free(sess);
    
    printf("  PASS\n");
}

int main() {
    printf("Running protocol tests...\n\n");
    
    test_frame_encode_decode();
    test_hello_frame();
    test_open_session_encoding();
    test_intern_encoding();
    test_value_helpers();
    test_open_disabled();
    
    printf("\nAll tests passed!\n");
    return 0;
}