"use strict";
/**
 * Resolve the plain `.dtj` journal that a `.traceql` sidecar targets.
 * Convention: `<name>.traceql` ↔ `<name>.dtj` in the same directory.
 */
Object.defineProperty(exports, "__esModule", { value: true });
exports.siblingDtjFsPath = siblingDtjFsPath;
function siblingDtjFsPath(traceqlFsPath) {
    if (!traceqlFsPath.endsWith(".traceql"))
        return null;
    return `${traceqlFsPath.slice(0, -".traceql".length)}.dtj`;
}
//# sourceMappingURL=resolveSessionJournal.js.map