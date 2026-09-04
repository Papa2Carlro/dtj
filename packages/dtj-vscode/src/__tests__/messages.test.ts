import { describe, expect, it } from "vitest";

import {
  assertPersistedNavSafe,
  parseWebviewMessage,
  sanitizePersistedNav,
} from "../messages";
import { renderWebviewHtml } from "../webviewHtml";

describe("webview messages and persistence", () => {
  it("accepts runQuery / openQueryFile and pagination messages", () => {
    expect(parseWebviewMessage({ type: "refresh" })).toEqual({ type: "refresh" });
    expect(
      parseWebviewMessage({
        type: "runQuery",
        text: 'FROM events WHERE domain = "wire" LIMIT 100',
      }),
    ).toEqual({
      type: "runQuery",
      text: 'FROM events WHERE domain = "wire" LIMIT 100',
    });
    expect(
      parseWebviewMessage({
        type: "openQueryFile",
        text: "FROM events LIMIT 10",
      }),
    ).toBeTruthy();
    expect(
      parseWebviewMessage({
        type: "loadEvents",
        offset: "0",
        limit: 100,
        filters: { domain: "wire" },
      }),
    ).toBeTruthy();
    expect(
      parseWebviewMessage({ type: "loadEvents", offset: "0", limit: 999, filters: {} }),
    ).toBeNull();
    expect(parseWebviewMessage({ type: "selectEvent", sequence: "1" })).toEqual({
      type: "selectEvent",
      sequence: "1",
    });
    expect(parseWebviewMessage({ type: "exec", args: ["rm", "-rf"] })).toBeNull();
  });

  it("persists queryText and filter value payload; drops unknown keys", () => {
    const nav = sanitizePersistedNav({
      filters: { domain: "payload" },
      offset: "10",
      selectedSequence: "3",
      queryText: 'FROM events WHERE domain = "payload" LIMIT 100',
      payload: { secret: 1 },
      dtjBinaryPath: "/evil",
      stderr: "nope",
    });
    expect(nav).toEqual({
      filters: { domain: "payload" },
      offset: "10",
      selectedSequence: "3",
      queryText: 'FROM events WHERE domain = "payload" LIMIT 100',
    });
    expect(assertPersistedNavSafe(nav)).toBe(true);
    expect(Object.keys(nav).sort()).toEqual([
      "filters",
      "offset",
      "queryText",
      "selectedSequence",
    ]);
  });

  it("webview exposes linear chrome, modal detail, timestamps, pagination", () => {
    const html = renderWebviewHtml("NONCE123", "https://csp.source");
    expect(html).toContain('class="chrome"');
    expect(html).toContain('class="querybar"');
    expect(html).toContain('id="query"');
    expect(html).toContain('id="modal-root"');
    expect(html).toContain("openDetailModal");
    expect(html).toContain("formatEventTime");
    expect(html).toContain("<th>time</th>");
    expect(html).toContain('id="first"');
    expect(html).toContain('id="last"');
    expect(html).toContain('type: "runQuery"');
    expect(html).toContain("File changed on disk");
    expect(html).toContain("torn_tail");
    expect(html).toContain("Query edited — press Run before paging");
    expect(html).toContain('s.busy === "events"');
    expect(html).toContain("ResponseTooLarge");
  });
});
