import { EVENTS_PAGE_MAX, FILTER_MAX_CHARS, QUERY_TEXT_MAX_CHARS } from "./constants";
import type { EventFilters } from "./protocol";

export type TraceqlSubsetOk = {
  ok: true;
  filters: EventFilters;
  limit: number;
};

export type TraceqlSubsetErr = {
  ok: false;
  kind: "TraceqlSubsetError";
  message: string;
};

export type TraceqlSubsetResult = TraceqlSubsetOk | TraceqlSubsetErr;

const SEVERITIES = new Set(["trace", "debug", "info", "warn", "error", "fatal"]);

const FIELD_TO_FILTER: Record<string, keyof EventFilters> = {
  domain: "domain",
  category: "category",
  event_name: "eventName",
  severity: "severity",
  correlation: "correlation",
};

const SELECT_FIELDS = new Set([
  "event_sequence",
  "severity",
  "domain",
  "category",
  "event_name",
  "correlation",
  "monotonic_ns",
]);

type Tok =
  | { kind: "kw"; value: string }
  | { kind: "ident"; value: string }
  | { kind: "string"; value: string }
  | { kind: "number"; value: number }
  | { kind: "op"; value: string }
  | { kind: "star" }
  | { kind: "comma" }
  | { kind: "eof" };

function fail(message: string): TraceqlSubsetErr {
  return { ok: false, kind: "TraceqlSubsetError", message };
}

function tokenize(input: string): Tok[] | TraceqlSubsetErr {
  const s = input.trim();
  if (!s) return fail("empty query");
  if (s.length > QUERY_TEXT_MAX_CHARS) {
    return fail(`query exceeds ${QUERY_TEXT_MAX_CHARS} characters`);
  }
  if (s.includes("\0")) return fail("query contains NUL");
  if (s.includes(";")) return fail("multiple statements are not supported");

  const tokens: Tok[] = [];
  let i = 0;
  const isAlpha = (c: string) => /[A-Za-z_]/.test(c);
  const isAlnum = (c: string) => /[A-Za-z0-9_]/.test(c);

  while (i < s.length) {
    const c = s[i]!;
    if (/\s/.test(c)) {
      i += 1;
      continue;
    }
    if (c === "*") {
      tokens.push({ kind: "star" });
      i += 1;
      continue;
    }
    if (c === ",") {
      tokens.push({ kind: "comma" });
      i += 1;
      continue;
    }
    if (c === "." || c === "%" || c === "(" || c === ")") {
      return fail(`unsupported token '${c}' in explorer TraceQL subset`);
    }
    if (c === "=") {
      tokens.push({ kind: "op", value: "=" });
      i += 1;
      continue;
    }
    if (c === "!" || c === "<" || c === ">") {
      return fail("only '=' comparisons are supported in explorer TraceQL subset");
    }
    if (c === '"' || c === "'") {
      const quote = c;
      i += 1;
      let value = "";
      while (i < s.length && s[i] !== quote) {
        if (s[i] === "\\") {
          i += 1;
          if (i >= s.length) return fail("unterminated string escape");
          value += s[i];
          i += 1;
          continue;
        }
        value += s[i];
        i += 1;
      }
      if (i >= s.length) return fail("unterminated string literal");
      i += 1;
      tokens.push({ kind: "string", value });
      continue;
    }
    if (/[0-9]/.test(c)) {
      let num = "";
      while (i < s.length && /[0-9]/.test(s[i]!)) {
        num += s[i];
        i += 1;
      }
      if (i < s.length && isAlpha(s[i]!)) {
        return fail("numeric units are not supported in explorer TraceQL subset");
      }
      const n = Number(num);
      if (!Number.isSafeInteger(n)) return fail("integer out of safe range");
      tokens.push({ kind: "number", value: n });
      continue;
    }
    if (isAlpha(c)) {
      let ident = "";
      while (i < s.length && isAlnum(s[i]!)) {
        ident += s[i];
        i += 1;
      }
      const upper = ident.toUpperCase();
      const keywords = new Set([
        "FROM",
        "EVENTS",
        "WHERE",
        "AND",
        "OR",
        "NOT",
        "SELECT",
        "LIMIT",
        "IN",
        "BETWEEN",
        "GROUP",
        "BY",
        "ORDER",
        "ASC",
        "DESC",
        "GRAPH",
        "AS",
        "COUNT",
        "MIN",
        "MAX",
        "P50",
        "P95",
      ]);
      if (keywords.has(upper)) {
        tokens.push({ kind: "kw", value: upper });
      } else {
        tokens.push({ kind: "ident", value: ident.toLowerCase() });
      }
      continue;
    }
    return fail(`unexpected character '${c}'`);
  }
  tokens.push({ kind: "eof" });
  return tokens;
}

/**
 * Parse explorer TraceQL subset → exact ui-session filters + limit.
 * Does not send TraceQL to the native binary.
 */
export function parseTraceqlSubset(input: string): TraceqlSubsetResult {
  const tokResult = tokenize(input);
  if (!Array.isArray(tokResult)) return tokResult;
  const tokens = tokResult;
  let pos = 0;

  const peek = (): Tok => tokens[pos] ?? { kind: "eof" };
  const take = (): Tok => {
    const t = peek();
    pos += 1;
    return t;
  };
  const peekKw = (): string | null => {
    const t = peek();
    return t.kind === "kw" ? t.value : null;
  };
  const expectKw = (kw: string): TraceqlSubsetErr | null => {
    const t = take();
    if (t.kind !== "kw" || t.value !== kw) {
      return fail(`expected ${kw}`);
    }
    return null;
  };

  {
    const err = expectKw("FROM");
    if (err) return err;
  }
  {
    const t = take();
    if (t.kind === "kw" && t.value === "GRAPH") {
      return fail("FROM graph is not supported in explorer TraceQL subset");
    }
    if (t.kind !== "kw" || t.value !== "EVENTS") {
      return fail('expected "events" after FROM');
    }
  }

  const filters: EventFilters = {};
  const seen = new Set<keyof EventFilters>();

  if (peekKw() === "WHERE") {
    take();
    for (;;) {
      if (peekKw() === "OR") {
        return fail("OR is not supported in explorer TraceQL subset");
      }
      if (peekKw() === "NOT") {
        return fail("NOT is not supported in explorer TraceQL subset");
      }
      const fieldTok = take();
      if (fieldTok.kind !== "ident") {
        return fail("expected field name in WHERE");
      }
      if (fieldTok.value === "payload") {
        return fail("payload.* predicates are not supported in explorer TraceQL subset");
      }
      const filterKey = FIELD_TO_FILTER[fieldTok.value];
      if (!filterKey) {
        return fail(
          `unsupported field '${fieldTok.value}' (allowed: domain, category, event_name, severity, correlation)`,
        );
      }
      const op = take();
      if (op.kind === "kw" && (op.value === "IN" || op.value === "BETWEEN")) {
        return fail(`${op.value} is not supported in explorer TraceQL subset`);
      }
      if (op.kind !== "op" || op.value !== "=") {
        return fail("only '=' comparisons are supported");
      }
      const valueTok = take();
      let value: string;
      if (valueTok.kind === "string") {
        value = valueTok.value;
      } else if (valueTok.kind === "ident" && SEVERITIES.has(valueTok.value)) {
        value = valueTok.value;
      } else {
        return fail("predicate value must be a string or severity literal");
      }
      if (value.length > FILTER_MAX_CHARS) {
        return fail(`filter value for ${fieldTok.value} exceeds max length`);
      }
      if (filterKey === "severity" && !SEVERITIES.has(value)) {
        return fail("severity must be an exact severity name");
      }
      if (seen.has(filterKey)) {
        return fail(`duplicate predicate for ${fieldTok.value}`);
      }
      seen.add(filterKey);
      filters[filterKey] = value;

      if (peekKw() === "AND") {
        take();
        continue;
      }
      break;
    }
  }

  if (peekKw() === "GROUP") {
    return fail("GROUP BY is not supported in explorer TraceQL subset");
  }
  if (peekKw() === "ORDER") {
    return fail("ORDER BY is not supported in explorer TraceQL subset");
  }

  if (peekKw() === "SELECT") {
    take();
    const first = take();
    if (first.kind === "kw" && ["COUNT", "MIN", "MAX", "P50", "P95"].includes(first.value)) {
      return fail("aggregates are not supported in explorer TraceQL subset");
    }
    if (first.kind === "star") {
      // SELECT * — timeline shape is fixed by the explorer.
    } else if (first.kind === "ident" && SELECT_FIELDS.has(first.value)) {
      while (peek().kind === "comma") {
        take();
        const col = take();
        if (col.kind !== "ident" || !SELECT_FIELDS.has(col.value)) {
          return fail("SELECT supports only timeline fields or *");
        }
      }
    } else {
      return fail("SELECT supports only * or timeline fields");
    }
  }

  {
    const err = expectKw("LIMIT");
    if (err) return fail("LIMIT is required in explorer TraceQL subset");
  }
  const limTok = take();
  if (limTok.kind !== "number") return fail("LIMIT must be an integer");
  if (limTok.value < 1 || limTok.value > EVENTS_PAGE_MAX) {
    return fail(`LIMIT must be in 1..=${EVENTS_PAGE_MAX}`);
  }
  if (peek().kind !== "eof") {
    return fail("unexpected trailing tokens");
  }

  return { ok: true, filters, limit: limTok.value };
}

/** Format exact filters + limit as canonical explorer TraceQL subset text. */
export function formatTraceqlSubset(filters: EventFilters, limit: number): string {
  const preds: string[] = [];
  const push = (field: string, value: string | undefined) => {
    if (value === undefined || value === "") return;
    preds.push(`${field} = ${JSON.stringify(value)}`);
  };
  push("domain", filters.domain);
  push("category", filters.category);
  push("event_name", filters.eventName);
  push("severity", filters.severity);
  push("correlation", filters.correlation);
  const where = preds.length > 0 ? ` WHERE ${preds.join(" AND ")}` : "";
  return `FROM events${where} LIMIT ${limit}`;
}
