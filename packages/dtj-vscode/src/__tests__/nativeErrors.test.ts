import { describe, expect, it } from "vitest";

import { initialExplorerState } from "../webviewHtml";
import {
  formatNativeError,
  mapBrowseOpError,
  mapClientError,
} from "../nativeErrors";

describe("nativeErrors", () => {
  it("formats timeout and response-too-large distinctly", () => {
    expect(formatNativeError({ kind: "NativeTimeout", message: "x" })).toMatch(/timed out/);
    expect(formatNativeError({ kind: "ResponseTooLarge", message: "x" })).toMatch(/too large/);
  });

  it("hard-fails hello-style errors by clearing summary", () => {
    const base = initialExplorerState({
      summary: { event_count: "1" },
      phase: "ready",
    });
    const mapped = mapClientError(
      { kind: "NativeTimeout", message: "native process timed out" },
      base,
    );
    expect(mapped.phase).toBe("NativeTimeout");
    expect(mapped.summary).toBeUndefined();
    expect(mapped.errorKind).toBe("NativeTimeout");
    expect(mapped.busy).toBeNull();
  });

  it("soft-fails browse ops when summary is present", () => {
    const base = initialExplorerState({
      summary: { event_count: "2" },
      phase: "ready",
      events: { events: [] },
    });
    const mapped = mapBrowseOpError(
      { kind: "ResponseTooLarge", message: "stdout exceeded host cap" },
      base,
    );
    expect(mapped.phase).toBe("ready");
    expect(mapped.summary).toEqual({ event_count: "2" });
    expect(mapped.errorKind).toBe("ResponseTooLarge");
    expect(mapped.message).toMatch(/too large/);
  });
});
