/**
 * Main client for dtj-agent communication.
 * Provides TraceSession for active tracing and NoOpTraceSession for disabled mode.
 */

import { createInterface, Interface } from "node:readline";
import { connect, Socket } from "node:net";
import { DTJProtocolError, DTJConnectionError, DTJAgentNotFoundError, DTJValueError, DTJSessionError } from "./errors.js";
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
  type OpenSessionMetadata,
} from "./protocol.js";
import { AgentDiscovery } from "./discovery.js";

/**
 * No-op trace session for disabled mode.
 * Emits exactly one warning on first emit() call.
 */
export class NoOpTraceSession {
  private warningEmitted = false;
  private closed = false;

  private emitWarning(): void {
    if (!this.warningEmitted) {
      process.emitWarning(
        "dtj-sdk: tracing disabled (no dtj-agent found). " +
        "Install dtj-agent or set DTJ_AGENT_PATH to enable.",
        {
          type: "DTJWarning",
          code: "DTJ_AGENT_UNAVAILABLE",
        }
      );
      this.warningEmitted = true;
    }
  }

  /**
   * Emit an event (no-op, emits warning once).
   */
  emit(_event: TraceEvent): void {
    this.emitWarning();
  }

  /**
   * Close the session (no-op).
   */
  close(): void {
    this.closed = true;
  }

  /**
   * Check if session is closed.
   */
  isClosed(): boolean {
    return this.closed;
  }

  /**
   * Async iterator support (no-op).
   */
  [Symbol.asyncIterator](): AsyncIterator<never> {
    return {
      next(): Promise<IteratorResult<never>> {
        return Promise.resolve({ done: true, value: undefined });
      },
    };
  }
}

/**
 * Trace event structure for emit().
 */
export interface TraceEvent {
  domain: string;
  category: string;
  name: string;
  severity: "debug" | "info" | "warn" | "error" | "fatal";
  fieldName: string;
  value: boolean | bigint | number | string | Uint8Array;
  correlation?: string;
}

/**
 * Active trace session connected to dtj-agent.
 */
export class TraceSession {
  private socket: Socket | null = null;
  private discovery: AgentDiscovery;
  private metadata: OpenSessionMetadata;
  private closed = false;

  // Dictionary caches: (kind, string) -> id
  private domainCache = new Map<string, number>();
  private categoryCache = new Map<string, number>();
  private eventNameCache = new Map<string, number>();
  private stringCache = new Map<string, number>();

  private constructor(socket: Socket, discovery: AgentDiscovery, metadata: OpenSessionMetadata) {
    this.socket = socket;
    this.discovery = discovery;
    this.metadata = metadata;
  }

  /**
   * Open a new trace session.
   * Returns TraceSession if agent available, NoOpTraceSession otherwise.
   */
  static async open(config: TraceConfig): Promise<TraceSession | NoOpTraceSession> {
    const {
      producerName,
      producerVersion,
      dataDir = "./traces",
      agentPath,
      socketPath,
      sessionFileName,
      enabled = true,
    } = config;

    if (!enabled) {
      return new NoOpTraceSession();
    }

    const discovery = new AgentDiscovery({ agentPath, socketPath, dataDir });

    // Check if agent exists before trying to connect
    const agentBinary = discovery.findAgent();
    if (!agentBinary) {
      return new NoOpTraceSession();
    }

    // Start or connect to agent
    let actualSocketPath: string;
    try {
      actualSocketPath = discovery.startAgent();
    } catch (err) {
      if (err instanceof DTJAgentNotFoundError) {
        return new NoOpTraceSession();
      }
      throw new DTJConnectionError(`Failed to start agent: ${err}`);
    }

    // Connect to socket with retry
    const socket = await TraceSession.connectWithRetry(actualSocketPath);

    try {
      // Hello handshake
      socket.write(encodeHello());
      const helloResponse = await TraceSession.readFrame(socket);
      const { opcode, body } = decodeFrame(helloResponse);
      if (opcode === Resp.ERROR) {
        throw new DTJProtocolError(`Hello failed: ${decodeError(body)}`);
      }
      if (opcode !== Resp.HELLO_OK) {
        throw new DTJProtocolError(`Expected HelloOk, got 0x${opcode.toString(16)}`);
      }
      const version = decodeHelloOk(body);
      if (version !== PROTOCOL_VERSION) {
        throw new DTJProtocolError(`Protocol version mismatch: ${version} != ${PROTOCOL_VERSION}`);
      }

      // Generate metadata
      const fileName = sessionFileName || `session-${Date.now()}.dtj`;
      const metadata = createOpenSessionMetadata(fileName, producerName, producerVersion);

      // OpenSession
      const openFrame = encodeOpenSession(metadata);
      socket.write(openFrame);
      const openResponse = await TraceSession.readFrame(socket);
      const { opcode: openOpcode, body: openBody } = decodeFrame(openResponse);
      if (openOpcode === Resp.ERROR) {
        throw new DTJProtocolError(`OpenSession failed: ${decodeError(openBody)}`);
      }
      if (openOpcode !== Resp.OPEN_SESSION_OK) {
        throw new DTJProtocolError(`Expected OpenSessionOk, got 0x${openOpcode.toString(16)}`);
      }

      return new TraceSession(socket, discovery, metadata);
    } catch (err) {
      discovery.stopAgent();
      socket.destroy();
      throw err;
    }
  }

  /**
   * Connect to Unix socket with retry.
   */
  private static async connectWithRetry(socketPath: string, timeoutMs = 5000): Promise<Socket> {
    const deadline = Date.now() + timeoutMs;
    let lastError: Error | null = null;

    while (Date.now() < deadline) {
      try {
        const socket = connect(socketPath);
        await new Promise<void>((resolve, reject) => {
          socket.once("connect", resolve);
          socket.once("error", reject);
          socket.setTimeout(5000);
          socket.once("timeout", () => reject(new Error("Connection timeout")));
        });
        return socket;
      } catch (err) {
        lastError = err instanceof Error ? err : new Error(String(err));
        await new Promise((r) => setTimeout(r, 10));
      }
    }

    throw new DTJConnectionError(`Failed to connect to agent at ${socketPath}: ${lastError?.message}`);
  }

  /**
   * Read a complete frame from socket.
   */
  private static readFrame(socket: Socket): Promise<Uint8Array> {
    return new Promise((resolve, reject) => {
      const chunks: Uint8Array[] = [];
      let totalLength = 0;
      let expectedLength: number | null = null;

      const onData = (chunk: Buffer) => {
        const uint8Chunk = new Uint8Array(chunk);
        chunks.push(uint8Chunk);
        totalLength += uint8Chunk.length;

        // If we don't know expected length yet, check if we have the header
        if (expectedLength === null && totalLength >= 4) {
          // Combine chunks to read length
          const combined = new Uint8Array(totalLength);
          let offset = 0;
          for (const c of chunks) {
            combined.set(c, offset);
            offset += c.length;
          }
          const view = new DataView(combined.buffer, combined.byteOffset, combined.byteLength);
          expectedLength = view.getUint32(0, true); // LE
          if (expectedLength > 1_048_576) {
            cleanup();
            reject(new DTJProtocolError(`Frame too large: ${expectedLength}`));
            return;
          }
        }

        // Check if we have the complete frame
        if (expectedLength !== null && totalLength >= 4 + expectedLength) {
          cleanup();
          // Combine and return exact frame
          const combined = new Uint8Array(4 + expectedLength);
          let offset = 0;
          for (const c of chunks) {
            const toCopy = Math.min(c.length, combined.length - offset);
            combined.set(c.subarray(0, toCopy), offset);
            offset += toCopy;
            if (offset >= combined.length) break;
          }
          resolve(combined);
        }
      };

      const onError = (err: Error) => {
        cleanup();
        reject(new DTJConnectionError(`Socket error: ${err.message}`));
      };

      const onClose = () => {
        cleanup();
        reject(new DTJConnectionError("Socket closed unexpectedly"));
      };

      function cleanup() {
        socket.off("data", onData);
        socket.off("error", onError);
        socket.off("close", onClose);
      }

      socket.on("data", onData);
      socket.on("error", onError);
      socket.on("close", onClose);
    });
  }

  /**
   * Get or intern a dictionary entry.
   */
  private async getOrIntern(kind: DictKind, name: string, cache: Map<string, number>): Promise<number> {
    if (cache.has(name)) {
      return cache.get(name)!;
    }

    if (!this.socket) {
      throw new DTJSessionError("Session not connected");
    }

    this.socket.write(encodeIntern(kind, name));
    const response = await TraceSession.readFrame(this.socket);
    const { opcode, body } = decodeFrame(response);
    if (opcode === Resp.ERROR) {
      throw new DTJProtocolError(`Intern failed: ${decodeError(body)}`);
    }
    if (opcode !== Resp.INTERN_OK) {
      throw new DTJProtocolError(`Expected InternOk, got 0x${opcode.toString(16)}`);
    }
    const id = decodeInternOk(body);
    cache.set(name, id);
    return id;
  }

  /**
   * Emit a trace event.
   * MVP: exactly one field per event.
   */
  async emit(event: TraceEvent): Promise<void> {
    if (this.closed) {
      throw new DTJSessionError("Session already closed");
    }
    if (!this.socket) {
      throw new DTJSessionError("Session not connected");
    }

    // Get/intern dictionary IDs
    const domainId = await this.getOrIntern(DictKind.DOMAIN, event.domain, this.domainCache);
    const categoryId = await this.getOrIntern(DictKind.CATEGORY, event.category, this.categoryCache);
    const eventNameId = await this.getOrIntern(DictKind.EVENT_NAME, event.name, this.eventNameCache);
    const correlationId = await this.getOrIntern(
      DictKind.STRING,
      event.correlation || "",
      this.stringCache
    );
    const fieldNameId = await this.getOrIntern(DictKind.STRING, event.fieldName, this.stringCache);

    // Encode value
    const { typeTag, body: valueBody } = encodeValue(event.value);

    // Calculate monotonic timestamp
    const monotonicNs = process.hrtime.bigint() - this.metadata.monoOriginNs;

    // Encode and send AppendEvent
    const frame = encodeAppendEvent(
      monotonicNs,
      domainId,
      categoryId,
      eventNameId,
      correlationId,
      severityFromString(event.severity),
      fieldNameId,
      typeTag,
      valueBody
    );

    this.socket.write(frame);

    // Wait for AppendEventOk
    const response = await TraceSession.readFrame(this.socket);
    const { opcode, body } = decodeFrame(response);
    if (opcode === Resp.ERROR) {
      throw new DTJProtocolError(`AppendEvent failed: ${decodeError(body)}`);
    }
    if (opcode !== Resp.APPEND_EVENT_OK) {
      throw new DTJProtocolError(`Expected AppendEventOk, got 0x${opcode.toString(16)}`);
    }
    // Event sequence returned but not used in MVP
    decodeAppendEventOk(body);
  }

  /**
   * Close the session gracefully.
   * Sends FinishSession, waits for response, closes socket, stops agent.
   */
  async close(): Promise<void> {
    if (this.closed) {
      return; // Idempotent
    }
    this.closed = true;

    if (this.socket) {
      try {
        // Send FinishSession
        this.socket.write(encodeFinishSession());

        // Wait for FinishSessionOk
        const response = await TraceSession.readFrame(this.socket);
        const { opcode } = decodeFrame(response);
        if (opcode !== Resp.FINISH_SESSION_OK) {
          // Log but don't throw - we're closing anyway
          console.warn(`Expected FinishSessionOk, got 0x${opcode.toString(16)}`);
        }
      } catch (err) {
        // Log but don't throw - we're closing anyway
        console.warn(`Error during FinishSession: ${err}`);
      } finally {
        this.socket.destroy();
        this.socket = null;
      }
    }

    // Stop agent (wait for it to exit)
    this.discovery.stopAgent();
  }

  /**
   * Check if session is closed.
   */
  isClosed(): boolean {
    return this.closed;
  }
}

/**
 * Configuration for opening a trace session.
 */
export interface TraceConfig {
  /** Directory for .dtj session files */
  dataDir?: string;
  /** Producer name (max 32 bytes UTF-8) */
  producerName: string;
  /** Producer version (max 16 bytes UTF-8) */
  producerVersion: string;
  /** Optional explicit path to dtj-agent binary */
  agentPath?: string;
  /** Optional socket path for already-running agent */
  socketPath?: string;
  /** Optional session file name (default: session-<timestamp>.dtj) */
  sessionFileName?: string;
  /** Enable/disable tracing (default: true) */
  enabled?: boolean;
}