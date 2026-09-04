import * as path from "node:path";

export type DocumentValidation =
  | { ok: true; fsPath: string }
  | { ok: false; kind: "UnsupportedDocument"; message: string };

/** Validate a VS Code URI-like object for plain local `.dtj` only. */
export function validateDtjDocumentUri(uri: {
  scheme: string;
  fsPath: string;
  path: string;
}): DocumentValidation {
  if (uri.scheme !== "file") {
    return {
      ok: false,
      kind: "UnsupportedDocument",
      message: "Only local file: URIs are supported",
    };
  }
  const fsPath = uri.fsPath || uri.path;
  if (!fsPath || !path.isAbsolute(fsPath)) {
    return {
      ok: false,
      kind: "UnsupportedDocument",
      message: "Document path must be an absolute local file path",
    };
  }
  const base = path.basename(fsPath).toLowerCase();
  if (base.endsWith(".dtgb") || base.endsWith(".dtgb.age") || base.endsWith(".dtjp.json")) {
    return {
      ok: false,
      kind: "UnsupportedDocument",
      message: "Unsupported artifact; Session Explorer opens plain .dtj only",
    };
  }
  if (!base.endsWith(".dtj")) {
    return {
      ok: false,
      kind: "UnsupportedDocument",
      message: "Unsupported file type; expected .dtj",
    };
  }
  return { ok: true, fsPath };
}
