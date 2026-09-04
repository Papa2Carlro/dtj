"use strict";
Object.defineProperty(exports, "__esModule", { value: true });
exports.defaultProcessRunner = void 0;
const node_child_process_1 = require("node:child_process");
const constants_1 = require("./constants");
const defaultProcessRunner = (req) => {
    const timeoutMs = req.timeoutMs ?? constants_1.PROCESS_TIMEOUT_MS;
    const stdoutMax = req.stdoutMax ?? constants_1.STDOUT_MAX_BYTES;
    const stderrMax = req.stderrMax ?? constants_1.STDERR_MAX_BYTES;
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
        let child;
        try {
            child = (0, node_child_process_1.spawn)(req.executable, req.args, {
                shell: false,
                windowsHide: true,
                stdio: ["ignore", "pipe", "pipe"],
            });
        }
        catch (err) {
            reject(err);
            return;
        }
        const stdoutChunks = [];
        const stderrChunks = [];
        let stdoutLen = 0;
        let stderrLen = 0;
        let stdoutTruncated = false;
        let stderrTruncated = false;
        let killed = false;
        let timedOut = false;
        let settled = false;
        const finish = (exitCode) => {
            if (settled)
                return;
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
            }
            catch {
                /* ignore */
            }
            setTimeout(() => {
                try {
                    if (!child.killed)
                        child.kill("SIGKILL");
                }
                catch {
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
        child.stdout?.on("data", (chunk) => {
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
            }
            else {
                stdoutChunks.push(chunk);
                stdoutLen += chunk.length;
            }
        });
        child.stderr?.on("data", (chunk) => {
            if (stderrLen >= stderrMax) {
                stderrTruncated = true;
                return;
            }
            const room = stderrMax - stderrLen;
            if (chunk.length > room) {
                stderrChunks.push(chunk.subarray(0, room));
                stderrLen = stderrMax;
                stderrTruncated = true;
            }
            else {
                stderrChunks.push(chunk);
                stderrLen += chunk.length;
            }
        });
        child.on("error", (err) => {
            if (settled)
                return;
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
exports.defaultProcessRunner = defaultProcessRunner;
//# sourceMappingURL=processRunner.js.map