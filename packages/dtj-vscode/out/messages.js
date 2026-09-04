"use strict";
Object.defineProperty(exports, "__esModule", { value: true });
exports.parseWebviewMessage = parseWebviewMessage;
exports.sanitizePersistedNav = sanitizePersistedNav;
exports.assertPersistedNavSafe = assertPersistedNavSafe;
const constants_1 = require("./constants");
const protocol_1 = require("./protocol");
const FILTER_KEYS = new Set([
    "domain",
    "category",
    "eventName",
    "severity",
    "correlation",
]);
const NAV_KEYS = new Set(["filters", "offset", "selectedSequence", "queryText"]);
function normalizeQueryText(raw) {
    if (typeof raw !== "string")
        return null;
    if (raw.length > constants_1.QUERY_TEXT_MAX_CHARS || raw.includes("\0"))
        return null;
    return raw;
}
function parseWebviewMessage(raw) {
    if (!raw || typeof raw !== "object")
        return null;
    const msg = raw;
    switch (msg.type) {
        case "ready":
        case "refresh":
            return { type: msg.type };
        case "runQuery":
        case "openQueryFile": {
            const text = normalizeQueryText(msg.text);
            if (text === null)
                return null;
            return { type: msg.type, text };
        }
        case "loadEvents": {
            if (typeof msg.offset !== "string" || !(0, protocol_1.isCanonicalDecimalU64)(msg.offset)) {
                return null;
            }
            if (typeof msg.limit !== "number" ||
                !Number.isInteger(msg.limit) ||
                msg.limit < 1 ||
                msg.limit > constants_1.EVENTS_PAGE_MAX) {
                return null;
            }
            const filters = normalizeFilters(msg.filters);
            if (!filters)
                return null;
            return { type: "loadEvents", offset: msg.offset, limit: msg.limit, filters };
        }
        case "selectEvent": {
            if (typeof msg.sequence !== "string" || !(0, protocol_1.isCanonicalDecimalU64)(msg.sequence))
                return null;
            return { type: "selectEvent", sequence: msg.sequence };
        }
        default:
            return null;
    }
}
function normalizeFilters(raw) {
    if (raw === undefined || raw === null)
        return {};
    if (typeof raw !== "object" || Array.isArray(raw))
        return null;
    const f = raw;
    for (const key of Object.keys(f)) {
        if (!FILTER_KEYS.has(key))
            return null;
    }
    const out = {};
    for (const key of ["domain", "category", "eventName", "severity", "correlation"]) {
        const v = f[key];
        if (v === undefined || v === "")
            continue;
        if (typeof v !== "string" || v.length > constants_1.FILTER_MAX_CHARS || v.includes("\0"))
            return null;
        out[key] = v;
    }
    return out;
}
function sanitizePersistedNav(raw) {
    const empty = {
        filters: {},
        offset: "0",
        selectedSequence: null,
        queryText: "",
    };
    if (!raw || typeof raw !== "object" || Array.isArray(raw))
        return empty;
    const o = raw;
    const filters = normalizeFilters(o.filters) ?? {};
    let offset = "0";
    if (typeof o.offset === "string" && (0, protocol_1.isCanonicalDecimalU64)(o.offset)) {
        offset = o.offset;
    }
    else if (typeof o.offset === "number" && Number.isSafeInteger(o.offset) && o.offset >= 0) {
        offset = String(o.offset);
    }
    const selectedSequence = typeof o.selectedSequence === "string" && (0, protocol_1.isCanonicalDecimalU64)(o.selectedSequence)
        ? o.selectedSequence
        : null;
    const queryText = normalizeQueryText(o.queryText) ?? "";
    return { filters, offset, selectedSequence, queryText };
}
/** Structural allowlist — never substring denylist (filter value "payload" is valid). */
function assertPersistedNavSafe(state) {
    if (!state || typeof state !== "object" || Array.isArray(state))
        return false;
    const keys = Object.keys(state);
    if (keys.length !== NAV_KEYS.size || keys.some((k) => !NAV_KEYS.has(k)))
        return false;
    if (typeof state.offset !== "string" || !(0, protocol_1.isCanonicalDecimalU64)(state.offset))
        return false;
    if (state.selectedSequence !== null &&
        (typeof state.selectedSequence !== "string" ||
            !(0, protocol_1.isCanonicalDecimalU64)(state.selectedSequence))) {
        return false;
    }
    if (typeof state.queryText !== "string" || state.queryText.length > constants_1.QUERY_TEXT_MAX_CHARS) {
        return false;
    }
    if (state.queryText.includes("\0"))
        return false;
    if (!state.filters || typeof state.filters !== "object" || Array.isArray(state.filters)) {
        return false;
    }
    for (const key of Object.keys(state.filters)) {
        if (!FILTER_KEYS.has(key))
            return false;
    }
    return normalizeFilters(state.filters) !== null;
}
//# sourceMappingURL=messages.js.map