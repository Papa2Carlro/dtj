import * as crypto from "node:crypto";

/** Cryptographic CSP nonce; never persist or log. */
export function createCspNonce(): string {
  return crypto.randomBytes(16).toString("base64url");
}
