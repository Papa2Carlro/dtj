import { readFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";
import { describe, expect, it } from "vitest";

import { formatTraceqlSubset, parseTraceqlSubset } from "../traceqlSubset";

const root = join(dirname(fileURLToPath(import.meta.url)), "../..");

describe("parseTraceqlSubset", () => {
  it("parses FROM events WHERE equality AND LIMIT", () => {
    const r = parseTraceqlSubset(
      `FROM events WHERE domain = "wire" AND severity = info AND event_name = "KnotHit" LIMIT 50`,
    );
    expect(r.ok).toBe(true);
    if (r.ok) {
      expect(r.limit).toBe(50);
      expect(r.filters).toEqual({
        domain: "wire",
        severity: "info",
        eventName: "KnotHit",
      });
    }
  });

  it("accepts SELECT * before LIMIT", () => {
    const r = parseTraceqlSubset(`FROM events SELECT * LIMIT 100`);
    expect(r.ok).toBe(true);
    if (r.ok) {
      expect(r.filters).toEqual({});
      expect(r.limit).toBe(100);
    }
  });

  it("rejects OR, payload, graph, and missing LIMIT", () => {
    expect(parseTraceqlSubset(`FROM events WHERE domain = "a" OR category = "b" LIMIT 1`).ok).toBe(
      false,
    );
    expect(parseTraceqlSubset(`FROM events WHERE payload = "y" LIMIT 1`).ok).toBe(false);
    expect(parseTraceqlSubset(`FROM events WHERE payload.x = "y" LIMIT 1`).ok).toBe(false);
    expect(parseTraceqlSubset(`FROM graph START event("1") TRAVERSE contains DEPTH 0..1 RETURN node_id LIMIT 1`).ok).toBe(
      false,
    );
    expect(parseTraceqlSubset(`FROM events WHERE domain = "wire"`).ok).toBe(false);
  });

  it("round-trips formatTraceqlSubset", () => {
    const text = formatTraceqlSubset({ domain: "wire", severity: "info" }, 100);
    expect(text).toBe(`FROM events WHERE domain = "wire" AND severity = "info" LIMIT 100`);
    const r = parseTraceqlSubset(text);
    expect(r.ok).toBe(true);
    if (r.ok) {
      expect(r.filters).toEqual({ domain: "wire", severity: "info" });
      expect(r.limit).toBe(100);
    }
  });
});

describe("language contribution", () => {
  it("registers dtg-traceql language, grammar, and Run command in package.json", () => {
    const pkg = JSON.parse(readFileSync(join(root, "package.json"), "utf8")) as {
      contributes: {
        languages: Array<{ id: string; extensions: string[] }>;
        grammars: Array<{ language: string; path: string }>;
        commands?: Array<{ command: string }>;
        keybindings?: Array<{ command: string; when?: string }>;
      };
    };
    const lang = pkg.contributes.languages.find((l) => l.id === "dtg-traceql");
    expect(lang?.extensions).toContain(".traceql");
    const grammar = pkg.contributes.grammars.find((g) => g.language === "dtg-traceql");
    expect(grammar?.path).toBe("./syntaxes/dtg-traceql.tmLanguage.json");
    expect(readFileSync(join(root, "syntaxes/dtg-traceql.tmLanguage.json"), "utf8")).toContain(
      "source.dtg-traceql",
    );
    const snippets = (
      pkg.contributes as { snippets?: Array<{ language: string; path: string }> }
    ).snippets?.find((s) => s.language === "dtg-traceql");
    expect(snippets?.path).toBe("./snippets/dtg-traceql.json");
    const defaults = (
      pkg.contributes as { configurationDefaults?: { "files.associations"?: Record<string, string> } }
    ).configurationDefaults;
    expect(defaults?.["files.associations"]?.["*.traceql"]).toBe("dtg-traceql");
    expect(pkg.contributes.commands?.some((c) => c.command === "dtg.traceql.run")).toBe(true);
    expect(
      pkg.contributes.keybindings?.some(
        (k) => k.command === "dtg.traceql.run" && k.when?.includes("dtg-traceql"),
      ),
    ).toBe(true);
  });
});
