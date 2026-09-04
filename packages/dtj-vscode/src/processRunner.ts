import { spawn, type ChildProcess } from "node:child_process";

import { PROCESS_TIMEOUT_MS, STDERR_MAX_BYTES, STDOUT_MAX_BYTES } from "./constants";

export type ProcessRunRequest = {
  executable: string;
  args: string[];
  timeoutMs?: number;
  stdoutMax?: number;
  stderrMax?: number;
  signal?: AbortSignal;
};

export type ProcessRunResult = {
  stdout: string;
  stderr: string;
  exitCode: number | null;
  killed: boolean;
  timedOut: boolean;
  stdoutTruncated: boolean;
  stderrTruncated: boolean;
};

export type ProcessRunner = (req: ProcessRunRequest) => Promise<ProcessRunResult>;

export const defaultProcessRunner: ProcessRunner = (req) => {
  const timeoutMs = req.timeoutMs ?? PROCESS_TIMEOUT_MS;
  const stdoutMax = req.stdoutMax ?? STDOUT_MAX_BYTES;
  const stderrMax = req.stderrMax ?? STDERR_MAX_BYTES;

  return new Promise((resolve, reject) => {
    if (req.signal?.aborted) {
      resolve({
        stdout: "",
        stderr: "",
        exitCode: null,
        killed: true,
        timedOut: false,
        stdoutTruncated: false,
        stderrTruncated: false,
      });
      return;
    }

    let child: ChildProcess;
    try {
      child = spawn(req.executable, req.args, {
        shell: false,
        windowsHide: true,
        stdio: ["ignore", "pipe", "pipe"],
      });
    } catch (err) {
      reject(err);
      return;
    }

    const stdoutChunks: Buffer[] = [];
    const stderrChunks: Buffer[] = [];
    let stdoutLen = 0;
    let stderrLen = 0;
    let stdoutTruncated = false;
    let stderrTruncated = false;
    let killed = false;
    let timedOut = false;
    let settled = false;

    const finish = (exitCode: number | null) => {
      if (settled) return;
      settled = true;
      clearTimeout(timer);
      req.signal?.removeEventListener("abort", onAbort);
      resolve({
        stdout: Buffer.concat(stdoutChunks).toString("utf8"),
        stderr: Buffer.concat(stderrChunks).toString("utf8"),
        exitCode,
        killed,
        timedOut,
        stdoutTruncated,
        stderrTruncated,
      });
    };

    const killChild = () => {
      killed = true;
      try {
        child.kill("SIGTERM");
      } catch {
        /* ignore */
      }
      setTimeout(() => {
        try {
          if (!child.killed) child.kill("SIGKILL");
        } catch {
          /* ignore */
        }
      }, 500).unref?.();
    };

    const onAbort = () => {
      killChild();
    };
    req.signal?.addEventListener("abort", onAbort, { once: true });

    const timer = setTimeout(() => {
      timedOut = true;
      killChild();
    }, timeoutMs);
    timer.unref?.();

    child.stdout?.on("data", (chunk: Buffer) => {
      if (stdoutLen >= stdoutMax) {
        stdoutTruncated = true;
        return;
      }
      const room = stdoutMax - stdoutLen;
      if (chunk.length > room) {
        stdoutChunks.push(chunk.subarray(0, room));
        stdoutLen = stdoutMax;
        stdoutTruncated = true;
        killChild();
      } else {
        stdoutChunks.push(chunk);
        stdoutLen += chunk.length;
      }
    });

    child.stderr?.on("data", (chunk: Buffer) => {
      if (stderrLen >= stderrMax) {
        stderrTruncated = true;
        return;
      }
      const room = stderrMax - stderrLen;
      if (chunk.length > room) {
        stderrChunks.push(chunk.subarray(0, room));
        stderrLen = stderrMax;
        stderrTruncated = true;
      } else {
        stderrChunks.push(chunk);
        stderrLen += chunk.length;
      }
    });

    child.on("error", (err) => {
      if (settled) return;
      settled = true;
      clearTimeout(timer);
      req.signal?.removeEventListener("abort", onAbort);
      reject(err);
    });

    child.on("close", (code) => {
      finish(code);
    });
  });
};
