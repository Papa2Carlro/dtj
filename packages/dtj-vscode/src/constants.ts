export const VIEW_TYPE = "dtg.sessionExplorer";
export const REFRESH_COMMAND = "dtg.sessionExplorer.refresh";
export const RUN_TRACEQL_COMMAND = "dtg.traceql.run";
export const RUN_TRACEQL_PICK_COMMAND = "dtg.traceql.runAgainst";
export const BINARY_SETTING = "dtg.sessionExplorer.dtjBinaryPath";

export const UI_PROTOCOL_VERSION = 1;
export const EVENTS_PAGE_DEFAULT = 100;
export const EVENTS_PAGE_MAX = 256;
export const STDOUT_MAX_BYTES = 2_097_152;
export const STDERR_MAX_BYTES = 64 * 1024;
export const PROCESS_TIMEOUT_MS = 15_000;

export const REQUIRED_CAPABILITIES = ["summary", "events", "event"] as const;

export const FILTER_MAX_CHARS = 256;
/** Max persisted / accepted TraceQL subset query text length. */
export const QUERY_TEXT_MAX_CHARS = 4_096;
export const DEFAULT_TRACEQL_QUERY = "FROM events LIMIT 100";
