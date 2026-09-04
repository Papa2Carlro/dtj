/**
 * Resolve the plain `.dtj` journal that a `.traceql` sidecar targets.
 * Convention: `<name>.traceql` ↔ `<name>.dtj` in the same directory.
 */

export function siblingDtjFsPath(traceqlFsPath: string): string | null {
  if (!traceqlFsPath.endsWith(".traceql")) return null;
  return `${traceqlFsPath.slice(0, -".traceql".length)}.dtj`;
}
