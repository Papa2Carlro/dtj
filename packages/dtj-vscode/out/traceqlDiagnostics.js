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
exports.TraceqlDiagnosticsController = void 0;
const vscode = __importStar(require("vscode"));
const traceqlSubset_1 = require("./traceqlSubset");
/** Live Problems panel diagnostics for explorer TraceQL subset. */
class TraceqlDiagnosticsController {
    collection;
    constructor(context) {
        this.collection = vscode.languages.createDiagnosticCollection("dtg-traceql");
        context.subscriptions.push(this.collection);
        context.subscriptions.push(vscode.workspace.onDidChangeTextDocument((e) => {
            if (e.document.languageId === "dtg-traceql")
                this.refresh(e.document);
        }), vscode.workspace.onDidOpenTextDocument((doc) => {
            if (doc.languageId === "dtg-traceql")
                this.refresh(doc);
        }), vscode.workspace.onDidCloseTextDocument((doc) => {
            this.collection.delete(doc.uri);
        }));
        for (const doc of vscode.workspace.textDocuments) {
            if (doc.languageId === "dtg-traceql")
                this.refresh(doc);
        }
    }
    refresh(doc) {
        const parsed = (0, traceqlSubset_1.parseTraceqlSubset)(doc.getText());
        if (parsed.ok) {
            this.collection.set(doc.uri, []);
            return;
        }
        const range = doc.lineCount > 0
            ? doc.lineAt(0).range
            : new vscode.Range(0, 0, 0, 0);
        this.collection.set(doc.uri, [
            new vscode.Diagnostic(range, parsed.message, vscode.DiagnosticSeverity.Error),
        ]);
    }
}
exports.TraceqlDiagnosticsController = TraceqlDiagnosticsController;
//# sourceMappingURL=traceqlDiagnostics.js.map