#include "dtj/dtj.h"
#include <assert.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

/* Opt-in E2E test - only runs when DTJ_RUN_AGENT_E2E=1 */
int main(void) {
    const char *e2e_env = getenv("DTJ_RUN_AGENT_E2E");
    if (!e2e_env || strcmp(e2e_env, "1") != 0) {
        printf("E2E test skipped. Set DTJ_RUN_AGENT_E2E=1 to run.\n");
        return 0;
    }
    
    printf("Running E2E tests with real dtj-agent...\n\n");
    
    dtj_config config = {
        .data_dir = "./traces",
        .producer_name = "e2e-test",
        .producer_version = "0.1.0",
        .enabled = 1,
        .session_file_name = "e2e-session.dtj",
        .socket_path = NULL,
        .agent_path = NULL,
        .warning_handler = NULL,
        .warning_user_data = NULL,
    };
    
    dtj_error err;
    dtj_session *sess = dtj_open_strict(&config, &err);
    if (!sess) {
        fprintf(stderr, "Failed to open session: %s (code %d)\n", err.message, err.code);
        return 1;
    }
    
    printf("Session opened successfully (enabled: %d)\n", dtj_session_is_enabled(sess));
    
    /* Test various value types */
    
    /* Test bool */
    dtj_event event_bool = {
        .domain = "api",
        .category = "request",
        .name = "completed",
        .severity = DTJ_SEVERITY_INFO,
        .field_name = "success",
        .value = dtj_make_bool(1),
        .correlation = "req-001",
    };
    assert(dtj_emit(sess, &event_bool, NULL) == DTJ_OK);
    printf("  Emitted bool event\n");
    
    /* Test i64 */
    dtj_event event_i64 = {
        .domain = "test",
        .category = "counter",
        .name = "incremented",
        .severity = DTJ_SEVERITY_INFO,
        .field_name = "value",
        .value = dtj_make_i64(-42),
        .correlation = NULL,
    };
    assert(dtj_emit(sess, &event_i64, NULL) == DTJ_OK);
    printf("  Emitted i64 event\n");
    
    /* Test f64 */
    dtj_event event_f64 = {
        .domain = "test",
        .category = "ratio",
        .name = "calculated",
        .severity = DTJ_SEVERITY_INFO,
        .field_name = "ratio",
        .value = dtj_make_f64(3.14159),
        .correlation = NULL,
    };
    assert(dtj_emit(sess, &event_f64, NULL) == DTJ_OK);
    printf("  Emitted f64 event\n");
    
    /* Test bytes */
    uint8_t payload[] = {0xDE, 0xAD, 0xBE, 0xEF};
    dtj_event event_bytes = {
        .domain = "test",
        .category = "payload",
        .name = "received",
        .severity = DTJ_SEVERITY_INFO,
        .field_name = "data",
        .value = dtj_make_bytes(payload, sizeof(payload)),
        .correlation = NULL,
    };
    assert(dtj_emit(sess, &event_bytes, NULL) == DTJ_OK);
    printf("  Emitted bytes event\n");
    
    /* Test multiple severities */
    const char *severity_names[] = {"debug", "info", "warn", "error", "fatal"};
    dtj_severity severities[] = {
        DTJ_SEVERITY_DEBUG, DTJ_SEVERITY_INFO, DTJ_SEVERITY_WARN,
        DTJ_SEVERITY_ERROR, DTJ_SEVERITY_FATAL
    };
    
    for (int i = 0; i < 5; i++) {
        char corr[32];
        snprintf(corr, sizeof(corr), "sev-test-%d", i);
        
        dtj_event event_sev = {
            .domain = "test",
            .category = "severity",
            .name = severity_names[i],
            .severity = severities[i],
            .field_name = "level",
            .value = dtj_make_f64((double)i),
            .correlation = corr,
        };
        
        assert(dtj_emit(sess, &event_sev, NULL) == DTJ_OK);
        printf("  Emitted %s severity event\n", severity_names[i]);
    }
    
    /* Close session */
    int ret = dtj_close(sess, NULL);
    assert(ret == DTJ_OK);
    
    dtj_session_free(sess);
    
    printf("\nAll E2E tests passed!\n");
    return 0;
}