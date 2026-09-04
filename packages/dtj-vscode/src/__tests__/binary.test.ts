import * as fs from "node:fs";
import * as os from "node:os";
import * as path from "node:path";

import { describe, expect, it } from "vitest";

import { resolveDtjBinaryPath } from "../binary";

describe("resolveDtjBinaryPath", () => {
  it("rejects empty, relative, and PATH/env style values", () => {
    expect(resolveDtjBinaryPath("").ok).toBe(false);
    expect(resolveDtjBinaryPath("dtj").ok).toBe(false);
    expect(resolveDtjBinaryPath("./target/release/dtj").ok).toBe(false);
    expect(resolveDtjBinaryPath("~/dtj").ok).toBe(false);
  });

  it("accepts absolute executable file", () => {
    const dir = fs.mkdtempSync(path.join(os.tmpdir(), "dtj-bin-"));
    const bin = path.join(dir, "dtj");
    fs.writeFileSync(bin, "#!/bin/sh\n");
    fs.chmodSync(bin, 0o755);
    const res = resolveDtjBinaryPath(bin);
    expect(res.ok).toBe(true);
    if (res.ok) expect(res.executable).toBe(bin);
  });

  it("rejects missing and non-file paths", () => {
    expect(resolveDtjBinaryPath("/no/such/dtj-binary-xyz").ok).toBe(false);
    const dir = fs.mkdtempSync(path.join(os.tmpdir(), "dtj-dir-"));
    expect(resolveDtjBinaryPath(dir).ok).toBe(false);
  });
});
