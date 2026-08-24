/**
 * Agent discovery and process management.
 * Handles finding dtj-agent binary and managing its lifecycle.
 */

import { spawn, ChildProcess } from "node:child_process";
import { existsSync, mkdirSync, rmSync } from "node:fs";
import { join } from "node:path";
import { tmpdir } from "node:os";
import { mkdtempSync } from "node:fs";
import { DTJAgentNotFoundError, DTJConnectionError } from "./errors.js";

export interface AgentDiscoveryOptions {
  agentPath?: string;
  socketPath?: string;
  dataDir?: string;
}

export class AgentDiscovery {
  private agentPath?: string;
  private socketPath?: string;
  private dataDir: string;
  private process: ChildProcess | null = null;
  private tempSocketDir: string | null = null;
  private warningEmitted = false;

  constructor(options: AgentDiscoveryOptions = {}) {
    this.agentPath = options.agentPath;
    this.socketPath = options.socketPath;
    this.dataDir = options.dataDir || join(process.cwd(), "traces");
  }

  /**
   * Find dtj-agent binary using discovery order:
   * 1. Explicit agentPath
   * 2. DTJ_AGENT_PATH environment variable
   * 3. PATH lookup for 'dtj-agent'
   */
  findAgent(): string | null {
    // 1. Explicit agent_path
    if (this.agentPath) {
      if (existsSync(this.agentPath)) {
        return this.agentPath;
      }
      // Don't throw - return null to trigger no-op session with warning
      return null;
    }

    // 2. DTJ_AGENT_PATH environment variable
    const envPath = process.env["DTJ_AGENT_PATH"];
    if (envPath && existsSync(envPath)) {
      return envPath;
    }

    // 3. PATH lookup
    const pathDirs = process.env["PATH"]?.split(":") || [];
    for (const dir of pathDirs) {
      const candidate = join(dir, "dtj-agent");
      if (existsSync(candidate)) {
        return candidate;
      }
    }

    return null;
  }

  /**
   * Start dtj-agent and return socket path.
   * If socketPath provided, don't start agent (connect to existing).
   */
  startAgent(): string {
    // If socket_path provided, don't start agent (connect to existing)
    if (this.socketPath) {
      return this.socketPath;
    }

    const agentBinary = this.findAgent();
    if (!agentBinary) {
      this.emitWarning();
      throw new DTJAgentNotFoundError(
        "dtj-agent not found. Install dtj-agent or set DTJ_AGENT_PATH. Tracing disabled."
      );
    }

    // Create temporary directory for socket
    this.tempSocketDir = mkdtempSync(join(tmpdir(), "dtj-agent-"));
    const socketPath = join(this.tempSocketDir, "agent.sock");

    // Ensure data_dir exists
    mkdirSync(this.dataDir, { recursive: true });

    // Start agent process
    try {
      this.process = spawn(agentBinary, ["--socket", socketPath, "--data-dir", this.dataDir], {
        stdio: "ignore",
        detached: false,
      });

      // Handle process errors
      this.process.on("error", (err) => {
        console.error(`dtj-agent process error: ${err.message}`);
      });

      this.process.on("exit", (code, signal) => {
        if (code !== 0 && code !== null) {
          console.error(`dtj-agent exited with code ${code}, signal ${signal}`);
        }
      });
    } catch (err) {
      this.cleanup();
      throw new DTJConnectionError(`Failed to start dtj-agent: ${err}`);
    }

    return socketPath;
  }

  /**
   * Stop the agent process and cleanup.
   */
  stopAgent(): void {
    if (this.process) {
      try {
        this.process.kill("SIGTERM");
        // Wait for process to exit (with timeout)
        const exitPromise = new Promise<void>((resolve) => {
          const timeout = setTimeout(() => {
            if (this.process) {
              this.process.kill("SIGKILL");
            }
            resolve();
          }, 5000);
          this.process?.on("exit", () => {
            clearTimeout(timeout);
            resolve();
          });
        });
        // Don't await - fire and forget for cleanup
        exitPromise.catch(() => {});
      } catch {
        // Ignore errors during cleanup
      } finally {
        this.process = null;
      }
    }

    this.cleanup();
  }

  /**
   * Cleanup temporary resources.
   */
  private cleanup(): void {
    if (this.tempSocketDir) {
      try {
        // Remove temp directory and contents
        rmSync(this.tempSocketDir, { recursive: true, force: true });
      } catch {
        // Ignore cleanup errors
      }
      this.tempSocketDir = null;
    }
  }

  /**
   * Emit single Node warning about disabled tracing.
   */
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
   * Get the socket path (for connecting to existing agent).
   */
  getSocketPath(): string | undefined {
    return this.socketPath;
  }
}