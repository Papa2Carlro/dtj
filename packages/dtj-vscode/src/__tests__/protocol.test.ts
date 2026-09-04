import { describe, expect, it } from "vitest";

import {
  addDecimalU64,
  buildEventArgv,
  buildEventsArgv,
  buildHelloArgv,
  buildSummaryArgv,
  displayCount,
  parseUiSessionStdout,
  requireDecimalU64,
  saturatingSubDecimalU64,
  validateEventDetailResult,
  validateEventsResult,
  validateHelloResult,
  validateSummaryResult,
} from "../protocol";

describe("argv builders", () => {
  it("uses fixed ui-session operations", () => {
    expect(buildHelloArgv()).toEqual(["ui-session", "hello"]);
    expect(buildSummaryArgv("/abs/a.dtj")).toEqual([
      "ui-session",
      "summary",
      "/abs/a.dtj",
    ]);
  });

  it("validates pagination and filters; offset/sequence are decimal strings", () => {
    const ok = buildEventsArgv("/abs/a.dtj", {
      offset: "0",
      limit: 100,
      filters: { domain: "wire", severity: "info" },
    });
    expect(ok.ok).toBe(true);
    if (ok.ok) {
      expect(ok.args).toEqual([
        "ui-session",
        "events",
        "/abs/a.dtj",
        "--offset",
        "0",
        "--limit",
        "100",
        "--domain",
        "wire",
        "--severity",
        "info",
      ]);
    }
    expect(buildEventsArgv("/abs/a.dtj", { offset: "0", limit: 257, filters: {} }).ok).toBe(
      false,
    );
    expect(buildEventsArgv("/abs/a.dtj", { offset: "-1", limit: 10, filters: {} }).ok).toBe(
      false,
    );
    // Reject numeric offset (TypeScript callers must pass strings).
    expect(
      buildEventsArgv("/abs/a.dtj", {
        offset: 0 as unknown as string,
        limit: 10,
        filters: {},
      }).ok,
    ).toBe(false);
    expect(
      buildEventsArgv("/abs/a.dtj", {
        offset: "0",
        limit: 10,
        filters: { severity: "INFO" },
      }).ok,
    ).toBe(false);
    expect(buildEventArgv("/abs/a.dtj", "01").ok).toBe(false);
    expect(buildEventArgv("/abs/a.dtj", "2").ok).toBe(true);
    const maxSeq = buildEventArgv("/abs/a.dtj", "18446744073709551615");
    expect(maxSeq.ok).toBe(true);
    if (maxSeq.ok) {
      expect(maxSeq.args).toEqual([
        "ui-session",
        "event",
        "/abs/a.dtj",
        "--sequence",
        "18446744073709551615",
      ]);
    }
  });

  it("paginates offsets above MAX_SAFE_INTEGER via BigInt arithmetic", () => {
    const base = "9007199254740993";
    expect(addDecimalU64(base, "100")).toBe("9007199254741093");
    expect(saturatingSubDecimalU64(base, "100")).toBe("9007199254740893");
    expect(saturatingSubDecimalU64("10", "100")).toBe("0");
    const page = buildEventsArgv("/abs/a.dtj", {
      offset: addDecimalU64(base, "100"),
      limit: 50,
      filters: {},
    });
    expect(page.ok).toBe(true);
    if (page.ok) {
      expect(page.args).toContain("--offset");
      expect(page.args[page.args.indexOf("--offset") + 1]).toBe("9007199254741093");
    }
  });
});

describe("stdout protocol parser + lossless integers", () => {
  it("accepts hello handshake v1 with decimal stdout_max_bytes", () => {
    const stdout = JSON.stringify({
      protocol_version: 1,
      ok: true,
      operation: "hello",
      result: {
        ui_protocol_version: 1,
        capabilities: ["summary", "events", "event"],
        limits: {
          events_page_default: 100,
          events_page_max: 256,
          stdout_max_bytes: "2097152",
        },
      },
    });
    const parsed = parseUiSessionStdout(stdout, "hello");
    expect(parsed.ok).toBe(true);
    if (parsed.ok && parsed.envelope.ok) {
      expect(validateHelloResult(parsed.envelope.result).ok).toBe(true);
    }
  });

  it("rejects numeric values where decimal strings are required", () => {
    expect(requireDecimalU64(1, "offset").ok).toBe(false);
    expect(requireDecimalU64("1", "offset").ok).toBe(true);
    expect(
      validateSummaryResult({
        event_count: 2,
        chunks_committed: "1",
        header: { start_utc_unix_ms: "0", mono_origin_ns: "0" },
      }).ok,
    ).toBe(false);
    expect(
      validateEventsResult({
        matched_count: 1,
        returned_count: "1",
        offset: "0",
        limit: 100,
        events: [],
      }).ok,
    ).toBe(false);
    expect(
      validateEventDetailResult({
        event: {
          event_sequence: "1",
          monotonic_ns: "0",
          domain_id: "1",
          category_id: "1",
          event_name_id: "1",
          correlation_id: "0",
          payload: [{ name_id: "1", name: "x", type: "u64", value: Number.MAX_SAFE_INTEGER + 1 }],
        },
      }).ok,
    ).toBe(false);
    expect(
      validateEventDetailResult({
        event: {
          event_sequence: "1",
          monotonic_ns: "18446744073709551615",
          domain_id: "1",
          category_id: "1",
          event_name_id: "1",
          correlation_id: "0",
          payload: [
            { name_id: "1", name: "u", type: "u64", value: "18446744073709551615" },
            { name_id: "2", name: "i", type: "i64", value: "-9223372036854775808" },
          ],
        },
      }).ok,
    ).toBe(true);
  });

  it("displayCount preserves exact decimal strings", () => {
    expect(displayCount("18446744073709551615")).toBe("18446744073709551615");
    expect(displayCount(Number.MAX_SAFE_INTEGER + 1)).toBe("?");
  });

  it("rejects malformed or multiple JSON objects", () => {
    expect(parseUiSessionStdout("not-json", "summary").ok).toBe(false);
    expect(
      parseUiSessionStdout(
        '{"protocol_version":1,"ok":true,"operation":"summary","result":{}}{}',
        "summary",
      ).ok,
    ).toBe(false);
    expect(
      parseUiSessionStdout(
        '{"protocol_version":2,"ok":true,"operation":"hello","result":{}}',
        "hello",
      ).ok,
    ).toBe(false);
  });
});
