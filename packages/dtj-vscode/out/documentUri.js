"use strict";
var __createBinding = (this && this.__createBinding) || (Object.create ? (function(o, m, k, k2) {
    if (k2 === undefined) k2 = k;
    var desc = Object.getOwnPropertyDescriptor(m, k);
    if (!desc || ("get" in desc ? !m.__esModule : desc.writable || desc.configurable)) {
      desc = { enumerable: true, get: function() { return m[k]; } };
    }
    Object.defineProperty(o, k2, desc);
}) : (function(o, m, k, k2) {
    if (k2 === undefined) k2 = k;
    o[k2] = m[k];
}));
var __setModuleDefault = (this && this.__setModuleDefault) || (Object.create ? (function(o, v) {
    Object.defineProperty(o, "default", { enumerable: true, value: v });
}) : function(o, v) {
    o["default"] = v;
});
var __importStar = (this && this.__importStar) || (function () {
    var ownKeys = function(o) {
        ownKeys = Object.getOwnPropertyNames || function (o) {
            var ar = [];
            for (var k in o) if (Object.prototype.hasOwnProperty.call(o, k)) ar[ar.length] = k;
            return ar;
        };
        return ownKeys(o);
    };
    return function (mod) {
        if (mod && mod.__esModule) return mod;
        var result = {};
        if (mod != null) for (var k = ownKeys(mod), i = 0; i < k.length; i++) if (k[i] !== "default") __createBinding(result, mod, k[i]);
        __setModuleDefault(result, mod);
        return result;
    };
})();
Object.defineProperty(exports, "__esModule", { value: true });
exports.validateDtjDocumentUri = validateDtjDocumentUri;
const path = __importStar(require("node:path"));
/** Validate a VS Code URI-like object for plain local `.dtj` only. */
function validateDtjDocumentUri(uri) {
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
//# sourceMappingURL=documentUri.js.map