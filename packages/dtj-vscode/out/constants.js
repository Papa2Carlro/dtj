"use strict";
Object.defineProperty(exports, "__esModule", { value: true });
exports.DEFAULT_TRACEQL_QUERY = exports.QUERY_TEXT_MAX_CHARS = exports.FILTER_MAX_CHARS = exports.REQUIRED_CAPABILITIES = exports.PROCESS_TIMEOUT_MS = exports.STDERR_MAX_BYTES = exports.STDOUT_MAX_BYTES = exports.EVENTS_PAGE_MAX = exports.EVENTS_PAGE_DEFAULT = exports.UI_PROTOCOL_VERSION = exports.BINARY_SETTING = exports.RUN_TRACEQL_PICK_COMMAND = exports.RUN_TRACEQL_COMMAND = exports.REFRESH_COMMAND = exports.VIEW_TYPE = void 0;
exports.VIEW_TYPE = "dtg.sessionExplorer";
exports.REFRESH_COMMAND = "dtg.sessionExplorer.refresh";
exports.RUN_TRACEQL_COMMAND = "dtg.traceql.run";
exports.RUN_TRACEQL_PICK_COMMAND = "dtg.traceql.runAgainst";
exports.BINARY_SETTING = "dtg.sessionExplorer.dtjBinaryPath";
exports.UI_PROTOCOL_VERSION = 1;
exports.EVENTS_PAGE_DEFAULT = 100;
exports.EVENTS_PAGE_MAX = 256;
exports.STDOUT_MAX_BYTES = 2_097_152;
exports.STDERR_MAX_BYTES = 64 * 1024;
exports.PROCESS_TIMEOUT_MS = 15_000;
exports.REQUIRED_CAPABILITIES = ["summary", "events", "event"];
exports.FILTER_MAX_CHARS = 256;
/** Max persisted / accepted TraceQL subset query text length. */
exports.QUERY_TEXT_MAX_CHARS = 4_096;
exports.DEFAULT_TRACEQL_QUERY = "FROM events LIMIT 100";
//# sourceMappingURL=constants.js.map