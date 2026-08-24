/**
 * Unit tests for protocol.ts
 */

import { test, describe, beforeEach, afterEach } from "node:test";
import assert from "node:assert/strict";
import {
  PROTOCOL_VERSION,
  Cmd,
  Resp,
  DictKind,
  Severity,
  TypeTag,
  encodeFrame,
  decodeFrame,
  encodeHello,
  decodeHelloOk,
  encodeOpenSession,
  createOpenSessionMetadata,
  encodeIntern,
  decodeInternOk,
  encodeAppendEvent,
  decodeAppendEventOk,
  encodeFinishSession,
  encodePing,
  decodeError,
  encodeValue,
  severityFromString,
} from "../src/protocol.js";
import { DTJProtocolError, DTJValueError } from "../src/errors.js";

describe("protocol", () => {
  describe("encodeFrame / decodeFrame", () => {
    test("encodes and decodes a simple frame", () => {
      const body = new Uint8Array([1, 2, 3, 4]);
      const frame = encodeFrame(0x01, body);
      const { opcode, body: decoded } = decodeFrame(frame);
      assert.equal(opcode, 0x01);
      assert.deepEqual(decoded, body);
    });

    test("frame length includes opcode byte", () => {
      const body = new Uint8Array([0xAA]);
      const frame = encodeFrame(0x02, body);
      // Length = 1 (opcode) + 1 (body) = 2
      const view = new DataView(frame.buffer, frame.byteOffset, 4);
      assert.equal(view.getUint32(0, true), 2);
    });

    test("throws on frame too large", () => {
      const largeBody = new Uint8Array(1_048_576); // > MAX_FRAME_SIZE - 1
      assert.throws(() => encodeFrame(0x01, largeBody), DTJProtocolError);
    });

    test("throws on frame too short", () => {
      assert.throws(() => decodeFrame(new Uint8Array([0, 0, 0, 0])), DTJProtocolError);
    });

    test("throws on truncated frame", () => {
      const frame = encodeFrame(0x01, new Uint8Array([1, 2, 3]));
      const truncated = frame.slice(0, frame.length - 1);
      assert.throws(() => decodeFrame(truncated), DTJProtocolError);
    });
  });

  describe("Hello / HelloOk", () => {
    test("encodeHello produces correct frame", () => {
      const frame = encodeHello();
      const { opcode, body } = decodeFrame(frame);
      assert.equal(opcode, Cmd.HELLO);
      assert.equal(body.length, 4);
      const view = new DataView(body.buffer, body.byteOffset, body.byteLength);
      assert.equal(view.getUint32(0, true), PROTOCOL_VERSION);
    });

    test("decodeHelloOk returns protocol version", () => {
      const body = new Uint8Array(4);
      new DataView(body.buffer).setUint32(0, 1, true);
      assert.equal(decodeHelloOk(body), 1);
    });

    test("decodeHelloOk throws on wrong body length", () => {
      assert.throws(() => decodeHelloOk(new Uint8Array([1, 2])), DTJProtocolError);
    });
  });

  describe("OpenSession metadata", () => {
    test("createOpenSessionMetadata generates valid metadata", () => {
      const meta = createOpenSessionMetadata("test.dtj", "my-service", "1.0.0");
      assert.equal(meta.fileName, "test.dtj");
      assert.equal(meta.producerName, "my-service");
      assert.equal(meta.producerVersion, "1.0.0");
      assert.equal(meta.sessionId.length, 16);
      assert.ok(meta.startUtcUnixMs > 0n);
      assert.ok(meta.monoOriginNs > 0n);
    });

    test("encodeOpenSession produces valid frame", () => {
      const meta = createOpenSessionMetadata("test.dtj", "my-service", "1.0.0");
      const frame = encodeOpenSession(meta);
      const { opcode, body } = decodeFrame(frame);
      assert.equal(opcode, Cmd.OPEN_SESSION);
      assert.ok(body.length > 0);
    });

    test("encodeOpenSession validates producerName length", () => {
      const meta = createOpenSessionMetadata("test.dtj", "x".repeat(33), "1.0.0");
      assert.throws(() => encodeOpenSession(meta), /producerName must be <= 32 bytes/);
    });

    test("encodeOpenSession validates producerVersion length", () => {
      const meta = createOpenSessionMetadata("test.dtj", "my-service", "x".repeat(17));
      assert.throws(() => encodeOpenSession(meta), /producerVersion must be <= 16 bytes/);
    });

    test("encodeOpenSession validates sessionId length", () => {
      const meta = createOpenSessionMetadata("test.dtj", "my-service", "1.0.0");
      meta.sessionId = new Uint8Array(15);
      assert.throws(() => encodeOpenSession(meta), /sessionId must be 16 bytes/);
    });
  });

  describe("Intern / InternOk", () => {
    test("encodeIntern produces valid frame", () => {
      const frame = encodeIntern(DictKind.DOMAIN, "api");
      const { opcode, body } = decodeFrame(frame);
      assert.equal(opcode, Cmd.INTERN);
      assert.equal(body[0], DictKind.DOMAIN);
      // name_len = 3
      const view = new DataView(body.buffer, 1, 2);
      assert.equal(view.getUint16(0, true), 3);
      assert.equal(new TextDecoder().decode(body.slice(3)), "api");
    });

    test("encodeIntern throws on name too long", () => {
      assert.throws(() => encodeIntern(DictKind.DOMAIN, "x".repeat(1025)), /name too long/);
    });

    test("decodeInternOk returns dictionary ID", () => {
      const body = new Uint8Array(4);
      new DataView(body.buffer).setUint32(0, 42, true);
      assert.equal(decodeInternOk(body), 42);
    });

    test("decodeInternOk throws on wrong body length", () => {
      assert.throws(() => decodeInternOk(new Uint8Array([1, 2, 3])), DTJProtocolError);
    });
  });

  describe("encodeValue", () => {
    test("encodes boolean", () => {
      const { typeTag, body } = encodeValue(true);
      assert.equal(typeTag, TypeTag.BOOL);
      assert.deepEqual(body, new Uint8Array([1]));

      const { typeTag: t2, body: b2 } = encodeValue(false);
      assert.equal(t2, TypeTag.BOOL);
      assert.deepEqual(b2, new Uint8Array([0]));
    });

    test("encodes bigint as I64", () => {
      const { typeTag, body } = encodeValue(12345n);
      assert.equal(typeTag, TypeTag.I64);
      assert.equal(body.length, 8);
      const view = new DataView(body.buffer);
      assert.equal(view.getBigInt64(0, true), 12345n);
    });

    test("encodes integer number as I64", () => {
      const { typeTag, body } = encodeValue(42);
      assert.equal(typeTag, TypeTag.I64);
      const view = new DataView(body.buffer);
      assert.equal(view.getBigInt64(0, true), 42n);
    });

    test("encodes non-integer number as F64", () => {
      const { typeTag, body } = encodeValue(3.14);
      assert.equal(typeTag, TypeTag.F64);
      const view = new DataView(body.buffer);
      assert.equal(view.getFloat64(0, true), 3.14);
    });

    test("encodes Uint8Array as BYTES", () => {
      const data = new Uint8Array([1, 2, 3, 4]);
      const { typeTag, body } = encodeValue(data);
      assert.equal(typeTag, TypeTag.BYTES);
      assert.equal(body.length, 4 + 4); // length prefix + data
      const view = new DataView(body.buffer);
      assert.equal(view.getUint32(0, true), 4);
      assert.deepEqual(body.slice(4), data);
    });

    test("throws on string (must use dictionary)", () => {
      assert.throws(() => encodeValue("hello"), DTJValueError);
    });

    test("throws on unsupported type", () => {
      assert.throws(() => encodeValue({}), DTJValueError);
      assert.throws(() => encodeValue(null), DTJValueError);
      assert.throws(() => encodeValue(undefined), DTJValueError);
    });

    test("throws on non-finite number", () => {
      assert.throws(() => encodeValue(NaN), DTJValueError);
      assert.throws(() => encodeValue(Infinity), DTJValueError);
      assert.throws(() => encodeValue(-Infinity), DTJValueError);
    });
  });

  describe("AppendEvent", () => {
    test("encodeAppendEvent produces valid frame", () => {
      const frame = encodeAppendEvent(
        1000n,
        1, 2, 3, 4,
        Severity.INFO,
        5,
        TypeTag.I64,
        new Uint8Array([0, 0, 0, 0, 0, 0, 0, 42])
      );
      const { opcode, body } = decodeFrame(frame);
      assert.equal(opcode, Cmd.APPEND_EVENT);
      // Verify structure: 8 + 16 + 1 + 2 + 4 + 1 + 3 + 8 = 43 bytes body
      assert.equal(body.length, 43);
    });

    test("decodeAppendEventOk returns event sequence", () => {
      const body = new Uint8Array(8);
      new DataView(body.buffer).setBigUint64(0, 123n, true);
      assert.equal(decodeAppendEventOk(body), 123n);
    });

    test("decodeAppendEventOk throws on wrong body length", () => {
      assert.throws(() => decodeAppendEventOk(new Uint8Array([1, 2, 3])), DTJProtocolError);
    });
  });

  describe("FinishSession / Ping", () => {
    test("encodeFinishSession produces empty body frame", () => {
      const frame = encodeFinishSession();
      const { opcode, body } = decodeFrame(frame);
      assert.equal(opcode, Cmd.FINISH_SESSION);
      assert.equal(body.length, 0);
    });

    test("encodePing produces empty body frame", () => {
      const frame = encodePing();
      const { opcode, body } = decodeFrame(frame);
      assert.equal(opcode, Cmd.PING);
      assert.equal(body.length, 0);
    });
  });

  describe("decodeError", () => {
    test("decodes error message", () => {
      const msg = "Something went wrong";
      const body = new TextEncoder().encode(msg);
      assert.equal(decodeError(body), msg);
    });
  });

  describe("severityFromString", () => {
    test("maps severity strings correctly", () => {
      assert.equal(severityFromString("debug"), Severity.DEBUG);
      assert.equal(severityFromString("info"), Severity.INFO);
      assert.equal(severityFromString("warn"), Severity.WARN);
      assert.equal(severityFromString("error"), Severity.ERROR);
      assert.equal(severityFromString("fatal"), Severity.FATAL);
      assert.equal(severityFromString("DEBUG"), Severity.DEBUG);
      assert.equal(severityFromString("WARN"), Severity.WARN);
    });

    test("throws on invalid severity", () => {
      assert.throws(() => severityFromString("invalid"), DTJValueError);
      assert.throws(() => severityFromString(""), DTJValueError);
    });
  });
});