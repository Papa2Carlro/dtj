import {
  EVENTS_PAGE_MAX,
  FILTER_MAX_CHARS,
  QUERY_TEXT_MAX_CHARS,
} from "./constants";
import { isCanonicalDecimalU64, type EventFilters } from "./protocol";

export type HostToWebview =
  | { type: "state"; state: ExplorerUiState }
  | { type: "setQueryText"; text: string }
  | { type: "noop" };

export type WebviewToHost =
  | { type: "ready" }
  | { type: "refresh" }
  | { type: "runQuery"; text: string }
  | { type: "openQueryFile"; text: string }
  | { type: "loadEvents"; offset: string; limit: number; filters: EventFilters }
  | { type: "selectEvent"; sequence: string };

export type ExplorerBusy = null | "events" | "event" | "reload";

export type ExplorerUiState = {
  phase:
    | "loading"
    | "ready"
    | "NativeReaderUnavailable"
    | "WorkspaceUntrusted"
    | "UnsupportedDocument"
    | "NativeProtocolMismatch"
    | "NativeTimeout"
    | "NativeProtocolError"
    | "ReaderError"
    | "stale";
  message?: string;
  errorKind?: string;
  stale: boolean;
  tornTail: boolean;
  /** In-flight page/detail/reload indicator for webview chrome. */
  busy: ExplorerBusy;
  summary?: unknown;
  events?: unknown;
  detail?: unknown;
  filters: EventFilters;
  offset: string;
  limit: number;
  selectedSequence: string | null;
  queryText: string;
};

export type PersistedNavState = {
  filters: EventFilters;
  offset: string;
  selectedSequence: string | null;
  queryText: string;
};

const FILTER_KEYS = new Set([
  "domain",
  "category",
  "eventName",
  "severity",
  "correlation",
]);

const NAV_KEYS = new Set(["filters", "offset", "selectedSequence", "queryText"]);

function normalizeQueryText(raw: unknown): string | null {
  if (typeof raw !== "string") return null;
  if (raw.length > QUERY_TEXT_MAX_CHARS || raw.includes("\0")) return null;
  return raw;
}

export function parseWebviewMessage(raw: unknown): WebviewToHost | null {
  if (!raw || typeof raw !== "object") return null;
  const msg = raw as Record<string, unknown>;
  switch (msg.type) {
    case "ready":
    case "refresh":
      return { type: msg.type };
    case "runQuery":
    case "openQueryFile": {
      const text = normalizeQueryText(msg.text);
      if (text === null) return null;
      return { type: msg.type, text };
    }
    case "loadEvents": {
      if (typeof msg.offset !== "string" || !isCanonicalDecimalU64(msg.offset)) {
        return null;
      }
      if (
        typeof msg.limit !== "number" ||
        !Number.isInteger(msg.limit) ||
        msg.limit < 1 ||
        msg.limit > EVENTS_PAGE_MAX
      ) {
        return null;
      }
      const filters = normalizeFilters(msg.filters);
      if (!filters) return null;
      return { type: "loadEvents", offset: msg.offset, limit: msg.limit, filters };
    }
    case "selectEvent": {
      if (typeof msg.sequence !== "string" || !isCanonicalDecimalU64(msg.sequence)) return null;
      return { type: "selectEvent", sequence: msg.sequence };
    }
    default:
      return null;
  }
}

function normalizeFilters(raw: unknown): EventFilters | null {
  if (raw === undefined || raw === null) return {};
  if (typeof raw !== "object" || Array.isArray(raw)) return null;
  const f = raw as Record<string, unknown>;
  for (const key of Object.keys(f)) {
    if (!FILTER_KEYS.has(key)) return null;
  }
  const out: EventFilters = {};
  for (const key of ["domain", "category", "eventName", "severity", "correlation"] as const) {
    const v = f[key];
    if (v === undefined || v === "") continue;
    if (typeof v !== "string" || v.length > FILTER_MAX_CHARS || v.includes("\0")) return null;
    out[key] = v;
  }
  return out;
}

export function sanitizePersistedNav(raw: unknown): PersistedNavState {
  const empty: PersistedNavState = {
    filters: {},
    offset: "0",
    selectedSequence: null,
    queryText: "",
  };
  if (!raw || typeof raw !== "object" || Array.isArray(raw)) return empty;
  const o = raw as Record<string, unknown>;
  const filters = normalizeFilters(o.filters) ?? {};
  let offset = "0";
  if (typeof o.offset === "string" && isCanonicalDecimalU64(o.offset)) {
    offset = o.offset;
  } else if (typeof o.offset === "number" && Number.isSafeInteger(o.offset) && o.offset >= 0) {
    offset = String(o.offset);
  }
  const selectedSequence =
    typeof o.selectedSequence === "string" && isCanonicalDecimalU64(o.selectedSequence)
      ? o.selectedSequence
      : null;
  const queryText = normalizeQueryText(o.queryText) ?? "";
  return { filters, offset, selectedSequence, queryText };
}

/** Structural allowlist — never substring denylist (filter value "payload" is valid). */
export function assertPersistedNavSafe(state: PersistedNavState): boolean {
  if (!state || typeof state !== "object" || Array.isArray(state)) return false;
  const keys = Object.keys(state);
  if (keys.length !== NAV_KEYS.size || keys.some((k) => !NAV_KEYS.has(k))) return false;
  if (typeof state.offset !== "string" || !isCanonicalDecimalU64(state.offset)) return false;
  if (
    state.selectedSequence !== null &&
    (typeof state.selectedSequence !== "string" ||
      !isCanonicalDecimalU64(state.selectedSequence))
  ) {
    return false;
  }
  if (typeof state.queryText !== "string" || state.queryText.length > QUERY_TEXT_MAX_CHARS) {
    return false;
  }
  if (state.queryText.includes("\0")) return false;
  if (!state.filters || typeof state.filters !== "object" || Array.isArray(state.filters)) {
    return false;
  }
  for (const key of Object.keys(state.filters)) {
    if (!FILTER_KEYS.has(key)) return false;
  }
  return normalizeFilters(state.filters) !== null;
}
