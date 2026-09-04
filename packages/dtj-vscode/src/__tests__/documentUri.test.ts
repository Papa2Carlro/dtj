import { describe, expect, it } from "vitest";

import { validateDtjDocumentUri } from "../documentUri";

describe("validateDtjDocumentUri", () => {
  it("accepts local absolute .dtj", () => {
    const res = validateDtjDocumentUri({
      scheme: "file",
      fsPath: "/tmp/session.dtj",
      path: "/tmp/session.dtj",
    });
    expect(res.ok).toBe(true);
  });

  it("rejects untitled/remote and non-dtj artifacts without implying spawn", () => {
    expect(
      validateDtjDocumentUri({ scheme: "untitled", fsPath: "", path: "a.dtj" }).ok,
    ).toBe(false);
    expect(
      validateDtjDocumentUri({
        scheme: "vscode-remote",
        fsPath: "/tmp/a.dtj",
        path: "/tmp/a.dtj",
      }).ok,
    ).toBe(false);
    for (const name of ["pack.dtgb", "pack.dtgb.age", "side.dtjp.json", "notes.txt"]) {
      const res = validateDtjDocumentUri({
        scheme: "file",
        fsPath: `/tmp/${name}`,
        path: `/tmp/${name}`,
      });
      expect(res.ok).toBe(false);
      if (!res.ok) expect(res.kind).toBe("UnsupportedDocument");
    }
  });
});
