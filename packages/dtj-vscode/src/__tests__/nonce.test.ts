import { readFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";
import { describe, expect, it } from "vitest";

import { createCspNonce } from "../nonce";

const here = dirname(fileURLToPath(import.meta.url));

describe("createCspNonce", () => {
  it("uses crypto.randomBytes path and returns a base64url nonce", () => {
    const src = readFileSync(join(here, "../nonce.ts"), "utf8");
    expect(src).toContain("crypto.randomBytes");
    expect(src).not.toContain("Math.random");
    const a = createCspNonce();
    const b = createCspNonce();
    expect(a).toMatch(/^[A-Za-z0-9_-]+$/);
    expect(a.length).toBeGreaterThan(10);
    expect(a).not.toEqual(b);
  });
});
