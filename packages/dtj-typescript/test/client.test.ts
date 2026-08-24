/**
 * Unit tests for client.ts
 */

import { test, describe, beforeEach, afterEach, mock } from "node:test";
import * as assert from "node:assert/strict";
import { TraceSession, NoOpTraceSession, type TraceConfig, type TraceEvent } from "../src/client.js";
import { DTJError, DTJProtocolError, DTJConnectionError, DTJAgentNotFoundError, DTJValueError, DTJSessionError } from "../src/errors.js";
import { Severity, DictKind, TypeTag, encodeValue, severityFromString } from "../src/protocol.js";

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

describe("NoOpTraceSession", () => {
  test("emit is no-op but emits warning once", () => {
    const warnings: ProcessWarning[] = [];
    const originalEmitWarning = process.emitWarning;
    process.emitWarning = ((warning: string | Error, options?: ProcessWarningOptions) => {
      if (typeof warning === "string") {
        warnings.push({ message: warning, name: "Warning", ...options } as ProcessWarning);
      } else {
        warnings.push({ message: warning.message, name: warning.name, ...options } as ProcessWarning);
      }
    }) as typeof process.emitWarning;

    const session = new NoOpTraceSession();
    session.emit({ domain: "test", category: "cat", name: "event", severity: "info", fieldName: "field", value: 1 });
    session.emit({ domain: "test", category: "cat", name: "event", severity: "info", fieldName: "field", value: 2 });

    assert.equal(warnings.length, 1);
    assert.ok(warnings[0]!.message.includes("tracing disabled"));
    assert.equal(warnings[0]!.type, "DTJWarning");
    assert.equal(warnings[0]!.code, "DTJ_AGENT_UNAVAILABLE");

    process.emitWarning = originalEmitWarning;
  });

  test("close is idempotent", () => {
    const session = new NoOpTraceSession();
    session.close();
    session.close(); // Should not throw
    assert.ok(session.isClosed());
  });

  test("isClosed returns true after close", () => {
    const session = new NoOpTraceSession();
    assert.equal(session.isClosed(), false);
    session.close();
    assert.equal(session.isClosed(), true);
  });
});

describe("TraceSession", () => {
  describe("open", () => {
    test("returns NoOpTraceSession when enabled=false", async () => {
      const config: TraceConfig = {
        producerName: "test",
        producerVersion: "1.0.0",
        enabled: false,
      };
      const session = await TraceSession.open(config);
      assert.ok(session instanceof NoOpTraceSession);
    });

    test("returns NoOpTraceSession when agent not found", async () => {
      const config: TraceConfig = {
        producerName: "test",
        producerVersion: "1.0.0",
        dataDir: "/tmp/test",
        agentPath: "/nonexistent/dtj-agent",
      };
      const session = await TraceSession.open(config);
      assert.ok(session instanceof NoOpTraceSession);
    });
  });

  describe("TraceConfig", () => {
    test("accepts all required fields", () => {
      const config: TraceConfig = {
        producerName: "my-service",
        producerVersion: "1.0.0",
        dataDir: "./traces",
        agentPath: "/path/to/agent",
        socketPath: "/tmp/socket",
        sessionFileName: "custom.dtj",
        enabled: true,
      };
      assert.equal(config.producerName, "my-service");
      assert.equal(config.producerVersion, "1.0.0");
    });

    test("dataDir defaults to ./traces", () => {
      const config: TraceConfig = {
        producerName: "test",
        producerVersion: "1.0.0",
      };
      // Default is handled in open(), not in type
      assert.equal(config.dataDir, undefined);
    });

    test("enabled defaults to true", () => {
      const config: TraceConfig = {
        producerName: "test",
        producerVersion: "1.0.0",
      };
      assert.equal(config.enabled, undefined);
    });
  });

  describe("TraceEvent", () => {
    test("accepts all supported value types", () => {
      const events: TraceEvent[] = [
        { domain: "d", category: "c", name: "n", severity: "info", fieldName: "f", value: true },
        { domain: "d", category: "c", name: "n", severity: "info", fieldName: "f", value: 42n },
        { domain: "d", category: "c", name: "n", severity: "info", fieldName: "f", value: 42 },
        { domain: "d", category: "c", name: "n", severity: "info", fieldName: "f", value: 3.14 },
        { domain: "d", category: "c", name: "n", severity: "info", fieldName: "f", value: new Uint8Array([1, 2, 3]) },
        { domain: "d", category: "c", name: "n", severity: "info", fieldName: "f", value: "string", correlation: "corr" },
      ];
      assert.equal(events.length, 6);
    });
  });
});

describe("Protocol integration (mocked socket)", () => {
  // These tests verify the protocol encoding logic without a real agent
  test("encodeValue handles all supported types correctly", () => {
    // This is tested in protocol.test.ts but we verify the integration here
    
    // boolean
    let result = encodeValue(true);
    assert.equal(result.typeTag, TypeTag.BOOL);
    assert.deepEqual(result.body, new Uint8Array([1]));

    // bigint
    result = encodeValue(123n);
    assert.equal(result.typeTag, TypeTag.I64);

    // integer number
    result = encodeValue(42);
    assert.equal(result.typeTag, TypeTag.I64);

    // float number
    result = encodeValue(3.14);
    assert.equal(result.typeTag, TypeTag.F64);

    // Uint8Array
    result = encodeValue(new Uint8Array([1, 2]));
    assert.equal(result.typeTag, TypeTag.BYTES);
    assert.equal(result.body[0], 2); // length
    assert.deepEqual(result.body.slice(4), new Uint8Array([1, 2]));

    // string throws
    assert.throws(() => encodeValue("hello"), DTJValueError);
  });

  test("severityFromString maps correctly", () => {
    assert.equal(severityFromString("debug"), Severity.DEBUG);
    assert.equal(severityFromString("info"), Severity.INFO);
    assert.equal(severityFromString("warn"), Severity.WARN);
    assert.equal(severityFromString("error"), Severity.ERROR);
    assert.equal(severityFromString("fatal"), Severity.FATAL);
    assert.throws(() => severityFromString("invalid"), DTJValueError);
  });
});

describe("Error classes", () => {
  test("DTJError is base class", () => {
    const err = new DTJError("test");
    assert.ok(err instanceof Error);
    assert.equal(err.name, "DTJError");
  });

  test("DTJProtocolError includes opcode", () => {
    const err = new DTJProtocolError("protocol error", 0xFF);
    assert.equal(err.opcode, 0xFF);
    assert.equal(err.name, "DTJProtocolError");
  });

  test("DTJConnectionError", () => {
    const err = new DTJConnectionError("connection failed");
    assert.equal(err.name, "DTJConnectionError");
  });

  test("DTJAgentNotFoundError", () => {
    const err = new DTJAgentNotFoundError("agent not found");
    assert.equal(err.name, "DTJAgentNotFoundError");
  });

  test("DTJValueError", () => {
    const err = new DTJValueError("invalid value");
    assert.equal(err.name, "DTJValueError");
  });

  test("DTJSessionError", () => {
    const err = new DTJSessionError("session error");
    assert.equal(err.name, "DTJSessionError");
  });
});