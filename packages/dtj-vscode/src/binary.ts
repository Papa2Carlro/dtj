import * as fs from "node:fs";
import * as path from "node:path";

import { BINARY_SETTING } from "./constants";

export type BinaryResolution =
  | { ok: true; executable: string }
  | { ok: false; kind: "NativeReaderUnavailable"; message: string };

export function resolveDtjBinaryPath(
  configured: string | undefined,
  opts?: { accessSync?: typeof fs.accessSync; statSync?: typeof fs.statSync },
): BinaryResolution {
  const accessSync = opts?.accessSync ?? fs.accessSync;
  const statSync = opts?.statSync ?? fs.statSync;
  const raw = (configured ?? "").trim();
  if (!raw) {
    return {
      ok: false,
      kind: "NativeReaderUnavailable",
      message: `Set absolute setting ${BINARY_SETTING} to a prebuilt dtj binary`,
    };
  }
  if (!path.isAbsolute(raw)) {
    return {
      ok: false,
      kind: "NativeReaderUnavailable",
      message: "dtjBinaryPath must be an absolute filesystem path",
    };
  }
  let st: fs.Stats;
  try {
    st = statSync(raw);
  } catch {
    return {
      ok: false,
      kind: "NativeReaderUnavailable",
      message: "dtjBinaryPath does not exist or is unreadable",
    };
  }
  if (!st.isFile()) {
    return {
      ok: false,
      kind: "NativeReaderUnavailable",
      message: "dtjBinaryPath must point to a regular file",
    };
  }
  try {
    accessSync(raw, fs.constants.F_OK | fs.constants.X_OK);
  } catch {
    // Windows often lacks a meaningful X_OK; require F_OK at minimum.
    try {
      accessSync(raw, fs.constants.F_OK);
      if (process.platform !== "win32") {
        return {
          ok: false,
          kind: "NativeReaderUnavailable",
          message: "dtjBinaryPath is not executable",
        };
      }
    } catch {
      return {
        ok: false,
        kind: "NativeReaderUnavailable",
        message: "dtjBinaryPath is not readable",
      };
    }
  }
  return { ok: true, executable: raw };
}
