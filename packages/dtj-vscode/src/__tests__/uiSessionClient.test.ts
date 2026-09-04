import { describe, expect, it, vi } from "vitest";

import type { ProcessRunResult, ProcessRunner } from "../processRunner";
import { UiSessionClient } from "../uiSessionClient";

function okJson(operation: string, result: unknown): string {
  return JSON.stringify({
    protocol_version: 1,
    ok: true,
    operation,
    result,
  });
}

function fakeRunner(
  impl: (args: string[], signal?: AbortSignal) => Promise<Partial<ProcessRunResult>>,
): ProcessRunner {
  return async (req) => {
    const partial = await impl(req.args, req.signal);
    return {
      stdout: "",
      stderr: "",
      exitCode: 0,
      killed: false,
      timedOut: false,
      stdoutTruncated: false,
      stderrTruncated: false,
      ...partial,
    };
  };
}

describe("UiSessionClient", () => {
  it("runs summary/events/detail with fixed argv and string integers", async () => {
    const calls: string[][] = [];
    const runner = fakeRunner(async (args) => {
      calls.push(args);
      if (args[1] === "hello") {
        return {
          stdout: okJson("hello", {
            ui_protocol_version: 1,
            capabilities: ["summary", "events", "event"],
            limits: {
              events_page_default: 100,
              events_page_max: 256,
              stdout_max_bytes: "2097152",
            },
          }),
        };
      }
      if (args[1] === "summary") {
        return {
          stdout: okJson("summary", {
            session_file: "a.dtj",
            torn_tail: false,
            event_count: "1",
            chunks_committed: "1",
            header: {
              start_utc_unix_ms: "0",
              mono_origin_ns: "0",
              producer_name: "t",
            },
            dictionary_counts: {
              domain: "1",
              category: "1",
              event_name: "1",
              string: "0",
            },
          }),
        };
      }
      if (args[1] === "events") {
        return {
          stdout: okJson("events", {
            matched_count: "1",
            returned_count: "1",
            offset: "0",
            limit: 100,
            torn_tail: true,
            events: [
              {
                event_sequence: "1",
                monotonic_ns: "0",
                severity: "info",
                payload_field_count: "0",
              },
            ],
          }),
        };
      }
      return {
        stdout: okJson("event", {
          torn_tail: true,
          event: {
            event_sequence: "1",
            monotonic_ns: "0",
            domain_id: "1",
            category_id: "1",
            event_name_id: "1",
            correlation_id: "0",
            payload: [{ name_id: "1", name: "<b>x</b>", type: "bool", value: true }],
          },
        }),
      };
    });
    const client = new UiSessionClient("/abs/dtj", runner);
    expect((await client.hello()).ok).toBe(true);
    expect((await client.summary("/abs/a.dtj")).ok).toBe(true);
    const events = await client.events("/abs/a.dtj", {
      offset: "0",
      limit: 100,
      filters: {},
    });
    expect(events.ok).toBe(true);
    if (events.ok) {
      expect((events.value as { torn_tail: boolean }).torn_tail).toBe(true);
    }
    const detail = await client.event("/abs/a.dtj", "1");
    expect(detail.ok).toBe(true);
    expect(calls[0]).toEqual(["ui-session", "hello"]);
    expect(calls[1]).toEqual(["ui-session", "summary", "/abs/a.dtj"]);
    expect(calls[2]?.slice(0, 3)).toEqual(["ui-session", "events", "/abs/a.dtj"]);
    expect(calls[3]).toEqual(["ui-session", "event", "/abs/a.dtj", "--sequence", "1"]);
  });

  it("rejects protocol numeric integers for lossless fields", async () => {
    const client = new UiSessionClient(
      "/abs/dtj",
      fakeRunner(async () => ({
        stdout: okJson("events", {
          matched_count: 1,
          returned_count: 1,
          offset: 0,
          limit: 100,
          events: [],
        }),
      })),
    );
    const res = await client.events("/abs/a.dtj", { offset: "0", limit: 100, filters: {} });
    expect(res.ok).toBe(false);
    if (!res.ok) expect(res.error.kind).toBe("NativeProtocolError");
  });

  it("selects u64::MAX sequence argv exactly", async () => {
    const calls: string[][] = [];
    const client = new UiSessionClient(
      "/abs/dtj",
      fakeRunner(async (args) => {
        calls.push(args);
        return {
          stdout: JSON.stringify({
            protocol_version: 1,
            ok: false,
            operation: "event",
            error: { kind: "EventNotFound", message: "no event" },
          }),
        };
      }),
    );
    const res = await client.event("/abs/a.dtj", "18446744073709551615");
    expect(res.ok).toBe(false);
    expect(calls[0]).toEqual([
      "ui-session",
      "event",
      "/abs/a.dtj",
      "--sequence",
      "18446744073709551615",
    ]);
  });

  it("maps timeout, stdout cap, and cancellation", async () => {
    const timeoutClient = new UiSessionClient(
      "/abs/dtj",
      fakeRunner(async () => ({ timedOut: true, killed: true, exitCode: null })),
    );
    const t = await timeoutClient.hello();
    expect(t.ok).toBe(false);
    if (!t.ok) expect(t.error.kind).toBe("NativeTimeout");

    const bigClient = new UiSessionClient(
      "/abs/dtj",
      fakeRunner(async () => ({ stdoutTruncated: true, killed: true, exitCode: null })),
    );
    const b = await bigClient.hello();
    expect(b.ok).toBe(false);
    if (!b.ok) expect(b.error.kind).toBe("ResponseTooLarge");

    const runner = fakeRunner(async (_args, signal) => {
      await new Promise((r) => setTimeout(r, 30));
      if (signal?.aborted) {
        return { killed: true, exitCode: null };
      }
      return {
        stdout: okJson("hello", {
          ui_protocol_version: 1,
          capabilities: ["summary", "events", "event"],
          limits: {
            events_page_default: 100,
            events_page_max: 256,
            stdout_max_bytes: "2097152",
          },
        }),
      };
    });
    const client = new UiSessionClient("/abs/dtj", runner);
    const p = client.hello();
    client.cancel();
    const res = await p;
    expect(res.ok).toBe(false);
    if (!res.ok)
      expect(["NativeCancelled", "NativeTimeout", "NativeProtocolError"]).toContain(res.error.kind);
  });

  it("fail-closed reader errors have no result payload", async () => {
    const client = new UiSessionClient(
      "/abs/dtj",
      fakeRunner(async () => ({
        stdout: JSON.stringify({
          protocol_version: 1,
          ok: false,
          operation: "events",
          error: { kind: "ChecksumMismatch", message: "checksum mismatch" },
        }),
      })),
    );
    const res = await client.events("/abs/a.dtj", { offset: "0", limit: 100, filters: {} });
    expect(res.ok).toBe(false);
    if (!res.ok) expect(res.error.kind).toBe("ChecksumMismatch");
  });

  it("EventNotFound surfaces as structured error", async () => {
    const client = new UiSessionClient(
      "/abs/dtj",
      fakeRunner(async () => ({
        stdout: JSON.stringify({
          protocol_version: 1,
          ok: false,
          operation: "event",
          error: { kind: "EventNotFound", message: "no event" },
        }),
      })),
    );
    const res = await client.event("/abs/a.dtj", "9");
    expect(res.ok).toBe(false);
    if (!res.ok) expect(res.error.kind).toBe("EventNotFound");
  });

  it("ignores late results after dispose generation via cancel", async () => {
    let resolveRun: ((v: Partial<ProcessRunResult>) => void) | null = null;
    const runner = fakeRunner(
      () =>
        new Promise((resolve) => {
          resolveRun = resolve;
        }),
    );
    const client = new UiSessionClient("/abs/dtj", runner);
    const first = client.hello();
    client.dispose();
    resolveRun?.({
      stdout: okJson("hello", {
        ui_protocol_version: 1,
        capabilities: ["summary", "events", "event"],
        limits: {
          events_page_default: 100,
          events_page_max: 256,
          stdout_max_bytes: "2097152",
        },
      }),
    });
    const res = await first;
    expect(res.ok).toBe(false);
    vi.clearAllMocks();
  });
});
