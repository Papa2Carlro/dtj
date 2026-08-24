/**
 * Unit tests for discovery.ts
 */

import { test, describe, beforeEach, afterEach, mock } from "node:test";
import * as assert from "node:assert/strict";
import { AgentDiscovery } from "../src/discovery.js";
import { DTJAgentNotFoundError, DTJConnectionError } from "../src/errors.js";
import { existsSync, mkdirSync, rmSync, writeFileSync, chmodSync } from "node:fs";
import { join } from "node:path";
import { tmpdir } from "node:os";

// Local type for process.emitWarning options
interface ProcessWarningOptions {
  type?: string;
  code?: string;
  detail?: string;
  constructor?: Function;
}

interface ProcessWarning extends Error {
  type?: string;
  code?: string;
  detail?: string;
}

describe("AgentDiscovery", () => {
  let originalPath: string | undefined;
  let originalAgentPath: string | undefined;
  let testDir: string;
  let fakeAgentPath: string;

  beforeEach(() => {
    originalPath = process.env["PATH"];
    originalAgentPath = process.env["DTJ_AGENT_PATH"];
    testDir = join(tmpdir(), `dtj-test-${Date.now()}`);
    mkdirSync(testDir, { recursive: true });
    fakeAgentPath = join(testDir, "dtj-agent");
    // Create a fake executable
    writeFileSync(fakeAgentPath, "#!/bin/sh\necho 'fake agent'");
    chmodSync(fakeAgentPath, 0o755);
  });

  afterEach(() => {
    process.env["PATH"] = originalPath;
    process.env["DTJ_AGENT_PATH"] = originalAgentPath;
    try {
      rmSync(testDir, { recursive: true, force: true });
    } catch {}
  });

  describe("findAgent", () => {
    test("returns explicit agentPath if valid", () => {
      const discovery = new AgentDiscovery({ agentPath: fakeAgentPath });
      assert.equal(discovery.findAgent(), fakeAgentPath);
    });

    test("returns null if explicit agentPath not found", () => {
      const discovery = new AgentDiscovery({ agentPath: "/nonexistent/dtj-agent" });
      assert.equal(discovery.findAgent(), null);
    });

    test("returns DTJ_AGENT_PATH if set and valid", () => {
      process.env["DTJ_AGENT_PATH"] = fakeAgentPath;
      const discovery = new AgentDiscovery({});
      assert.equal(discovery.findAgent(), fakeAgentPath);
    });

    test("returns PATH lookup if found", () => {
      process.env["PATH"] = `${testDir}:${originalPath}`;
      const discovery = new AgentDiscovery({});
      assert.equal(discovery.findAgent(), fakeAgentPath);
    });

    test("returns null if not found anywhere", () => {
      process.env["PATH"] = "/empty/path";
      process.env["DTJ_AGENT_PATH"] = "";
      const discovery = new AgentDiscovery({});
      assert.equal(discovery.findAgent(), null);
    });

    test("explicit agentPath takes precedence over DTJ_AGENT_PATH", () => {
      process.env["DTJ_AGENT_PATH"] = "/other/dtj-agent";
      const discovery = new AgentDiscovery({ agentPath: fakeAgentPath });
      assert.equal(discovery.findAgent(), fakeAgentPath);
    });

    test("DTJ_AGENT_PATH takes precedence over PATH", () => {
      process.env["DTJ_AGENT_PATH"] = fakeAgentPath;
      process.env["PATH"] = "/empty/path";
      const discovery = new AgentDiscovery({});
      assert.equal(discovery.findAgent(), fakeAgentPath);
    });
  });

  describe("startAgent", () => {
    test("returns socketPath if provided (connect to existing)", () => {
      const discovery = new AgentDiscovery({ socketPath: "/tmp/existing.sock" });
      assert.equal(discovery.startAgent(), "/tmp/existing.sock");
    });

    test("throws DTJAgentNotFoundError if no agent found", () => {
      process.env["PATH"] = "/empty/path";
      const discovery = new AgentDiscovery({ dataDir: testDir });
      assert.throws(() => discovery.startAgent(), DTJAgentNotFoundError);
    });

    test("creates temp socket directory and returns socket path", () => {
      const discovery = new AgentDiscovery({ agentPath: fakeAgentPath, dataDir: testDir });
      const socketPath = discovery.startAgent();
      assert.ok(socketPath.includes("dtj-agent-"));
      assert.ok(socketPath.endsWith("agent.sock"));
      assert.ok(existsSync(socketPath.replace("agent.sock", "")));
    });

    test("creates dataDir if not exists", () => {
      const newDataDir = join(testDir, "new-data");
      const discovery = new AgentDiscovery({ agentPath: fakeAgentPath, dataDir: newDataDir });
      discovery.startAgent();
      assert.ok(existsSync(newDataDir));
    });
  });

  describe("stopAgent", () => {
    test("stops agent process and cleans up", async () => {
      const discovery = new AgentDiscovery({ agentPath: fakeAgentPath, dataDir: testDir });
      const socketPath = discovery.startAgent();
      const socketDir = socketPath.replace("agent.sock", "");
      assert.ok(existsSync(socketDir));

      discovery.stopAgent();

      // Wait for cleanup with retries
      let cleanedUp = false;
      for (let i = 0; i < 50; i++) {
        await new Promise(r => setTimeout(r, 50));
        if (!existsSync(socketDir)) {
          cleanedUp = true;
          break;
        }
      }
      assert.ok(cleanedUp, `Socket directory ${socketDir} was not cleaned up`);
    });

    test("is idempotent", () => {
      const discovery = new AgentDiscovery({ agentPath: fakeAgentPath, dataDir: testDir });
      discovery.startAgent();
      discovery.stopAgent();
      discovery.stopAgent(); // Should not throw
    });
  });

  describe("warning emission", () => {
    test("emits warning only once when agent not found", () => {
      const warnings: ProcessWarning[] = [];
      const originalEmitWarning = process.emitWarning;
      process.emitWarning = ((warning: string | Error, options?: ProcessWarningOptions) => {
        if (typeof warning === "string") {
          warnings.push({ message: warning, name: "Warning", ...options } as ProcessWarning);
        } else {
          warnings.push({ message: warning.message, name: warning.name, ...options } as ProcessWarning);
        }
      }) as typeof process.emitWarning;

      process.env["PATH"] = "/empty/path";
      const discovery = new AgentDiscovery({ dataDir: testDir });
      
      // First call
      try { discovery.startAgent(); } catch {}
      // Second call
      try { discovery.startAgent(); } catch {}

      assert.equal(warnings.length, 1);
      assert.ok(warnings[0]!.message.includes("tracing disabled"));
      assert.equal(warnings[0]!.type, "DTJWarning");
      assert.equal(warnings[0]!.code, "DTJ_AGENT_UNAVAILABLE");

      process.emitWarning = originalEmitWarning;
    });
  });
});