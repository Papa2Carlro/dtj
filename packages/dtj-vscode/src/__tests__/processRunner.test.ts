import * as fs from "node:fs";
import * as os from "node:os";
import * as path from "node:path";
import { afterEach, describe, expect, it } from "vitest";

import { defaultProcessRunner } from "../processRunner";

const fixtures: string[] = [];

function writeFixture(source: string): string {
  const file = path.join(
    os.tmpdir(),
    `dtj-vscode-runner-${Date.now()}-${Math.random().toString(16).slice(2)}.mjs`,
  );
  fs.writeFileSync(file, source, "utf8");
  fixtures.push(file);
  return file;
}

afterEach(() => {
  for (const f of fixtures.splice(0)) {
    try {
      fs.unlinkSync(f);
    } catch {
      /* ignore */
    }
  }
});

describe("defaultProcessRunner integration", () => {
  it("kills on timeout and marks timedOut", async () => {
    const script = writeFixture(`
      await new Promise((r) => setTimeout(r, 5000));
      process.stdout.write("late");
    `);
    const res = await defaultProcessRunner({
      executable: process.execPath,
      args: [script],
      timeoutMs: 80,
      stdoutMax: 1024,
      stderrMax: 1024,
    });
    expect(res.timedOut).toBe(true);
    expect(res.killed).toBe(true);
    expect(res.stdout).not.toContain("late");
  });

  it("cancels via AbortSignal and ignores late output", async () => {
    const script = writeFixture(`
      await new Promise((r) => setTimeout(r, 5000));
      process.stdout.write("should-not-appear");
    `);
    const ac = new AbortController();
    const pending = defaultProcessRunner({
      executable: process.execPath,
      args: [script],
      timeoutMs: 10_000,
      stdoutMax: 1024,
      stderrMax: 1024,
      signal: ac.signal,
    });
    await new Promise((r) => setTimeout(r, 40));
    ac.abort();
    const res = await pending;
    expect(res.killed).toBe(true);
    expect(res.stdout).not.toContain("should-not-appear");
  });

  it("caps stdout and kills the child", async () => {
    const script = writeFixture(`
      const chunk = "x".repeat(4096);
      for (let i = 0; i < 64; i++) process.stdout.write(chunk);
    `);
    const res = await defaultProcessRunner({
      executable: process.execPath,
      args: [script],
      timeoutMs: 5000,
      stdoutMax: 2048,
      stderrMax: 1024,
    });
    expect(res.stdoutTruncated).toBe(true);
    expect(res.killed).toBe(true);
    expect(Buffer.byteLength(res.stdout, "utf8")).toBeLessThanOrEqual(2048);
  });
});
