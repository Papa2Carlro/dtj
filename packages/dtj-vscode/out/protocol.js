"use strict";
Object.defineProperty(exports, "__esModule", { value: true });
exports.isCanonicalDecimalU64 = isCanonicalDecimalU64;
exports.isCanonicalDecimalI64 = isCanonicalDecimalI64;
exports.requireDecimalU64 = requireDecimalU64;
exports.addDecimalU64 = addDecimalU64;
exports.saturatingSubDecimalU64 = saturatingSubDecimalU64;
exports.buildHelloArgv = buildHelloArgv;
exports.buildSummaryArgv = buildSummaryArgv;
exports.buildEventsArgv = buildEventsArgv;
exports.buildEventArgv = buildEventArgv;
exports.parseUiSessionStdout = parseUiSessionStdout;
exports.validateHelloResult = validateHelloResult;
exports.validateSummaryResult = validateSummaryResult;
exports.validateEventsResult = validateEventsResult;
exports.validateEventDetailResult = validateEventDetailResult;
exports.defaultEventsQuery = defaultEventsQuery;
exports.displayCount = displayCount;
const constants_1 = require("./constants");
const SEVERITIES = new Set(["trace", "debug", "info", "warn", "error", "fatal"]);
function isCanonicalDecimalU64(value) {
    return /^(0|[1-9]\d*)$/.test(value);
}
function isCanonicalDecimalI64(value) {
    if (value === "0")
        return true;
    if (/^-?[1-9]\d*$/.test(value)) {
        try {
            const n = BigInt(value);
            return n >= BigInt("-9223372036854775808") && n <= BigInt("9223372036854775807");
        }
        catch {
            return false;
        }
    }
    return false;
}
/** Reject JSON numbers for fields that must be lossless decimal strings. */
function requireDecimalU64(value, field) {
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
function addDecimalU64(a, b) {
    return (BigInt(a) + BigInt(b)).toString();
}
function saturatingSubDecimalU64(a, b) {
    const av = BigInt(a);
    const bv = BigInt(b);
    return av > bv ? (av - bv).toString() : "0";
}
function buildHelloArgv() {
    return ["ui-session", "hello"];
}
function buildSummaryArgv(fsPath) {
    return ["ui-session", "summary", fsPath];
}
function buildEventsArgv(fsPath, query) {
    if (typeof query.offset !== "string" || !isCanonicalDecimalU64(query.offset)) {
        return {
            ok: false,
            kind: "InvalidArgument",
            message: "offset must be a canonical unsigned decimal string",
        };
    }
    if (typeof query.limit !== "number" ||
        !Number.isInteger(query.limit) ||
        query.limit < 1 ||
        query.limit > constants_1.EVENTS_PAGE_MAX) {
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
    const pushFilter = (flag, value) => {
        if (value === undefined || value === "")
            return null;
        if (value.length > constants_1.FILTER_MAX_CHARS) {
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
    ]) {
        const err = pushFilter(flag, value);
        if (err)
            return err;
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
function buildEventArgv(fsPath, sequence) {
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
function parseUiSessionStdout(stdout, expectedOperation) {
    const trimmed = stdout.trim();
    if (!trimmed) {
        return { ok: false, kind: "NativeProtocolError", message: "empty stdout" };
    }
    let envelope;
    try {
        const decoder = new JsonTailDecoder();
        envelope = decoder.parseOne(trimmed);
    }
    catch {
        return {
            ok: false,
            kind: "NativeProtocolError",
            message: "stdout is not exactly one JSON object",
        };
    }
    if (!envelope || typeof envelope !== "object") {
        return { ok: false, kind: "NativeProtocolError", message: "invalid envelope" };
    }
    const obj = envelope;
    if (obj.protocol_version !== constants_1.UI_PROTOCOL_VERSION) {
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
                protocol_version: constants_1.UI_PROTOCOL_VERSION,
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
    const e = err;
    if (typeof e.kind !== "string" || typeof e.message !== "string") {
        return { ok: false, kind: "NativeProtocolError", message: "invalid error shape" };
    }
    return {
        ok: true,
        envelope: {
            protocol_version: constants_1.UI_PROTOCOL_VERSION,
            ok: false,
            operation: String(obj.operation),
            error: { kind: e.kind, message: e.message },
        },
    };
}
function validateHelloResult(result) {
    if (!result || typeof result !== "object") {
        return { ok: false, kind: "NativeProtocolMismatch", message: "invalid hello result" };
    }
    const r = result;
    if (r.ui_protocol_version !== constants_1.UI_PROTOCOL_VERSION) {
        return {
            ok: false,
            kind: "NativeProtocolMismatch",
            message: "ui_protocol_version mismatch",
        };
    }
    const caps = r.capabilities;
    if (!Array.isArray(caps) || !constants_1.REQUIRED_CAPABILITIES.every((c) => caps.includes(c))) {
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
    const lim = limits;
    if (lim.events_page_default !== constants_1.EVENTS_PAGE_DEFAULT || lim.events_page_max !== constants_1.EVENTS_PAGE_MAX) {
        return {
            ok: false,
            kind: "NativeProtocolMismatch",
            message: "unexpected page limit constants",
        };
    }
    const stdoutMax = requireDecimalU64(lim.stdout_max_bytes, "limits.stdout_max_bytes");
    if (!stdoutMax.ok)
        return stdoutMax;
    return { ok: true };
}
function validateSummaryResult(result) {
    if (!result || typeof result !== "object") {
        return { ok: false, kind: "NativeProtocolError", message: "invalid summary result" };
    }
    const r = result;
    for (const field of ["event_count", "chunks_committed"]) {
        const v = requireDecimalU64(r[field], field);
        if (!v.ok)
            return v;
    }
    const header = r.header;
    if (!header || typeof header !== "object") {
        return { ok: false, kind: "NativeProtocolError", message: "missing summary header" };
    }
    const h = header;
    for (const field of ["start_utc_unix_ms", "mono_origin_ns"]) {
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
    if (!isCanonicalDecimalI64(h.start_utc_unix_ms)) {
        return {
            ok: false,
            kind: "NativeProtocolError",
            message: "header.start_utc_unix_ms must be a canonical i64 decimal string",
        };
    }
    if (!isCanonicalDecimalU64(h.mono_origin_ns)) {
        return {
            ok: false,
            kind: "NativeProtocolError",
            message: "header.mono_origin_ns must be a canonical u64 decimal string",
        };
    }
    const dict = r.dictionary_counts;
    if (dict && typeof dict === "object") {
        const d = dict;
        for (const field of ["domain", "category", "event_name", "string"]) {
            if (field in d) {
                const v = requireDecimalU64(d[field], `dictionary_counts.${field}`);
                if (!v.ok)
                    return v;
            }
        }
    }
    return { ok: true };
}
function validateEventsResult(result) {
    if (!result || typeof result !== "object") {
        return { ok: false, kind: "NativeProtocolError", message: "invalid events result" };
    }
    const r = result;
    for (const field of ["matched_count", "returned_count", "offset"]) {
        const v = requireDecimalU64(r[field], field);
        if (!v.ok)
            return v;
    }
    if (typeof r.limit !== "number" ||
        !Number.isInteger(r.limit) ||
        r.limit < 1 ||
        r.limit > constants_1.EVENTS_PAGE_MAX) {
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
        const ev = row;
        for (const field of ["event_sequence", "monotonic_ns", "payload_field_count"]) {
            const v = requireDecimalU64(ev[field], field);
            if (!v.ok)
                return v;
        }
    }
    return { ok: true };
}
function validateEventDetailResult(result) {
    if (!result || typeof result !== "object") {
        return { ok: false, kind: "NativeProtocolError", message: "invalid event result" };
    }
    const r = result;
    const event = r.event;
    if (!event || typeof event !== "object") {
        return { ok: false, kind: "NativeProtocolError", message: "missing event" };
    }
    const ev = event;
    for (const field of ["event_sequence", "monotonic_ns"]) {
        const v = requireDecimalU64(ev[field], field);
        if (!v.ok)
            return v;
    }
    for (const field of ["domain_id", "category_id", "event_name_id", "correlation_id"]) {
        const v = requireDecimalU64(ev[field], field);
        if (!v.ok)
            return v;
    }
    const payload = ev.payload;
    if (!Array.isArray(payload)) {
        return { ok: false, kind: "NativeProtocolError", message: "payload must be an array" };
    }
    for (const field of payload) {
        if (!field || typeof field !== "object") {
            return { ok: false, kind: "NativeProtocolError", message: "invalid payload field" };
        }
        const f = field;
        const nameId = requireDecimalU64(f.name_id, "payload.name_id");
        if (!nameId.ok)
            return nameId;
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
            if (!v.ok)
                return v;
        }
        if (f.type === "enum") {
            const v = requireDecimalU64(f.value, "enum value");
            if (!v.ok)
                return v;
        }
        if (f.type === "interned_string") {
            const v = requireDecimalU64(f.id, "interned_string.id");
            if (!v.ok)
                return v;
        }
    }
    return { ok: true };
}
function defaultEventsQuery(partial) {
    return {
        offset: partial?.offset ?? "0",
        limit: partial?.limit ?? constants_1.EVENTS_PAGE_DEFAULT,
        filters: partial?.filters ?? {},
    };
}
/** Parse one JSON value and ensure no trailing non-whitespace remains. */
class JsonTailDecoder {
    parseOne(text) {
        let value;
        let index = 0;
        try {
            value = JSON.parse(text);
            return value;
        }
        catch {
            // Fall through to detect multiple values via progressive parse.
        }
        for (let i = 1; i <= text.length; i++) {
            const slice = text.slice(0, i);
            try {
                value = JSON.parse(slice);
                index = i;
                break;
            }
            catch {
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
function displayCount(value) {
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
//# sourceMappingURL=protocol.js.map