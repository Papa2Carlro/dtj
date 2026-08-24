/**
 * Opt-in E2E test for dtj-sdk.
 * Only runs when DTJ_RUN_AGENT_E2E=1 environment variable is set.
 * Requires a working dtj-agent binary and Unix socket support.
 */

import { test, describe, before, after } from "node:test";
import assert from "node:assert/strict";
import { TraceSession } from "../src/client.js";
import { existsSync, readFileSync, rmSync } from "node:fs";
import { join } from "node:path";
import { tmpdir } from "node:os";

// Skip entire test suite if not opted in
const runE2E = process.env["DTJ_RUN_AGENT_E2E"] === "1";

if (!runE2E) {
  console.log("Skipping E2E tests. Set DTJ_RUN_AGENT_E2E=1 to run.");
  process.exit(0);
}

describe("E2E TraceSession", () => {
  let testDataDir: string;
  let sessionFile: string;

  before(() => {
    testDataDir = join(tmpdir(), `dtj-e2e-${Date.now()}`);
  });

  after(() => {
    try {
      rmSync(testDataDir, { recursive: true, force: true });
    } catch {}
  });

  test("creates trace session and emits event", async () => {
    const session = await TraceSession.open({
      producerName: "e2e-test",
      producerVersion: "0.1.0",
      dataDir: testDataDir,
      sessionFileName: "e2e-session.dtj",
    });

    // Should get a real TraceSession, not NoOpTraceSession
    assert.ok(session.constructor.name === "TraceSession");

    // Emit an event
    await session.emit({
      domain: "api",
      category: "request",
      name: "completed",
      severity: "info",
      fieldName: "duration_ms",
      value: 12.5,
      correlation: "request-42",
    });

    // Close session
    await session.close();

    // Verify .dtj file was created
    sessionFile = join(testDataDir, "e2e-session.dtj");
    assert.ok(existsSync(sessionFile), "Session file should exist");

    // Verify file has content
    const fileContent = readFileSync(sessionFile);
    assert.ok(fileContent.length > 0, "Session file should not be empty");

    // Verify it's a valid DTJ file by checking magic bytes
    // DTJ v1 magic: "DTJ\1" (0x44 0x54 0x4A 0x01)
    assert.equal(fileContent[0], 0x44); // 'D'
    assert.equal(fileContent[1], 0x54); // 'T'
    assert.equal(fileContent[2], 0x4A); // 'J'
    assert.equal(fileContent[3], 0x01); // version 1
  });

  test("emits multiple events with different value types", async () => {
    const session = await TraceSession.open({
      producerName: "e2e-test",
      producerVersion: "0.1.0",
      dataDir: testDataDir,
      sessionFileName: "e2e-multi.dtj",
    });

    // Emit boolean
    await session.emit({
      domain: "test",
      category: "bool",
      name: "flag",
      severity: "info",
      fieldName: "enabled",
      value: true,
    });

    // Emit integer
    await session.emit({
      domain: "test",
      category: "int",
      name: "count",
      severity: "info",
      fieldName: "value",
      value: 42,
    });

    // Emit bigint
    await session.emit({
      domain: "test",
      category: "bigint",
      name: "large",
      severity: "info",
      fieldName: "value",
      value: 123456789012345n,
    });

    // Emit float
    await session.emit({
      domain: "test",
      category: "float",
      name: "ratio",
      severity: "info",
      fieldName: "value",
      value: 3.14159,
    });

    // Emit bytes
    await session.emit({
      domain: "test",
      category: "bytes",
      name: "payload",
      severity: "info",
      fieldName: "data",
      value: new Uint8Array([0xDE, 0xAD, 0xBE, 0xEF]),
    });

    await session.close();

    const sessionFile = join(testDataDir, "e2e-multi.dtj");
    assert.ok(existsSync(sessionFile));
    const fileContent = readFileSync(sessionFile);
    assert.ok(fileContent.length > 0);
    assert.equal(fileContent[0], 0x44);
    assert.equal(fileContent[1], 0x54);
    assert.equal(fileContent[2], 0x4A);
    assert.equal(fileContent[3], 0x01);
  });

  test("handles different severity levels", async () => {
    const session = await TraceSession.open({
      producerName: "e2e-test",
      producerVersion: "0.1.0",
      dataDir: testDataDir,
      sessionFileName: "e2e-severity.dtj",
    });

    const severities = ["debug", "info", "warn", "error", "fatal"] as const;
    for (const sev of severities) {
      await session.emit({
        domain: "test",
        category: "severity",
        name: sev,
        severity: sev,
        fieldName: "level",
        value: sev,
      });
    }

    await session.close();

    const sessionFile = join(testDataDir, "e2e-severity.dtj");
    assert.ok(existsSync(sessionFile));
  });

  test("idempotent close", async () => {
    const session = await TraceSession.open({
      producerName: "e2e-test",
      producerVersion: "0.1.0",
      dataDir: testDataDir,
      sessionFileName: "e2e-close.dtj",
    });

    await session.close();
    await session.close(); // Should not throw
    assert.ok(true);
  });
});