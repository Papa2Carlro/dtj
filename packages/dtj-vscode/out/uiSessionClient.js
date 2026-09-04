"use strict";
Object.defineProperty(exports, "__esModule", { value: true });
exports.UiSessionClient = void 0;
const protocol_1 = require("./protocol");
const processRunner_1 = require("./processRunner");
class UiSessionClient {
    executable;
    runner;
    active = null;
    constructor(executable, runner = processRunner_1.defaultProcessRunner) {
        this.executable = executable;
        this.runner = runner;
    }
    cancel() {
        this.active?.abort();
        this.active = null;
    }
    dispose() {
        this.cancel();
    }
    async hello() {
        return this.run("hello", (0, protocol_1.buildHelloArgv)(), (envelope) => {
            if (!envelope.ok) {
                return { ok: false, error: envelope.error };
            }
            const v = (0, protocol_1.validateHelloResult)(envelope.result);
            if (!v.ok)
                return { ok: false, error: { kind: v.kind, message: v.message } };
            return { ok: true, value: envelope.result };
        });
    }
    async summary(fsPath) {
        return this.run("summary", (0, protocol_1.buildSummaryArgv)(fsPath), (envelope) => {
            if (!envelope.ok)
                return { ok: false, error: envelope.error };
            const v = (0, protocol_1.validateSummaryResult)(envelope.result);
            if (!v.ok)
                return { ok: false, error: { kind: v.kind, message: v.message } };
            return { ok: true, value: envelope.result };
        });
    }
    async events(fsPath, query) {
        const built = (0, protocol_1.buildEventsArgv)(fsPath, query);
        if (!built.ok) {
            return { ok: false, error: { kind: built.kind, message: built.message } };
        }
        return this.run("events", built.args, (envelope) => {
            if (!envelope.ok)
                return { ok: false, error: envelope.error };
            const v = (0, protocol_1.validateEventsResult)(envelope.result);
            if (!v.ok)
                return { ok: false, error: { kind: v.kind, message: v.message } };
            return { ok: true, value: envelope.result };
        });
    }
    async event(fsPath, sequence) {
        const built = (0, protocol_1.buildEventArgv)(fsPath, sequence);
        if (!built.ok) {
            return { ok: false, error: { kind: built.kind, message: built.message } };
        }
        return this.run("event", built.args, (envelope) => {
            if (!envelope.ok)
                return { ok: false, error: envelope.error };
            const v = (0, protocol_1.validateEventDetailResult)(envelope.result);
            if (!v.ok)
                return { ok: false, error: { kind: v.kind, message: v.message } };
            return { ok: true, value: envelope.result };
        });
    }
    async run(operation, args, map) {
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
            const parsed = (0, protocol_1.parseUiSessionStdout)(proc.stdout, operation);
            if (!parsed.ok) {
                return { ok: false, error: { kind: parsed.kind, message: parsed.message } };
            }
            const mapped = map(parsed.envelope);
            if (!mapped.ok)
                return mapped;
            return { ok: true, value: mapped.value, envelope: parsed.envelope };
        }
        catch {
            return {
                ok: false,
                error: { kind: "NativeReaderUnavailable", message: "failed to spawn dtj binary" },
            };
        }
        finally {
            if (this.active === ac)
                this.active = null;
        }
    }
}
exports.UiSessionClient = UiSessionClient;
//# sourceMappingURL=uiSessionClient.js.map