#include "dtj/dtj.h"
#include <stdlib.h>
#include <string.h>

const char *dtj_error_message(dtj_error_code code) {
    switch (code) {
        case DTJ_OK: return "Success";
        case DTJ_ERROR_PROTOCOL: return "Protocol error";
        case DTJ_ERROR_CONNECTION: return "Connection error";
        case DTJ_ERROR_AGENT_NOT_FOUND: return "Agent not found";
        case DTJ_ERROR_VALUE: return "Value error";
        case DTJ_ERROR_SESSION: return "Session error";
        case DTJ_ERROR_AGENT_UNAVAILABLE: return "Agent unavailable";
        default: return "Unknown error";
    }
}

void dtj_error_init(dtj_error *err, dtj_error_code code, const char *msg) {
    if (!err) return;
    err->code = code;
    err->message = msg ? msg : dtj_error_message(code);
}