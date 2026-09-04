import {
  EVENTS_PAGE_DEFAULT,
  EVENTS_PAGE_MAX,
  FILTER_MAX_CHARS,
  REQUIRED_CAPABILITIES,
  UI_PROTOCOL_VERSION,
} from "./constants";

export type UiOperation = "hello" | "summary" | "events" | "event";

export type UiError = { kind: string; message: string };

export type UiEnvelope =
  | { protocol_version: number; ok: true; operation: UiOperation; result: unknown }
  | { protocol_version: number; ok: false; operation: string; error: UiError };

export type EventFilters = {
  domain?: string;
  category?: string;
  eventName?: string;
  severity?: string;
  correlation?: string;
};

/** Canonical unsigned decimal string (no leading zeros except `"0"`). */
export type DecimalU64 = string;

export type EventsQuery = {
  offset: DecimalU64;
  limit: number;
  filters: EventFilters;
};

export type ArgvBuild =
  | { ok: true; args: string[] }
  | { ok: false; kind: "InvalidArgument"; message: string };

const SEVERITIES = new Set(["trace", "debug", "info", "warn", "error", "fatal"]);

export function isCanonicalDecimalU64(value: string): boolean {
  return /^(0|[1-9]\d*)$/.test(value);
}

export function isCanonicalDecimalI64(value: string): boolean {
  if (value === "0") return true;
  if (/^-?[1-9]\d*$/.test(value)) {
    try {
      const n = BigInt(value);
      return n >= BigInt("-9223372036854775808") && n <= BigInt("9223372036854775807");
    } catch {
      return false;
    }
  }
  return false;
}

/** Reject JSON numbers for fields that must be lossless decimal strings. */
export function requireDecimalU64(
  value: unknown,
  field: string,
): { ok: true; value: DecimalU64 } | { ok: false; kind: string; message: string } {
  if (typeof value === "number") {
    return {
      ok: false,
      kind: "NativeProtocolError",
      message: `${field} must be a canonical decimal string, not a JSON number`,
    };
  }
  if (typeof value !== "string" || !isCanonicalDecimalU64(value)) {
    return {
      ok: false,
      kind: "NativeProtocolError",
      message: `${field} must be a canonical unsigned decimal string`,
    };
  }
  return { ok: true, value };
}

export function addDecimalU64(a: DecimalU64, b: DecimalU64): DecimalU64 {
  return (BigInt(a) + BigInt(b)).toString();
}

export function saturatingSubDecimalU64(a: DecimalU64, b: DecimalU64): DecimalU64 {
  const av = BigInt(a);
  const bv = BigInt(b);
  return av > bv ? (av - bv).toString() : "0";
}

export function buildHelloArgv(): string[] {
  return ["ui-session", "hello"];
}

export function buildSummaryArgv(fsPath: string): string[] {
  return ["ui-session", "summary", fsPath];
}

export function buildEventsArgv(fsPath: string, query: EventsQuery): ArgvBuild {
  if (typeof query.offset !== "string" || !isCanonicalDecimalU64(query.offset)) {
    return {
      ok: false,
      kind: "InvalidArgument",
      message: "offset must be a canonical unsigned decimal string",
    };
  }
  if (
    typeof query.limit !== "number" ||
    !Number.isInteger(query.limit) ||
    query.limit < 1 ||
    query.limit > EVENTS_PAGE_MAX
  ) {
    return { ok: false, kind: "InvalidArgument", message: "limit must be in 1..=256" };
  }
  const args = [
    "ui-session",
    "events",
    fsPath,
    "--offset",
    query.offset,
    "--limit",
    String(query.limit),
  ];
  const pushFilter = (flag: string, value: string | undefined): ArgvBuild | null => {
    if (value === undefined || value === "") return null;
    if (value.length > FILTER_MAX_CHARS) {
      return { ok: false, kind: "InvalidArgument", message: `${flag} exceeds max length` };
    }
    if (value.includes("\0")) {
      return { ok: false, kind: "InvalidArgument", message: `${flag} contains NUL` };
    }
    args.push(flag, value);
    return null;
  };
  for (const [flag, value] of [
    ["--domain", query.filters.domain],
    ["--category", query.filters.category],
    ["--event-name", query.filters.eventName],
    ["--correlation", query.filters.correlation],
  ] as const) {
    const err = pushFilter(flag, value);
    if (err) return err;
  }
  if (query.filters.severity !== undefined && query.filters.severity !== "") {
    if (!SEVERITIES.has(query.filters.severity)) {
      return {
        ok: false,
        kind: "InvalidArgument",
        message: "severity must be an exact severity name",
      };
    }
    args.push("--severity", query.filters.severity);
  }
  return { ok: true, args };
}

export function buildEventArgv(fsPath: string, sequence: string): ArgvBuild {
  if (typeof sequence !== "string" || !isCanonicalDecimalU64(sequence.trim())) {
    return {
      ok: false,
      kind: "InvalidArgument",
      message: "sequence must be an unsigned decimal integer string",
    };
  }
  const seq = sequence.trim();
  return { ok: true, args: ["ui-session", "event", fsPath, "--sequence", seq] };
}

export function parseUiSessionStdout(
  stdout: string,
  expectedOperation: UiOperation,
): { ok: true; envelope: UiEnvelope } | { ok: false; kind: string; message: string } {
  const trimmed = stdout.trim();
  if (!trimmed) {
    return { ok: false, kind: "NativeProtocolError", message: "empty stdout" };
  }
  let envelope: unknown;
  try {
    const decoder = new JsonTailDecoder();
    envelope = decoder.parseOne(trimmed);
  } catch {
    return {
      ok: false,
      kind: "NativeProtocolError",
      message: "stdout is not exactly one JSON object",
    };
  }
  if (!envelope || typeof envelope !== "object") {
    return { ok: false, kind: "NativeProtocolError", message: "invalid envelope" };
  }
  const obj = envelope as Record<string, unknown>;
  if (obj.protocol_version !== UI_PROTOCOL_VERSION) {
    return {
      ok: false,
      kind: "NativeProtocolMismatch",
      message: "unsupported protocol_version",
    };
  }
  if (typeof obj.ok !== "boolean") {
    return { ok: false, kind: "NativeProtocolError", message: "missing ok" };
  }
  if (obj.operation !== expectedOperation) {
    return {
      ok: false,
      kind: "NativeProtocolError",
      message: "operation mismatch",
    };
  }
  if (obj.ok === true) {
    if (!("result" in obj)) {
      return { ok: false, kind: "NativeProtocolError", message: "missing result" };
    }
    return {
      ok: true,
      envelope: {
        protocol_version: UI_PROTOCOL_VERSION,
        ok: true,
        operation: expectedOperation,
        result: obj.result,
      },
    };
  }
  const err = obj.error;
  if (!err || typeof err !== "object") {
    return { ok: false, kind: "NativeProtocolError", message: "missing error" };
  }
  const e = err as Record<string, unknown>;
  if (typeof e.kind !== "string" || typeof e.message !== "string") {
    return { ok: false, kind: "NativeProtocolError", message: "invalid error shape" };
  }
  return {
    ok: true,
    envelope: {
      protocol_version: UI_PROTOCOL_VERSION,
      ok: false,
      operation: String(obj.operation),
      error: { kind: e.kind, message: e.message },
    },
  };
}

export function validateHelloResult(
  result: unknown,
): { ok: true } | { ok: false; kind: string; message: string } {
  if (!result || typeof result !== "object") {
    return { ok: false, kind: "NativeProtocolMismatch", message: "invalid hello result" };
  }
  const r = result as Record<string, unknown>;
  if (r.ui_protocol_version !== UI_PROTOCOL_VERSION) {
    return {
      ok: false,
      kind: "NativeProtocolMismatch",
      message: "ui_protocol_version mismatch",
    };
  }
  const caps = r.capabilities;
  if (!Array.isArray(caps) || !REQUIRED_CAPABILITIES.every((c) => caps.includes(c))) {
    return {
      ok: false,
      kind: "NativeProtocolMismatch",
      message: "missing required capabilities",
    };
  }
  const limits = r.limits;
  if (!limits || typeof limits !== "object") {
    return { ok: false, kind: "NativeProtocolMismatch", message: "missing hello limits" };
  }
  const lim = limits as Record<string, unknown>;
  if (lim.events_page_default !== EVENTS_PAGE_DEFAULT || lim.events_page_max !== EVENTS_PAGE_MAX) {
    return {
      ok: false,
      kind: "NativeProtocolMismatch",
      message: "unexpected page limit constants",
    };
  }
  const stdoutMax = requireDecimalU64(lim.stdout_max_bytes, "limits.stdout_max_bytes");
  if (!stdoutMax.ok) return stdoutMax;
  return { ok: true };
}

export function validateSummaryResult(
  result: unknown,
): { ok: true } | { ok: false; kind: string; message: string } {
  if (!result || typeof result !== "object") {
    return { ok: false, kind: "NativeProtocolError", message: "invalid summary result" };
  }
  const r = result as Record<string, unknown>;
  for (const field of ["event_count", "chunks_committed"] as const) {
    const v = requireDecimalU64(r[field], field);
    if (!v.ok) return v;
  }
  const header = r.header;
  if (!header || typeof header !== "object") {
    return { ok: false, kind: "NativeProtocolError", message: "missing summary header" };
  }
  const h = header as Record<string, unknown>;
  for (const field of ["start_utc_unix_ms", "mono_origin_ns"] as const) {
    if (typeof h[field] === "number") {
      return {
        ok: false,
        kind: "NativeProtocolError",
        message: `header.${field} must be a canonical decimal string, not a JSON number`,
      };
    }
    if (typeof h[field] !== "string") {
      return {
        ok: false,
        kind: "NativeProtocolError",
        message: `header.${field} must be a canonical decimal string`,
      };
    }
  }
  if (!isCanonicalDecimalI64(h.start_utc_unix_ms as string)) {
    return {
      ok: false,
      kind: "NativeProtocolError",
      message: "header.start_utc_unix_ms must be a canonical i64 decimal string",
    };
  }
  if (!isCanonicalDecimalU64(h.mono_origin_ns as string)) {
    return {
      ok: false,
      kind: "NativeProtocolError",
      message: "header.mono_origin_ns must be a canonical u64 decimal string",
    };
  }
  const dict = r.dictionary_counts;
  if (dict && typeof dict === "object") {
    const d = dict as Record<string, unknown>;
    for (const field of ["domain", "category", "event_name", "string"] as const) {
      if (field in d) {
        const v = requireDecimalU64(d[field], `dictionary_counts.${field}`);
        if (!v.ok) return v;
      }
    }
  }
  return { ok: true };
}

export function validateEventsResult(
  result: unknown,
): { ok: true } | { ok: false; kind: string; message: string } {
  if (!result || typeof result !== "object") {
    return { ok: false, kind: "NativeProtocolError", message: "invalid events result" };
  }
  const r = result as Record<string, unknown>;
  for (const field of ["matched_count", "returned_count", "offset"] as const) {
    const v = requireDecimalU64(r[field], field);
    if (!v.ok) return v;
  }
  if (
    typeof r.limit !== "number" ||
    !Number.isInteger(r.limit) ||
    r.limit < 1 ||
    r.limit > EVENTS_PAGE_MAX
  ) {
    return { ok: false, kind: "NativeProtocolError", message: "limit must be a number in 1..=256" };
  }
  const events = r.events;
  if (!Array.isArray(events)) {
    return { ok: false, kind: "NativeProtocolError", message: "events must be an array" };
  }
  for (const row of events) {
    if (!row || typeof row !== "object") {
      return { ok: false, kind: "NativeProtocolError", message: "invalid event row" };
    }
    const ev = row as Record<string, unknown>;
    for (const field of ["event_sequence", "monotonic_ns", "payload_field_count"] as const) {
      const v = requireDecimalU64(ev[field], field);
      if (!v.ok) return v;
    }
  }
  return { ok: true };
}

export function validateEventDetailResult(
  result: unknown,
): { ok: true } | { ok: false; kind: string; message: string } {
  if (!result || typeof result !== "object") {
    return { ok: false, kind: "NativeProtocolError", message: "invalid event result" };
  }
  const r = result as Record<string, unknown>;
  const event = r.event;
  if (!event || typeof event !== "object") {
    return { ok: false, kind: "NativeProtocolError", message: "missing event" };
  }
  const ev = event as Record<string, unknown>;
  for (const field of ["event_sequence", "monotonic_ns"] as const) {
    const v = requireDecimalU64(ev[field], field);
    if (!v.ok) return v;
  }
  for (const field of ["domain_id", "category_id", "event_name_id", "correlation_id"] as const) {
    const v = requireDecimalU64(ev[field], field);
    if (!v.ok) return v;
  }
  const payload = ev.payload;
  if (!Array.isArray(payload)) {
    return { ok: false, kind: "NativeProtocolError", message: "payload must be an array" };
  }
  for (const field of payload) {
    if (!field || typeof field !== "object") {
      return { ok: false, kind: "NativeProtocolError", message: "invalid payload field" };
    }
    const f = field as Record<string, unknown>;
    const nameId = requireDecimalU64(f.name_id, "payload.name_id");
    if (!nameId.ok) return nameId;
    if (typeof f.type !== "string") {
      return { ok: false, kind: "NativeProtocolError", message: "payload.type required" };
    }
    if (f.type === "i64") {
      if (typeof f.value === "number") {
        return {
          ok: false,
          kind: "NativeProtocolError",
          message: "i64 value must be a canonical decimal string, not a JSON number",
        };
      }
      if (typeof f.value !== "string" || !isCanonicalDecimalI64(f.value)) {
        return {
          ok: false,
          kind: "NativeProtocolError",
          message: "i64 value must be a canonical decimal string",
        };
      }
    }
    if (f.type === "u64") {
      const v = requireDecimalU64(f.value, "u64 value");
      if (!v.ok) return v;
    }
    if (f.type === "enum") {
      const v = requireDecimalU64(f.value, "enum value");
      if (!v.ok) return v;
    }
    if (f.type === "interned_string") {
      const v = requireDecimalU64(f.id, "interned_string.id");
      if (!v.ok) return v;
    }
  }
  return { ok: true };
}

export function defaultEventsQuery(partial?: Partial<EventsQuery>): EventsQuery {
  return {
    offset: partial?.offset ?? "0",
    limit: partial?.limit ?? EVENTS_PAGE_DEFAULT,
    filters: partial?.filters ?? {},
  };
}

/** Parse one JSON value and ensure no trailing non-whitespace remains. */
class JsonTailDecoder {
  parseOne(text: string): unknown {
    let value: unknown;
    let index = 0;
    try {
      value = JSON.parse(text);
      return value;
    } catch {
      // Fall through to detect multiple values via progressive parse.
    }
    for (let i = 1; i <= text.length; i++) {
      const slice = text.slice(0, i);
      try {
        value = JSON.parse(slice);
        index = i;
        break;
      } catch {
        // continue
      }
    }
    if (index === 0) {
      throw new Error("parse failed");
    }
    const rest = text.slice(index).trim();
    if (rest.length > 0) {
      throw new Error("trailing content");
    }
    return value;
  }
}

/** Safe display for lossless integer fields (decimal strings / bigint). */
export function displayCount(value: unknown): string {
  if (typeof value === "string" && /^-?\d+$/.test(value)) {
    return value;
  }
  if (typeof value === "bigint") {
    return value.toString();
  }
  // Numbers are not the protocol contract for wide ints; still render small ones for UX fallbacks.
  if (typeof value === "number" && Number.isFinite(value) && Number.isSafeInteger(value)) {
    return String(value);
  }
  return "?";
}
