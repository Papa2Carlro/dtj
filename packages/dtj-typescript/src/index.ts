/**
 * @dtj/sdk - TypeScript SDK for DTJ (Distributed Tracing Journal)
 * 
 * Thin middleware to local dtj-agent. The SDK never writes .dtj bytes directly.
 * Architecture: TypeScript application → dtj SDK → local dtj-agent → Rust SessionWriter → .dtj
 */

// Errors
export {
  DTJError,
  DTJProtocolError,
  DTJConnectionError,
  DTJAgentNotFoundError,
  DTJValueError,
  DTJSessionError,
} from "./errors.js";

// Protocol constants and types
export {
  PROTOCOL_VERSION,
  Cmd,
  Resp,
  DictKind,
  Severity,
  TypeTag,
  createOpenSessionMetadata,
  encodeFrame,
  decodeFrame,
  encodeHello,
  decodeHelloOk,
  encodeOpenSession,
  encodeIntern,
  decodeInternOk,
  encodeAppendEvent,
  decodeAppendEventOk,
  encodeFinishSession,
  encodePing,
  decodeError,
  encodeValue,
  severityFromString,
} from "./protocol.js";

export type { OpenSessionMetadata } from "./protocol.js";

// Discovery
export { AgentDiscovery } from "./discovery.js";

// Client
export { TraceSession, NoOpTraceSession } from "./client.js";
export type { TraceConfig, TraceEvent } from "./client.js";