#include "dtj/dtj.h"
#include <assert.h>
#include <stdio.h>
#include <string.h>

/* Test warning handler callback */
static void test_warning_handler(const char *msg, void *user_data) {
    int *count = (int*)user_data;
    (*count)++;
    printf("Warning: %s", msg);
}

/* Test dtj_open_strict with disabled mode */
static void test_open_strict_disabled(void) {
    printf("Testing OpenStrict with disabled mode...\n");
    
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
    
    dtj_error err;
    dtj_session *sess = dtj_open_strict(&config, &err);
    assert(sess != NULL);
    assert(err.code == DTJ_OK);
    assert(dtj_session_is_enabled(sess) == 0);
    
    dtj_close(sess, NULL);
    dtj_session_free(sess);
    
    printf("  PASS\n");
}

/* Test OpenStrict with missing agent (should return error) */
static void test_open_strict_missing_agent(void) {
    printf("Testing OpenStrict with missing agent...\n");
    
    dtj_config config = {
        .data_dir = "./traces",
        .producer_name = "test",
        .producer_version = "1.0.0",
        .agent_path = "/nonexistent/dtj-agent",
        .enabled = 1,
        .warning_handler = NULL,
        .warning_user_data = NULL,
        .session_file_name = "test.dtj",
        .socket_path = NULL,
    };
    
    dtj_error err;
    dtj_session *sess = dtj_open_strict(&config, &err);
    assert(sess == NULL);
    assert(err.code == DTJ_ERROR_AGENT_NOT_FOUND);
    
    printf("  PASS\n");
}

/* Test warning handler callback */
static void test_warning_handler(const char *msg, void *user_data) {
    int *count = (int*)user_data;
    (*count)++;
    printf("Warning: %s", msg);
}

/* Test warning handler is called once */
static void test_warning_handler_once(void) {
    printf("Testing warning handler called once...\n");
    
    int warning_count = 0;
    
    dtj_config config = {
        .data_dir = "./traces",
        .producer_name = "test",
        .producer_version = "1.0.0",
        .agent_path = "/nonexistent/dtj-agent",
        .enabled = 0,
        .warning_handler = test_warning_handler,
        .warning_user_data = &warning_count,
        .session_file_name = "test.dtj",
        .socket_path = NULL,
    };
    
    dtj_session *sess1 = dtj_open(&config);
    assert(sess1 != NULL);
    
    dtj_session_free(sess1);
    
    /* Warning handler should be called exactly once */
    
    printf("  PASS\n");
}

/* Test Close idempotent */
static void test_close_idempotent(void) {
    printf("Testing Close idempotent...\n");
    
    dtj_config config = {
        .data_dir = "./traces",
        .producer_name = "test",
        .producer_version = "1.0.0",
        .enabled = 0,
        .session_file_name = "test.dtj",
        .socket_path = NULL,
        .agent_path = "/nonexistent/dtj-agent",
    };
    
    dtj_session *sess = dtj_open(&config);
    assert(sess != NULL);
    
    int ret1 = dtj_close(sess, NULL);
    assert(ret1 == DTJ_OK);
    
    int ret2 = dtj_close(sess, NULL);
    assert(ret2 == DTJ_OK);
    
    dtj_session_free(sess);
    
    printf("  PASS\n");
}

int main(void) {
    printf("Running client tests...\n\n");
    
    test_open_strict_disabled();
    test_open_strict_missing_agent();
    test_warning_handler_once();
    test_close_idempotent();
    
    printf("\nAll client tests passed!\n");
    return 0;
}