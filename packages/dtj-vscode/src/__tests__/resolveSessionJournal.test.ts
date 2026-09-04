import { readFileSync } from "node:fs";
import { join } from "node:path";
import { describe, expect, it } from "vitest";

import { siblingDtjFsPath } from "../resolveSessionJournal";

const root = join(__dirname, "../..");

describe("siblingDtjFsPath", () => {
  it("maps basename.traceql → basename.dtj", () => {
    expect(siblingDtjFsPath("/repo/fixtures/minimal_session.traceql")).toBe(
      "/repo/fixtures/minimal_session.dtj",
    );
  });

  it("returns null for non-.traceql paths", () => {
    expect(siblingDtjFsPath("/repo/fixtures/minimal_session.dtj")).toBeNull();
    expect(siblingDtjFsPath("/repo/fixtures/note.md")).toBeNull();
  });

  it("registers Run against… command", () => {
    const pkg = JSON.parse(readFileSync(join(root, "package.json"), "utf8")) as {
      contributes: { commands?: Array<{ command: string }> };
    };
    expect(pkg.contributes.commands?.some((c) => c.command === "dtg.traceql.runAgainst")).toBe(
      true,
    );
  });
});
