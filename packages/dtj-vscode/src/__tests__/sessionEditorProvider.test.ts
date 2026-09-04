import { describe, expect, it } from "vitest";

import { validateDtjDocumentUri } from "../documentUri";

/**
 * P1-A contract: openCustomDocument rejects non-file URIs before session/spawn.
 * Provider wiring calls validateDtjDocumentUri with the real uri.scheme.
 */
describe("openCustomDocument URI gate", () => {
  it("rejects non-file schemes before any spawn state", () => {
    for (const scheme of ["untitled", "vscode-remote", "untitled-dtj"]) {
      const res = validateDtjDocumentUri({
        scheme,
        fsPath: "/tmp/a.dtj",
        path: "/tmp/a.dtj",
      });
      expect(res.ok).toBe(false);
      if (!res.ok) expect(res.kind).toBe("UnsupportedDocument");
    }
  });

  it("reload must use original scheme — hardcoding file would wrongly accept remote", () => {
    const remote = validateDtjDocumentUri({
      scheme: "vscode-remote",
      fsPath: "/tmp/a.dtj",
      path: "/tmp/a.dtj",
    });
    expect(remote.ok).toBe(false);
    // Contrived: if scheme were rewritten to file, validation would pass — that is the bug.
    const rewritten = validateDtjDocumentUri({
      scheme: "file",
      fsPath: "/tmp/a.dtj",
      path: "/tmp/a.dtj",
    });
    expect(rewritten.ok).toBe(true);
  });
});
