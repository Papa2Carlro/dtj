/**
 * Binary protocol implementation for dtj-agent communication.
 * Frame layout: 4 bytes LE frame_length + 1 byte opcode + N bytes payload
 */

import { DTJProtocolError, DTJValueError } from "./errors.js";

// Protocol version
export const PROTOCOL_VERSION = 1;

// Command opcodes (client -> server)
export const enum Cmd {
  HELLO = 0x01,
  OPEN_SESSION = 0x02,
  APPEND_EVENT = 0x03,
  FINISH_SESSION = 0x04,
  PING = 0x05,
  INTERN = 0x06,
}

// Response opcodes (server -> client)
export const enum Resp {
  HELLO_OK = 0x81,
  OPEN_SESSION_OK = 0x82,
  APPEND_EVENT_OK = 0x83,
  FINISH_SESSION_OK = 0x84,
  PONG = 0x85,
  INTERN_OK = 0x86,
  ERROR = 0xFF,
}

// Dictionary kinds
export const enum DictKind {
  DOMAIN = 1,
  CATEGORY = 2,
  EVENT_NAME = 3,
  STRING = 4,
}

// Severity mapping (matches dtj::Severity)
export const enum Severity {
  DEBUG = 0,
  INFO = 1,
  WARN = 2,
  ERROR = 3,
  FATAL = 4,
}

// Type tags (match dtj::Value)
export const enum TypeTag {
  BOOL = 0x01,
  I32 = 0x02,
  I64 = 0x03,
  U32 = 0x04,
  U64 = 0x05,
  F32 = 0x06,
  F64 = 0x07,
  ENUM = 0x08,
  VEC2_F32 = 0x09,
  VEC3_F32 = 0x0A,
  INTERNED = 0x0B,
  BYTES = 0x0C,
}

const MAX_FRAME_SIZE = 1_048_576; // 1 MiB

/**
 * Encode a length-prefixed frame: 4-byte LE length + opcode + body.
 * frame_length includes the opcode byte (1 + payload.len()).
 */
export function encodeFrame(opcode: number, body: Uint8Array): Uint8Array {
  const length = 1 + body.length;
  if (length > MAX_FRAME_SIZE) {
    throw new DTJProtocolError(`Frame too large: ${length} > ${MAX_FRAME_SIZE}`);
  }
  const frame = new Uint8Array(4 + length);
  const view = new DataView(frame.buffer);
  view.setUint32(0, length, true); // LE
  frame[4] = opcode;
  frame.set(body, 5);
  return frame;
}

/**
 * Decode a frame, returning { opcode, body }.
 */
export function decodeFrame(data: Uint8Array): { opcode: number; body: Uint8Array } {
  if (data.length < 5) {
    throw new DTJProtocolError("Frame too short");
  }
  const view = new DataView(data.buffer, data.byteOffset, data.byteLength);
  const length = view.getUint32(0, true); // LE
  if (length > MAX_FRAME_SIZE) {
    throw new DTJProtocolError(`Frame too large: ${length}`);
  }
  if (data.length < 4 + length) {
    throw new DTJProtocolError("Frame truncated");
  }
  const opcode = data[4]!; // We know data.length >= 5 from check above
  const body = data.slice(5, 5 + length - 1);
  return { opcode, body };
}

/**
 * Encode Hello frame with protocol version.
 */
export function encodeHello(): Uint8Array {
  const body = new Uint8Array(4);
  const view = new DataView(body.buffer);
  view.setUint32(0, PROTOCOL_VERSION, true);
  return encodeFrame(Cmd.HELLO, body);
}

/**
 * Decode HelloOk response, return protocol version.
 */
export function decodeHelloOk(body: Uint8Array): number {
  if (body.length !== 4) {
    throw new DTJProtocolError("HelloOk body must be 4 bytes");
  }
  const view = new DataView(body.buffer, body.byteOffset, body.byteLength);
  return view.getUint32(0, true);
}

/**
 * OpenSession metadata (sent by client, no 128-byte FileHeader).
 */
export interface OpenSessionMetadata {
  fileName: string;
  sessionId: Uint8Array; // 16 bytes
  startUtcUnixMs: bigint;
  monoOriginNs: bigint;
  producerName: string;
  producerVersion: string;
}

/**
 * Create OpenSession metadata with generated sessionId and timestamps.
 */
export function createOpenSessionMetadata(
  fileName: string,
  producerName: string,
  producerVersion: string
): OpenSessionMetadata {
  const sessionId = crypto.getRandomValues(new Uint8Array(16));
  const startUtcUnixMs = BigInt(Date.now());
  const monoOriginNs = process.hrtime.bigint();
  return {
    fileName,
    sessionId,
    startUtcUnixMs,
    monoOriginNs,
    producerName,
    producerVersion,
  };
}

/**
 * Encode OpenSession metadata payload.
 */
export function encodeOpenSession(metadata: OpenSessionMetadata): Uint8Array {
  // Validate lengths
  if (metadata.sessionId.length !== 16) {
    throw new Error("sessionId must be 16 bytes");
  }
  const producerNameBytes = new TextEncoder().encode(metadata.producerName);
  if (producerNameBytes.length > 32) {
    throw new Error("producerName must be <= 32 bytes");
  }
  const producerVersionBytes = new TextEncoder().encode(metadata.producerVersion);
  if (producerVersionBytes.length > 16) {
    throw new Error("producerVersion must be <= 16 bytes");
  }

  const fileNameBytes = new TextEncoder().encode(metadata.fileName);
  const body = new Uint8Array(
    2 + fileNameBytes.length +
    16 +
    8 + 8 +
    2 + producerNameBytes.length +
    2 + producerVersionBytes.length
  );
  let offset = 0;
  const view = new DataView(body.buffer, body.byteOffset, body.byteLength);

  // file_name_len (u16 LE) + file_name
  view.setUint16(offset, fileNameBytes.length, true);
  offset += 2;
  body.set(fileNameBytes, offset);
  offset += fileNameBytes.length;

  // session_id (16 bytes)
  body.set(metadata.sessionId, offset);
  offset += 16;

  // start_utc_unix_ms (i64 LE)
  view.setBigInt64(offset, metadata.startUtcUnixMs, true);
  offset += 8;

  // mono_origin_ns (u64 LE)
  view.setBigUint64(offset, metadata.monoOriginNs, true);
  offset += 8;

  // producer_name_len (u16 LE) + producer_name
  view.setUint16(offset, producerNameBytes.length, true);
  offset += 2;
  body.set(producerNameBytes, offset);
  offset += producerNameBytes.length;

  // producer_version_len (u16 LE) + producer_version
  view.setUint16(offset, producerVersionBytes.length, true);
  offset += 2;
  body.set(producerVersionBytes, offset);

  return encodeFrame(Cmd.OPEN_SESSION, body);
}

/**
 * Encode Intern request.
 */
export function encodeIntern(kind: DictKind, name: string): Uint8Array {
  const nameBytes = new TextEncoder().encode(name);
  if (nameBytes.length > 1024) {
    throw new Error("name too long (max 1024 bytes)");
  }
  const body = new Uint8Array(1 + 2 + nameBytes.length);
  body[0] = kind;
  const view = new DataView(body.buffer, 1, 2);
  view.setUint16(0, nameBytes.length, true);
  body.set(nameBytes, 3);
  return encodeFrame(Cmd.INTERN, body);
}

/**
 * Decode InternOk response, return dictionary ID.
 */
export function decodeInternOk(body: Uint8Array): number {
  if (body.length !== 4) {
    throw new DTJProtocolError("InternOk body must be 4 bytes");
  }
  const view = new DataView(body.buffer, body.byteOffset, body.byteLength);
  return view.getUint32(0, true);
}

/**
 * Encode value body for a given type tag.
 */
export function encodeValue(value: unknown): { typeTag: TypeTag; body: Uint8Array } {
  // boolean
  if (typeof value === "boolean") {
    return { typeTag: TypeTag.BOOL, body: new Uint8Array([value ? 1 : 0]) };
  }

  // bigint (signed i64 range)
  if (typeof value === "bigint") {
    const body = new Uint8Array(8);
    const view = new DataView(body.buffer);
    view.setBigInt64(0, value, true);
    return { typeTag: TypeTag.I64, body };
  }

  // number
  if (typeof value === "number") {
    if (!Number.isFinite(value)) {
      throw new DTJValueError(`Non-finite number not supported: ${value}`);
    }
    if (Number.isInteger(value)) {
      // Check safe integer range for i64
      if (value >= -0x8000000000000000n && value <= 0x7fffffffffffffffn) {
        const body = new Uint8Array(8);
        const view = new DataView(body.buffer);
        view.setBigInt64(0, BigInt(value), true);
        return { typeTag: TypeTag.I64, body };
      }
      // Out of i64 range - use f64
      const body = new Uint8Array(8);
      const view = new DataView(body.buffer);
      view.setFloat64(0, value, true);
      return { typeTag: TypeTag.F64, body };
    }
    // Non-integer -> f64
    const body = new Uint8Array(8);
    const view = new DataView(body.buffer);
    view.setFloat64(0, value, true);
    return { typeTag: TypeTag.F64, body };
  }

  // string -> INTERNED (handled separately via dictionary)
  if (typeof value === "string") {
    throw new DTJValueError("String values must be interned via dictionary (use fieldName for string values)");
  }

  // Uint8Array -> BYTES
  if (value instanceof Uint8Array) {
    const body = new Uint8Array(4 + value.length);
    const view = new DataView(body.buffer);
    view.setUint32(0, value.length, true);
    body.set(value, 4);
    return { typeTag: TypeTag.BYTES, body };
  }

  throw new DTJValueError(`Unsupported value type: ${typeof value}`);
}

/**
 * Encode AppendEvent with single field (MVP).
 */
export function encodeAppendEvent(
  monotonicNs: bigint,
  domainId: number,
  categoryId: number,
  eventNameId: number,
  correlationId: number,
  severity: Severity,
  fieldNameId: number,
  typeTag: TypeTag,
  valueBody: Uint8Array
): Uint8Array {
  // Calculate body size:
  // 8 (monotonic_ns) + 4*4 (ids) + 1 (severity) + 2 (field_count) + 4 (field_name_id) + 1 (type_tag) + 3 (reserved) + value_body
  const body = new Uint8Array(8 + 16 + 1 + 2 + 4 + 1 + 3 + valueBody.length);
  let offset = 0;
  const view = new DataView(body.buffer, body.byteOffset, body.byteLength);

  view.setBigUint64(offset, monotonicNs, true);
  offset += 8;

  view.setUint32(offset, domainId, true);
  offset += 4;
  view.setUint32(offset, categoryId, true);
  offset += 4;
  view.setUint32(offset, eventNameId, true);
  offset += 4;
  view.setUint32(offset, correlationId, true);
  offset += 4;

  body[offset] = severity;
  offset += 1;

  view.setUint16(offset, 1, true); // field_count = 1
  offset += 2;

  view.setUint32(offset, fieldNameId, true);
  offset += 4;

  body[offset] = typeTag;
  offset += 1;

  // reserved 3 bytes (already zero)

  offset += 3;

  body.set(valueBody, offset);

  return encodeFrame(Cmd.APPEND_EVENT, body);
}

/**
 * Decode AppendEventOk response, return event sequence.
 */
export function decodeAppendEventOk(body: Uint8Array): bigint {
  if (body.length !== 8) {
    throw new DTJProtocolError("AppendEventOk body must be 8 bytes");
  }
  const view = new DataView(body.buffer, body.byteOffset, body.byteLength);
  return view.getBigUint64(0, true);
}

/**
 * Encode FinishSession (empty body).
 */
export function encodeFinishSession(): Uint8Array {
  return encodeFrame(Cmd.FINISH_SESSION, new Uint8Array(0));
}

/**
 * Encode Ping (empty body).
 */
export function encodePing(): Uint8Array {
  return encodeFrame(Cmd.PING, new Uint8Array(0));
}

/**
 * Decode Error frame, return error message.
 */
export function decodeError(body: Uint8Array): string {
  return new TextDecoder().decode(body);
}

/**
 * Severity string to enum mapping.
 */
export function severityFromString(severity: string): Severity {
  const map: Record<string, Severity> = {
    debug: Severity.DEBUG,
    info: Severity.INFO,
    warn: Severity.WARN,
    error: Severity.ERROR,
    fatal: Severity.FATAL,
  };
  const s = severity.toLowerCase();
  if (!(s in map)) {
    throw new DTJValueError(`Invalid severity: ${severity}. Must be one of: debug, info, warn, error, fatal`);
  }
  return map[s]!; // Type assertion - we verified s in map above
}