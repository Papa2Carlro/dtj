import type { ExplorerUiState } from "./messages";

/** User-facing copy for native/host transport failures. */
export function formatNativeError(error: { kind: string; message: string }): string {
  switch (error.kind) {
    case "NativeTimeout":
      return "dtj timed out — try a smaller LIMIT or Refresh";
    case "ResponseTooLarge":
      return "Response too large for host cap — reduce LIMIT";
    case "NativeCancelled":
      return "Request cancelled";
    case "NativeProtocolMismatch":
      return error.message || "ui-session protocol mismatch";
    case "NativeReaderUnavailable":
      return error.message || "dtj binary unavailable";
    default:
      return error.message || error.kind;
  }
}

const SOFT_WHEN_BROWSING = new Set([
  "NativeTimeout",
  "ResponseTooLarge",
  "NativeCancelled",
  "NativeProtocolError",
  "NativeProtocolMismatch",
]);

/**
 * Fatal client errors wipe summary (initial open).
 * Soft errors keep the browse surface when a summary is already loaded.
 */
export function mapClientError(
  error: { kind: string; message: string },
  base: ExplorerUiState,
): ExplorerUiState {
  const message = formatNativeError(error);
  const phase =
    error.kind === "NativeProtocolMismatch"
      ? "NativeProtocolMismatch"
      : error.kind === "NativeTimeout"
        ? "NativeTimeout"
        : error.kind === "NativeReaderUnavailable"
          ? "NativeReaderUnavailable"
          : error.kind === "ResponseTooLarge"
            ? "NativeProtocolError"
            : "NativeProtocolError";
  return {
    ...base,
    busy: null,
    phase,
    errorKind: error.kind,
    message,
    events: undefined,
    detail: undefined,
    summary: undefined,
  };
}

export function mapReaderOrNativeError(
  error: { kind: string; message: string },
  base: ExplorerUiState,
): ExplorerUiState {
  const readerKinds = new Set([
    "ChecksumMismatch",
    "SequenceGap",
    "UnknownDictionaryId",
    "DuplicateDictionaryId",
    "UnsupportedVersion",
    "InvalidMagic",
    "InvalidEndian",
    "InvalidChunkMagic",
    "InvalidHeaderSize",
    "MalformedRecord",
    "UnknownTypeTag",
    "InvalidSeverity",
    "PayloadTooLarge",
    "LimitExceeded",
    "Io",
  ]);
  if (readerKinds.has(error.kind)) {
    return {
      ...base,
      busy: null,
      phase: "ReaderError",
      errorKind: error.kind,
      message: error.message,
      events: undefined,
      detail: undefined,
    };
  }
  return mapClientError(error, base);
}

/** Prefer soft banner when paging/detail fails after a successful summary. */
export function mapBrowseOpError(
  error: { kind: string; message: string },
  base: ExplorerUiState,
): ExplorerUiState {
  if (base.summary && SOFT_WHEN_BROWSING.has(error.kind)) {
    return {
      ...base,
      busy: null,
      phase: base.stale ? "stale" : "ready",
      errorKind: error.kind,
      message: formatNativeError(error),
    };
  }
  if (base.summary && error.kind === "EventNotFound") {
    return {
      ...base,
      busy: null,
      phase: base.stale ? "stale" : "ready",
      errorKind: error.kind,
      message: error.message,
      detail: undefined,
    };
  }
  return mapReaderOrNativeError(error, base);
}
