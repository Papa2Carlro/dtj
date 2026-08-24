/**
 * Custom exceptions for dtj-sdk.
 */

export class DTJError extends Error {
  constructor(message: string) {
    super(message);
    this.name = "DTJError";
  }
}

export class DTJProtocolError extends DTJError {
  public readonly opcode: number | null;

  constructor(message: string, opcode: number | null = null) {
    super(message);
    this.name = "DTJProtocolError";
    this.opcode = opcode;
  }
}

export class DTJConnectionError extends DTJError {
  constructor(message: string) {
    super(message);
    this.name = "DTJConnectionError";
  }
}

export class DTJAgentNotFoundError extends DTJError {
  constructor(message: string) {
    super(message);
    this.name = "DTJAgentNotFoundError";
  }
}

export class DTJValueError extends DTJError {
  constructor(message: string) {
    super(message);
    this.name = "DTJValueError";
  }
}

export class DTJSessionError extends DTJError {
  constructor(message: string) {
    super(message);
    this.name = "DTJSessionError";
  }
}