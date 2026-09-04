import { describe, expect, it } from "vitest";

/** Mirrors provider gate: never spawn when workspace is untrusted. */
function maySpawnNative(isTrusted: boolean): boolean {
  return isTrusted === true;
}

describe("workspace trust gate", () => {
  it("blocks process spawn when untrusted", () => {
    expect(maySpawnNative(false)).toBe(false);
    expect(maySpawnNative(true)).toBe(true);
  });
});
