import {
  buildEventArgv,
  buildEventsArgv,
  buildHelloArgv,
  buildSummaryArgv,
  parseUiSessionStdout,
  validateEventDetailResult,
  validateEventsResult,
  validateHelloResult,
  validateSummaryResult,
  type EventsQuery,
  type UiEnvelope,
  type UiOperation,
} from "./protocol";
import { defaultProcessRunner, type ProcessRunner } from "./processRunner";

export type ClientError = {
  kind: string;
  message: string;
};

export type ClientResult<T> =
  | { ok: true; value: T; envelope: UiEnvelope }
  | { ok: false; error: ClientError };

export class UiSessionClient {
  private readonly executable: string;
  private readonly runner: ProcessRunner;
  private active: AbortController | null = null;

  constructor(executable: string, runner: ProcessRunner = defaultProcessRunner) {
    this.executable = executable;
    this.runner = runner;
  }

  cancel(): void {
    this.active?.abort();
    this.active = null;
  }

  dispose(): void {
    this.cancel();
  }

  async hello(): Promise<ClientResult<unknown>> {
    return this.run("hello", buildHelloArgv(), (envelope) => {
      if (!envelope.ok) {
        return { ok: false, error: envelope.error };
      }
      const v = validateHelloResult(envelope.result);
      if (!v.ok) return { ok: false, error: { kind: v.kind, message: v.message } };
      return { ok: true, value: envelope.result };
    });
  }

  async summary(fsPath: string): Promise<ClientResult<unknown>> {
    return this.run("summary", buildSummaryArgv(fsPath), (envelope) => {
      if (!envelope.ok) return { ok: false, error: envelope.error };
      const v = validateSummaryResult(envelope.result);
      if (!v.ok) return { ok: false, error: { kind: v.kind, message: v.message } };
      return { ok: true, value: envelope.result };
    });
  }

  async events(fsPath: string, query: EventsQuery): Promise<ClientResult<unknown>> {
    const built = buildEventsArgv(fsPath, query);
    if (!built.ok) {
      return { ok: false, error: { kind: built.kind, message: built.message } };
    }
    return this.run("events", built.args, (envelope) => {
      if (!envelope.ok) return { ok: false, error: envelope.error };
      const v = validateEventsResult(envelope.result);
      if (!v.ok) return { ok: false, error: { kind: v.kind, message: v.message } };
      return { ok: true, value: envelope.result };
    });
  }

  async event(fsPath: string, sequence: string): Promise<ClientResult<unknown>> {
    const built = buildEventArgv(fsPath, sequence);
    if (!built.ok) {
      return { ok: false, error: { kind: built.kind, message: built.message } };
    }
    return this.run("event", built.args, (envelope) => {
      if (!envelope.ok) return { ok: false, error: envelope.error };
      const v = validateEventDetailResult(envelope.result);
      if (!v.ok) return { ok: false, error: { kind: v.kind, message: v.message } };
      return { ok: true, value: envelope.result };
    });
  }

  private async run<T>(
    operation: UiOperation,
    args: string[],
    map: (
      envelope: UiEnvelope,
    ) => { ok: true; value: T } | { ok: false; error: ClientError },
  ): Promise<ClientResult<T>> {
    this.cancel();
    const ac = new AbortController();
    this.active = ac;
    try {
      const proc = await this.runner({
        executable: this.executable,
        args,
        signal: ac.signal,
      });
      if (ac.signal.aborted) {
        return { ok: false, error: { kind: "NativeCancelled", message: "request cancelled" } };
      }
      if (proc.timedOut) {
        return { ok: false, error: { kind: "NativeTimeout", message: "native process timed out" } };
      }
      if (proc.stdoutTruncated) {
        return {
          ok: false,
          error: { kind: "ResponseTooLarge", message: "stdout exceeded host cap" },
        };
      }
      // stderr is never treated as content; ignore after capture.
      void proc.stderr;
      if (proc.exitCode !== 0 && proc.exitCode !== 2 && proc.exitCode !== null && !proc.killed) {
        return {
          ok: false,
          error: { kind: "NativeProtocolError", message: "unexpected native exit code" },
        };
      }
      const parsed = parseUiSessionStdout(proc.stdout, operation);
      if (!parsed.ok) {
        return { ok: false, error: { kind: parsed.kind, message: parsed.message } };
      }
      const mapped = map(parsed.envelope);
      if (!mapped.ok) return mapped;
      return { ok: true, value: mapped.value, envelope: parsed.envelope };
    } catch {
      return {
        ok: false,
        error: { kind: "NativeReaderUnavailable", message: "failed to spawn dtj binary" },
      };
    } finally {
      if (this.active === ac) this.active = null;
    }
  }
}
